//! Detects a C2PA (Content Credentials) manifest by walking the file's own
//! structure, without reading the media data:
//!   MP4/MOV  top-level `uuid` box with the C2PA extended type
//!   JPEG     APP11 segments carrying a JUMBF box labelled "c2pa"
//!   PNG      a `caBX` chunk
//!   WebP     a `C2PA` RIFF chunk

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum C2pa {
    Present,
    Absent,
    /// A format this check doesn't understand, or the file couldn't be read.
    Unknown,
}

/// UUID d8fec3d6-1b0e-483c-9297-5828877ec481 from the C2PA spec's BMFF binding.
const BMFF_UUID: [u8; 16] = [
    0xd8, 0xfe, 0xc3, 0xd6, 0x1b, 0x0e, 0x48, 0x3c, 0x92, 0x97, 0x58, 0x28, 0x87, 0x7e, 0xc4, 0x81,
];

/// Stops walking pathological files.
const MAX_ENTRIES: usize = 100_000;

pub fn detect(path: &Path) -> C2pa {
    let Ok(file) = File::open(path) else {
        return C2pa::Unknown;
    };
    detect_in(&mut BufReader::new(file)).unwrap_or(C2pa::Unknown)
}

fn detect_in<R: Read + Seek>(r: &mut R) -> std::io::Result<C2pa> {
    let mut head = [0u8; 12];
    r.read_exact(&mut head)?;
    r.seek(SeekFrom::Start(0))?;
    if head[..2] == [0xFF, 0xD8] {
        scan_jpeg(r)
    } else if head[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        scan_png(r)
    } else if &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        scan_webp(r)
    } else if &head[4..8] == b"ftyp" {
        scan_bmff(r)
    } else {
        Ok(C2pa::Unknown)
    }
}

fn read_u32_be<R: Read>(r: &mut R) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

fn scan_bmff<R: Read + Seek>(r: &mut R) -> std::io::Result<C2pa> {
    let end = r.seek(SeekFrom::End(0))?;
    let mut pos = 0u64;
    for _ in 0..MAX_ENTRIES {
        if pos + 8 > end {
            break;
        }
        r.seek(SeekFrom::Start(pos))?;
        let size32 = read_u32_be(r)?;
        let mut kind = [0u8; 4];
        r.read_exact(&mut kind)?;
        let (size, header) = match size32 {
            0 => (end - pos, 8), // box runs to the end of the file
            1 => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b)?;
                (u64::from_be_bytes(b), 16)
            }
            n => (n as u64, 8),
        };
        if &kind == b"uuid" && size >= header + 16 {
            let mut ext = [0u8; 16];
            r.read_exact(&mut ext)?;
            if ext == BMFF_UUID {
                return Ok(C2pa::Present);
            }
        }
        if size < header {
            break; // corrupt box
        }
        pos = pos.saturating_add(size);
    }
    Ok(C2pa::Absent)
}

