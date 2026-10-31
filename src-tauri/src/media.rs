//! What's inside a media file, from ffprobe.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::tools::{command, run};

/// A probe longer than this means a damaged file or a drive that stopped answering.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Disposition {
    #[serde(default)]
    pub attached_pic: u8,
}

/// One ffprobe stream. ffprobe reports some numbers as strings; helpers parse them.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stream {
    pub index: u32,
    pub codec_type: Option<String>,
    pub codec_name: Option<String>,
    pub profile: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<String>,
    pub r_frame_rate: Option<String>,
    pub avg_frame_rate: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub sample_rate: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    #[serde(default)]
    pub disposition: Disposition,
}

impl Stream {
    pub fn is_video(&self) -> bool {
        // Cover art in MKV/MP3 shows up as a one-frame "video" stream.
        self.codec_type.as_deref() == Some("video") && self.disposition.attached_pic == 0
    }

    pub fn is_audio(&self) -> bool {
        self.codec_type.as_deref() == Some("audio")
    }

    pub fn is_subtitle(&self) -> bool {
        self.codec_type.as_deref() == Some("subtitle")
    }

    pub fn codec(&self) -> &str {
        self.codec_name.as_deref().unwrap_or("unknown")
    }

    /// The rate the container declares, e.g. 60.
    pub fn nominal_fps(&self) -> Option<f64> {
        self.r_frame_rate.as_deref().and_then(parse_ratio)
    }

    /// Frames divided by duration. Differs from nominal on variable-frame-rate files.
    pub fn average_fps(&self) -> Option<f64> {
        self.avg_frame_rate.as_deref().and_then(parse_ratio)
    }

    pub fn bit_depth(&self) -> Option<u32> {
        if let Some(bits) = self
            .bits_per_raw_sample
            .as_deref()
            .and_then(|b| b.parse().ok())
        {
            return Some(bits);
        }
        let pix = self.pix_fmt.as_deref()?;
        Some(if pix.contains("12") {
            12
        } else if pix.contains("10") {
            10
        } else {
            8
        })
    }

    /// PQ (HDR10) or HLG.
    pub fn is_hdr(&self) -> bool {
        matches!(
            self.color_transfer.as_deref(),
            Some("smpte2084" | "arib-std-b67")
        )
    }

    pub fn sample_rate_hz(&self) -> Option<u32> {
        self.sample_rate.as_deref().and_then(|s| s.parse().ok())
    }
}

/// How a codec is usually written, e.g. "h264" → "H.264".
pub fn codec_label(codec: &str) -> String {
    match codec {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "vp8" => "VP8".into(),
        "prores" => "ProRes".into(),
        "dnxhd" => "DNxHD/HR".into(),
        "mpeg4" => "MPEG-4".into(),
        "aac" => "AAC".into(),
        "opus" => "Opus".into(),
        "vorbis" => "Vorbis".into(),
        "mp3" => "MP3".into(),
        "flac" => "FLAC".into(),
        "ac3" => "AC-3".into(),
        "eac3" => "E-AC-3".into(),
        "mjpeg" => "JPEG".into(),
        "png" => "PNG".into(),
        "webp" => "WebP".into(),
        c if c.starts_with("pcm_") => "PCM".into(),
        c => c.to_uppercase(),
    }
}

/// "60000/1001" → 59.94. Zero denominators and "0/0" give None.
pub fn parse_ratio(s: &str) -> Option<f64> {
    let (n, d) = s.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d)
}

#[derive(Debug, Deserialize)]
struct Format {
    duration: Option<String>,
    format_name: Option<String>,
    format_long_name: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub duration: Option<f64>,
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub size_bytes: Option<u64>,
    pub bit_rate: Option<u64>,
    pub tags: BTreeMap<String, String>,
    pub streams: Vec<Stream>,
    /// Variable frame rate. Filled in by Inspect, which samples timestamps; None if not checked.
    pub vfr: Option<bool>,
    /// A C2PA manifest is present. Filled in by Inspect; None if not checked or unknown.
    pub c2pa: Option<bool>,
    /// For images: what exiftool found (location, camera…). Filled in by Inspect.
    pub image: Option<crate::exif::ImageMeta>,
}

impl MediaInfo {
    pub fn video(&self) -> impl Iterator<Item = &Stream> {
        self.streams.iter().filter(|s| s.is_video())
    }

    pub fn audio(&self) -> impl Iterator<Item = &Stream> {
        self.streams.iter().filter(|s| s.is_audio())
    }

    pub fn subtitles(&self) -> impl Iterator<Item = &Stream> {
        self.streams.iter().filter(|s| s.is_subtitle())
    }

    /// Still images come through ffprobe's image demuxers ("png_pipe", "image2"…),
    /// except AVIF and HEIC, which are MP4-style files that say so in their brand.
    pub fn is_image(&self) -> bool {
        let still_brand = self.tags.get("major_brand").is_some_and(|b| {
            matches!(
                b.trim(),
                "avif" | "heic" | "heix" | "heim" | "heis" | "mif1"
            )
        });
        self.format_name.as_deref().is_some_and(|f| {
            f == "image2" || f.ends_with("_pipe") || (still_brand && self.is_mp4_family())
        })
    }

    /// Already in a container Premiere opens (MP4 / MOV family).
    pub fn is_mp4_family(&self) -> bool {
        self.format_name
            .as_deref()
            .is_some_and(|f| f.split(',').any(|n| matches!(n, "mp4" | "mov" | "m4a")))
    }
}

pub fn probe(ffprobe: &Path, file: &Path) -> Result<MediaInfo, String> {
    let out = run(
        command(ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                // A playlist-like file can name web addresses; this reads one file.
                "-protocol_whitelist",
                "file",
            ])
            .arg(file),
        PROBE_TIMEOUT,
    )
    .map_err(|e| match e.kind() {
        std::io::ErrorKind::TimedOut => "Reading this file took over a minute, so it was stopped. It may be damaged, or on a drive that stopped answering.".to_string(),
        _ => format!("Couldn't start ffprobe: {e}"),
    })?;
    if !out.status.success() {
        // ffprobe says "<full path>: <reason>"; the path is noise in a message.
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().last().unwrap_or("unknown error").trim();
        let reason = last.rsplit(": ").next().unwrap_or(last);
        return Err(format!(
            "This isn't a video, audio or image file the toolkit can read ({reason})."
        ));
    }
    parse(&out.stdout)
}

fn parse(json: &[u8]) -> Result<MediaInfo, String> {
    let raw: ProbeOutput =
        serde_json::from_slice(json).map_err(|e| format!("Couldn't read ffprobe's answer: {e}"))?;
    let mut info = MediaInfo {
        streams: raw.streams,
        ..Default::default()
    };
    if let Some(f) = raw.format {
        info.duration = f.duration.and_then(|d| d.parse().ok());
        info.format_name = f.format_name;
        info.format_long_name = f.format_long_name;
        info.size_bytes = f.size.and_then(|s| s.parse().ok());
        info.bit_rate = f.bit_rate.and_then(|s| s.parse().ok());
        info.tags = f.tags;
    }
    Ok(info)
}

/// Samples video packet timestamps from the first `seconds` and decides whether
/// frames arrive at an even pace. None when there's too little to judge.
pub fn sample_vfr(ffprobe: &Path, file: &Path, seconds: u32) -> Option<bool> {
    let out = run(
        command(ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-read_intervals",
                &format!("%+{seconds}"),
                "-show_entries",
                "packet=pts_time",
                "-of",
                "csv=p=0",
            ])
            .arg(file),
        PROBE_TIMEOUT,
    )
    .ok()?;
    let times: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    is_variable(times)
}

/// Packets arrive in decode order, so sort first. A file is variable when more than
/// 2% of frame gaps are more than 15% away from the typical gap. The tolerance
/// absorbs MKV's millisecond rounding (16/17 ms at 60 fps).
fn is_variable(mut times: Vec<f64>) -> Option<bool> {
    times.sort_by(f64::total_cmp);
    times.dedup();
    let mut gaps: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.len() < 30 {
        return None;
    }
    let mut sorted = gaps.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    if median <= 0.0 {
        return None;
    }
    gaps.retain(|g| (g - median).abs() > median * 0.15);
    Some(gaps.len() as f64 / sorted.len() as f64 > 0.02)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_json() {
        let json = br#"{
          "streams": [
            {"index": 0, "codec_type": "video", "codec_name": "vp9", "width": 2560, "height": 1440,
             "r_frame_rate": "60/1", "avg_frame_rate": "60000/1001", "pix_fmt": "yuv420p10le",
             "disposition": {"attached_pic": 0}},
            {"index": 1, "codec_type": "audio", "codec_name": "opus", "sample_rate": "48000", "channels": 2,
             "tags": {"language": "eng"}},
            {"index": 2, "codec_type": "video", "codec_name": "mjpeg", "disposition": {"attached_pic": 1}}
          ],
          "format": {"duration": "11564.250000", "format_name": "matroska,webm", "size": "15248112640",
                     "tags": {"encoder": "OBS"}}
        }"#;
        let info = parse(json).unwrap();
        assert_eq!(info.duration, Some(11564.25));
        assert_eq!(info.size_bytes, Some(15_248_112_640));
        assert_eq!(info.video().count(), 1);
        let v = info.video().next().unwrap();
        assert_eq!(v.nominal_fps(), Some(60.0));
        assert!((v.average_fps().unwrap() - 59.94).abs() < 0.01);
        assert_eq!(v.bit_depth(), Some(10));
        assert_eq!(info.audio().next().unwrap().codec(), "opus");
        assert!(!info.is_mp4_family());
        assert!(!info.is_image());
    }

    #[test]
    fn ratio_edge_cases() {
        assert_eq!(parse_ratio("0/0"), None);
        assert_eq!(parse_ratio("30/0"), None);
        assert_eq!(parse_ratio("nonsense"), None);
    }

    #[test]
    fn detects_variable_frame_rate() {
        // 60 fps in MKV: millisecond timestamps alternate 16/17 ms. Constant.
        let mkv: Vec<f64> = (0..600)
            .map(|i| (i as f64 * 1000.0 / 60.0).round() / 1000.0)
            .collect();
        assert_eq!(is_variable(mkv), Some(false));

        // Phone-style: mostly 30 fps with frequent stretches at 24 fps.
        let mut t = 0.0;
        let phone: Vec<f64> = (0..600)
            .map(|i| {
                t += if i % 10 < 3 { 1.0 / 24.0 } else { 1.0 / 30.0 };
                t
            })
            .collect();
        assert_eq!(is_variable(phone), Some(true));

        assert_eq!(is_variable(vec![0.0, 0.1]), None);
    }
}