fn scan_jpeg<R: Read + Seek>(r: &mut R) -> std::io::Result<C2pa> {
    r.seek(SeekFrom::Start(2))?;
    for _ in 0..MAX_ENTRIES {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        if b[0] != 0xFF {
            break;
        }
        let mut marker = 0xFFu8;
        while marker == 0xFF {
            r.read_exact(&mut b)?;
            marker = b[0];
        }
        match marker {
            0xDA | 0xD9 => break, // image data starts / ends: metadata is all before this
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        let mut len = [0u8; 2];
        r.read_exact(&mut len)?;
        let len = u16::from_be_bytes(len) as usize;
        if len < 2 {
            break;
        }
        if marker == 0xEB {
            let mut data = vec![0u8; len - 2];
            r.read_exact(&mut data)?;
            if contains(&data, b"jumb") && contains(&data, b"c2pa") {
                return Ok(C2pa::Present);
            }
        } else {
            r.seek(SeekFrom::Current(len as i64 - 2))?;
        }
    }
    Ok(C2pa::Absent)
}

fn scan_png<R: Read + Seek>(r: &mut R) -> std::io::Result<C2pa> {
    r.seek(SeekFrom::Start(8))?;
    for _ in 0..MAX_ENTRIES {
        let Ok(len) = read_u32_be(r) else { break };
        let mut kind = [0u8; 4];
        r.read_exact(&mut kind)?;
        match &kind {
            b"caBX" => return Ok(C2pa::Present),
            b"IEND" => break,
            _ => {}
        }
        r.seek(SeekFrom::Current(len as i64 + 4))?; // data + CRC
    }
    Ok(C2pa::Absent)
}

fn scan_webp<R: Read + Seek>(r: &mut R) -> std::io::Result<C2pa> {
    r.seek(SeekFrom::Start(12))?;
    for _ in 0..MAX_ENTRIES {
        let mut kind = [0u8; 4];
        if r.read_exact(&mut kind).is_err() {
            break;
        }
        let mut len = [0u8; 4];
        r.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as i64;
        if &kind == b"C2PA" {
            return Ok(C2pa::Present);
        }
        r.seek(SeekFrom::Current(len + (len & 1)))?; // chunks are padded to even sizes
    }
    Ok(C2pa::Absent)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bmff_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(kind);
        b.extend_from_slice(body);
        b
    }

    fn check(bytes: Vec<u8>) -> C2pa {
        detect_in(&mut Cursor::new(bytes)).unwrap()
    }

    #[test]
    fn mp4() {
        let mut clean = bmff_box(b"ftyp", b"isom\0\0\0\0isom");
        clean.extend(bmff_box(b"moov", &[0; 32]));
        clean.extend(bmff_box(b"mdat", &[0; 64]));
        assert_eq!(check(clean.clone()), C2pa::Absent);

        let mut signed = clean;
        let mut body = BMFF_UUID.to_vec();
        body.extend_from_slice(b"\0\0\0\0manifest");
        signed.extend(bmff_box(b"uuid", &body));
        assert_eq!(check(signed), C2pa::Present);

        // An unrelated uuid box isn't C2PA.
        let mut other = bmff_box(b"ftyp", b"isom");
        other.extend(bmff_box(b"uuid", &[7; 20]));
        assert_eq!(check(other), C2pa::Absent);
    }

    #[test]
    fn jpeg() {
        let segment = |marker: u8, data: &[u8]| {
            let mut s = vec![0xFF, marker];
            s.extend(((data.len() + 2) as u16).to_be_bytes());
            s.extend_from_slice(data);
            s
        };
        let mut clean = vec![0xFF, 0xD8];
        clean.extend(segment(0xE1, b"Exif\0\0data"));
        clean.extend([0xFF, 0xDA, 0, 2, 1, 2, 3]);
        assert_eq!(check(clean), C2pa::Absent);

        let mut signed = vec![0xFF, 0xD8];
        signed.extend(segment(
            0xEB,
            b"JP\0\x01\0\0\0\x01\0\0\0\x20jumbjumd...c2pa",
        ));
        signed.extend([0xFF, 0xDA, 0, 2]);
        assert_eq!(check(signed), C2pa::Present);
    }

    #[test]
    fn png() {
        let chunk = |kind: &[u8; 4], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            c.extend([0; 4]);
            c
        };
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend(chunk(b"IHDR", &[0; 13]));
        let mut signed = png.clone();
        png.extend(chunk(b"IEND", &[]));
        assert_eq!(check(png), C2pa::Absent);
        signed.extend(chunk(b"caBX", b"jumb"));
        signed.extend(chunk(b"IEND", &[]));
        assert_eq!(check(signed), C2pa::Present);
    }

    #[test]
    fn webp_and_unknown() {
        let mut webp = b"RIFF\0\0\0\0WEBP".to_vec();
        webp.extend(b"VP8 \x03\0\0\0abc\0");
        assert_eq!(check(webp.clone()), C2pa::Absent);
        webp.extend(b"C2PA\x04\0\0\0jumb");
        assert_eq!(check(webp), C2pa::Present);

        assert_eq!(check(b"\x1aE\xdf\xa3 matroska....".to_vec()), C2pa::Unknown);
    }
}
