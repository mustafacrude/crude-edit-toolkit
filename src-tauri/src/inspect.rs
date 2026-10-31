//! Inspect: probes a dropped file and explains, in plain words, what Premiere
//! will struggle with and which module fixes it. Powers the drop-first Home.

use std::path::Path;

use serde::Serialize;
use tauri::AppHandle;

use crate::c2pa::{self, C2pa};
use crate::exif;
use crate::media::{self, MediaInfo, Stream, codec_label};
use crate::modules::{self, JobInput};
use crate::tools::Tool;

pub const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "mov", "webm", "m4v", "avi", "ts", "flv"];
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "webp", "avif", "heic", "heif", "svg", "jxl", "tif", "tiff",
    "bmp",
];
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "flac", "opus", "ogg", "aif", "aiff",
];

/// Codecs Premiere edits without trouble.
const FRIENDLY_VIDEO: &[&str] = &["h264", "hevc", "prores", "dnxhd", "mpeg2video"];
const FRIENDLY_AUDIO: &[&str] = &["aac", "mp3", "ac3", "eac3"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Audio,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Ok,
    Info,
    Warn,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub label: String,
    /// One plain sentence: why it matters.
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Detail {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionView {
    pub module: &'static str,
    pub name: &'static str,
    pub reason: String,
    pub recommended: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub path: String,
    pub name: String,
    pub kind: Kind,
    pub duration: Option<f64>,
    pub size_bytes: Option<u64>,
    /// e.g. "VP9 · 2560×1440 · 60 fps"
    pub headline: String,
    /// What's wrong, in a sentence or two, or that nothing is.
    pub summary: String,
    pub findings: Vec<Finding>,
    /// The Inspect table.
    pub details: Vec<Detail>,
    pub c2pa: C2pa,
    pub suggestions: Vec<SuggestionView>,
}

pub fn inspect(app: &AppHandle, temp_dir: &Path, raw: &str) -> Result<Inspection, String> {
    let input = JobInput::parse(raw)?;
    let path = input.file().ok_or("Inspect works on files.")?;
    let ffprobe = Tool::Ffprobe.path(app)?;
    let mut info = media::probe(&ffprobe, path)?;
    info.size_bytes = info
        .size_bytes
        .or_else(|| std::fs::metadata(path).ok().map(|m| m.len()));

    let kind = if info.is_image() {
        Kind::Image
    } else if info.video().next().is_some() {
        Kind::Video
    } else if info.audio().next().is_some() {
        Kind::Audio
    } else {
        return Err("There's no video, audio or image inside this file.".into());
    };
    if kind == Kind::Video {
        info.vfr = media::sample_vfr(&ffprobe, path, 20).or_else(|| rate_mismatch(&info));
    }
    if kind == Kind::Image {
        // exiftool sees what ffprobe can't: location, camera, editing software.
        info.image = Tool::Exiftool
            .path(app)
            .ok()
            .and_then(|exiftool| exif::image_meta(&exiftool, temp_dir, path).ok());
    }
    let mut c2pa = c2pa::detect(path);
    if c2pa != C2pa::Present && info.image.as_ref().is_some_and(|m| m.c2pa) {
        c2pa = C2pa::Present;
    }
    info.c2pa = match c2pa {
        C2pa::Present => Some(true),
        C2pa::Absent => Some(false),
        C2pa::Unknown => None,
    };

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let findings = findings(&info, kind, c2pa, &ext);
    let summary = summary(&findings, kind);
    let mut suggested: Vec<_> = modules::all()
        .iter()
        .filter_map(|m| m.suggest(&info).map(|s| (m, s)))
        .collect();
    suggested.sort_by_key(|(_, s)| std::cmp::Reverse(s.priority));
    let suggestions = suggested
        .into_iter()
        .enumerate()
        .map(|(i, (m, s))| SuggestionView {
            module: m.id(),
            name: m.name(),
            recommended: i == 0 && s.priority >= 50,
            reason: s.reason,
        })
        .collect();

    Ok(Inspection {
        path: path.display().to_string(),
        name: input.short_name(),
        kind,
        duration: info.duration.filter(|_| kind != Kind::Image),
        size_bytes: info.size_bytes,
        headline: headline(&info, kind),
        summary,
        details: details(&info, c2pa, &ext),
        findings,
        c2pa,
        suggestions,
    })
}

/// Fallback when timestamps can't be sampled: declared vs measured rate differ by over 1%.
fn rate_mismatch(info: &MediaInfo) -> Option<bool> {
    let v = info.video().next()?;
    let (nominal, avg) = (v.nominal_fps()?, v.average_fps()?);
    Some((nominal - avg).abs() / nominal > 0.01)
}

/// Expands dropped folders into the media files directly inside them.
/// `kind` "image" or "video" keeps only those (for the tool screens, which take
/// big folders); otherwise every media type, capped for Home's inspections.
/// The media files in what was dropped, and how many more there were than the limit.
#[derive(Debug, Default, serde::Serialize)]
pub struct Expanded {
    pub files: Vec<String>,
    /// Files past the limit that were left out; the UI says so.
    pub skipped: usize,
}

pub fn expand(paths: Vec<String>, kind: Option<&str>) -> Expanded {
    let (lists, limit): (&[&[&str]], usize) = match kind {
        Some("image") => (&[IMAGE_EXTENSIONS], 2000),
        Some("video") => (&[VIDEO_EXTENSIONS], 2000),
        _ => (&[VIDEO_EXTENSIONS, IMAGE_EXTENSIONS, AUDIO_EXTENSIONS], 100),
    };
    let wanted = |p: &Path| {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        lists.iter().any(|list| list.contains(&ext.as_str()))
    };
    let mut out = Vec::new();
    for raw in paths {
        let path = Path::new(&raw);
        if path.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && wanted(p))
                .collect();
            files.sort();
            out.extend(files.into_iter().map(|p| p.display().to_string()));
        } else if kind.is_none() || wanted(path) {
            out.push(raw);
        }
    }
    let skipped = out.len().saturating_sub(limit);
    out.truncate(limit);
    Expanded {
        files: out,
        skipped,
    }
}

fn finding(severity: Severity, label: impl Into<String>, detail: Option<&str>) -> Finding {
    Finding {
        severity,
        label: label.into(),
        detail: detail.map(String::from),
    }
}

/// `ext` is the lowercased file extension; ffprobe reports MKV and WebM alike.
fn findings(info: &MediaInfo, kind: Kind, c2pa: C2pa, ext: &str) -> Vec<Finding> {
    use Severity::*;
    let mut f = Vec::new();

    if kind == Kind::Video {
        if !info.is_mp4_family() {
            let label = match ext {
                "webm" => "WebM file".to_string(),
                "" => "Unusual container".to_string(),
                other => format!("{} file", other.to_uppercase()),
            };
            f.push(finding(
                Warn,
                label,
                Some("Premiere can't open this kind of file."),
            ));
        }

        let v = info.video().next().expect("video kind has a video stream");
        if !FRIENDLY_VIDEO.contains(&v.codec()) {
            let detail = format!(
                "Premiere can't edit {} video smoothly. Converting it to H.264 fixes this.",
                codec_label(v.codec())
            );
            f.push(finding(
                Warn,
                format!("{} video", codec_label(v.codec())),
                Some(&detail),
            ));
        }
        if info.vfr == Some(true) {
            f.push(finding(
                Warn,
                "Variable frame rate",
                Some("Premiere drifts the audio out of sync on files like this. Converting to a constant frame rate fixes it."),
            ));
        }
        if v.is_hdr() {
            let label = if v.color_transfer.as_deref() == Some("arib-std-b67") {
                "HDR (HLG)"
            } else {
                "HDR (PQ)"
            };
            f.push(finding(
                Info,
                label,
                Some("Edit it in an HDR sequence, or it will look washed out."),
            ));
        }
        if v.bit_depth().is_some_and(|b| b > 8) {
            f.push(finding(
                Info,
                format!("{}-bit", v.bit_depth().unwrap_or(10)),
                None,
            ));
        }
    }

    if kind != Kind::Image {
        let mut odd: Vec<&str> = Vec::new();
        for codec in info.audio().map(Stream::codec) {
            if !FRIENDLY_AUDIO.contains(&codec)
                && !codec.starts_with("pcm_")
                && !odd.contains(&codec)
            {
                odd.push(codec);
            }
        }
        for codec in odd {
            let detail = format!("Premiere can't read {} audio.", codec_label(codec));
            f.push(finding(
                Warn,
                format!("{} audio", codec_label(codec)),
                Some(&detail),
            ));
        }
        match info.audio().count() {
            0 if kind == Kind::Video => f.push(finding(Info, "No audio", None)),
            n if n > 1 => f.push(finding(
                Info,
                format!("{n} audio tracks"),
                Some("Every tool keeps all of them."),
            )),
            _ => {}
        }
        let subs = info.subtitles().count();
        if subs > 0 {
            f.push(finding(Info, plural(subs, "subtitle track"), None));
        }
    }

    if let Some(meta) = info.image.as_ref().filter(|_| kind == Kind::Image) {
        if meta.gps {
            f.push(finding(
                Warn,
                "Location",
                Some("This image records where it was taken."),
            ));
        }
        if meta.ai_marked {
            f.push(finding(
                Info,
                "Marked as AI-generated",
                Some("Its metadata labels it as made with generative AI."),
            ));
        }
        if let Some(camera) = &meta.camera {
            f.push(finding(
                Info,
                short(camera),
                Some("The camera or phone that took it."),
            ));
        }
        if let Some(software) = &meta.software {
            f.push(finding(
                Info,
                format!("Edited in {}", short(software)),
                None,
            ));
        }
        if meta.fields > 0 {
            f.push(finding(Info, plural(meta.fields, "metadata field"), None));
        }
    }

    match c2pa {
        C2pa::Present => f.push(finding(
            Info,
            "C2PA found",
            Some("This file carries Content Credentials (a C2PA manifest) that can identify how it was made."),
        )),
        C2pa::Absent => f.push(finding(Ok, "No C2PA", None)),
        C2pa::Unknown => {}
    }

    if kind == Kind::Video && !f.iter().any(|x| x.severity == Warn) {
        f.insert(0, finding(Ok, "Premiere-ready", None));
    }
    f
}

fn summary(findings: &[Finding], kind: Kind) -> String {
    let warnings: Vec<&str> = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .filter_map(|f| f.detail.as_deref())
        .collect();
    if !warnings.is_empty() {
        return warnings.join(" ");
    }
    let hidden = findings.iter().any(|f| {
        f.label == "C2PA found"
            || f.label.ends_with("metadata field")
            || f.label.ends_with("metadata fields")
    });
    match kind {
        Kind::Video => "Ready for Premiere. Nothing needs fixing.".into(),
        Kind::Audio => "Premiere can use this as it is.".into(),
        Kind::Image if hidden => {
            "This image carries hidden metadata. Strip metadata removes it without changing the picture.".into()
        }
        Kind::Image => "Nothing hidden in this image.".into(),
    }
}

/// Keeps a badge short: "Adobe Photoshop 26.0 (Windows)" → at most 32 characters.
fn short(s: &str) -> String {
    match s.char_indices().nth(32) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

fn headline(info: &MediaInfo, kind: Kind) -> String {
    let mut parts = Vec::new();
    match kind {
        Kind::Video | Kind::Image => {
            let s = if kind == Kind::Video {
                info.video().next()
            } else {
                info.streams.first()
            };
            if let Some(s) = s {
                parts.push(codec_label(s.codec()));
                if let (Some(w), Some(h)) = (s.width, s.height) {
                    parts.push(format!("{w}×{h}"));
                }
                if kind == Kind::Video
                    && let Some(fps) = s.nominal_fps()
                {
                    parts.push(format!("{} fps", fmt_fps(fps)));
                }
            }
        }
        Kind::Audio => {
            if let Some(a) = info.audio().next() {
                parts.push(codec_label(a.codec()));
                if let Some(hz) = a.sample_rate_hz() {
                    parts.push(format!("{} kHz", fmt_fps(hz as f64 / 1000.0)));
                }
            }
        }
    }
    parts.join(" · ")
}

fn details(info: &MediaInfo, c2pa: C2pa, ext: &str) -> Vec<Detail> {
    let mut d = Vec::new();
    let mut add = |label: &str, value: String| {
        d.push(Detail {
            label: label.into(),
            value,
        })
    };

    let image = info.is_image();
    // ffprobe calls every MP4 "QuickTime / MOV"; the extension says which it is.
    // (Images have no container worth naming; ffprobe would say "image2 sequence".)
    let container = match ext {
        _ if image => None,
        "mp4" | "m4v" if info.is_mp4_family() => Some("MP4".to_string()),
        "mov" if info.is_mp4_family() => Some("QuickTime (MOV)".to_string()),
        "mkv" => Some("Matroska (MKV)".to_string()),
        "webm" => Some("WebM".to_string()),
        _ => info.format_long_name.clone(),
    };
    if let Some(name) = container {
        add("Container", name);
    }
    if let Some(secs) = info.duration.filter(|s| *s > 0.0 && !image) {
        add("Duration", fmt_clock(secs));
    }
    if let Some(bytes) = info.size_bytes {
        add("Size", fmt_bytes(bytes));
    }
    if let Some(bps) = info.bit_rate.filter(|_| !image) {
        add("Bitrate", format!("{:.1} Mb/s", bps as f64 / 1_000_000.0));
    }

    if info.is_image()
        && let Some(s) = info.streams.first()
    {
        let mut parts = vec![codec_label(s.codec())];
        if let (Some(w), Some(h)) = (s.width, s.height) {
            parts.push(format!("{w}×{h}"));
        }
        parts.extend(s.pix_fmt.clone());
        add("Image", parts.join(" · "));
    }

    // A still image has no frame rate, pacing or colour range worth showing.
    for v in info.video().filter(|_| !info.is_image()) {
        let mut parts = vec![match &v.profile {
            Some(p) => format!("{} {p}", codec_label(v.codec())),
            None => codec_label(v.codec()),
        }];
        if let (Some(w), Some(h)) = (v.width, v.height) {
            parts.push(format!("{w}×{h}"));
        }
        if let Some(pix) = &v.pix_fmt {
            parts.push(pix.clone());
        }
        add("Video", parts.join(" · "));

        let rate = match (v.nominal_fps(), v.average_fps()) {
            (Some(n), Some(a)) if (n - a).abs() / n > 0.001 => {
                format!("{} avg / {} nominal", fmt_fps(a), fmt_fps(n))
            }
            (Some(n), _) => format!("{} fps", fmt_fps(n)),
            _ => "unknown".into(),
        };
        let pace = match info.vfr {
            Some(true) => " · variable",
            Some(false) => " · constant",
            None => "",
        };
        add("Frame rate", format!("{rate}{pace}"));

        let colour = [v.color_space.as_deref(), v.color_transfer.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let range = if v.is_hdr() { "HDR" } else { "SDR" };
        add(
            "Colour",
            if colour.is_empty() {
                range.into()
            } else {
                format!("{colour} · {range}")
            },
        );
        if let Some(bits) = v.bit_depth() {
            add("Bit depth", format!("{bits}-bit"));
        }
    }

    for (i, a) in info.audio().enumerate() {
        let mut parts = vec![codec_label(a.codec())];
        if let Some(hz) = a.sample_rate_hz() {
            parts.push(format!("{} kHz", fmt_fps(hz as f64 / 1000.0)));
        }
        match (&a.channel_layout, a.channels) {
            (Some(layout), _) => parts.push(layout.clone()),
            (None, Some(n)) => parts.push(plural(n as usize, "channel")),
            _ => {}
        }
        // "und" is ffprobe's "language not set".
        parts.extend(a.tags.get("language").filter(|l| *l != "und").cloned());
        parts.extend(a.tags.get("title").cloned());
        add(&format!("Audio {}", i + 1), parts.join(" · "));
    }

    let subs: Vec<String> = info
        .subtitles()
        .map(|s| {
            s.tags
                .get("language")
                .cloned()
                .unwrap_or_else(|| codec_label(s.codec()))
        })
        .collect();
    if !subs.is_empty() {
        add("Subtitles", subs.join(", "));
    }

    for (key, value) in info.tags.iter().take(8) {
        let value: String = value.chars().take(80).collect();
        add(key, value);
    }

    if let Some(meta) = &info.image {
        if let Some(camera) = &meta.camera {
            add("Camera", camera.clone());
        }
        if let Some(software) = &meta.software {
            add("Software", software.clone());
        }
        add(
            "Location",
            if meta.gps {
                "Recorded".into()
            } else {
                "None".into()
            },
        );
        if let Some(o) = meta.orientation.filter(|o| *o != 1) {
            let turn = match o {
                3 => "Shown upside down",
                6 => "Shown turned 90° clockwise",
                8 => "Shown turned 90° anticlockwise",
                _ => "Shown mirrored",
            };
            add(
                "Rotation tag",
                format!("{turn} (kept when stripping, so it stays the right way up)"),
            );
        }
        add(
            "Colour profile",
            if meta.icc { "Yes".into() } else { "No".into() },
        );
        add("Metadata fields", meta.fields.to_string());
    }

    add(
        "C2PA",
        match c2pa {
            C2pa::Present => "Present".into(),
            C2pa::Absent => "Not present".into(),
            C2pa::Unknown => "Not checked for this format".into(),
        },
    );
    d
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// 60 → "60", 59.94006 → "59.94", 44.1 → "44.1".
fn fmt_fps(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn fmt_clock(secs: f64) -> String {
    let s = secs.round() as u64;
    let (h, m, s) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn fmt_bytes(b: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = b as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(index: u32, kind: &str, codec: &str) -> Stream {
        Stream {
            index,
            codec_type: Some(kind.into()),
            codec_name: Some(codec.into()),
            ..Default::default()
        }
    }

    fn labels(f: &[Finding]) -> Vec<&str> {
        f.iter().map(|x| x.label.as_str()).collect()
    }

    #[test]
    fn vp9_vfr_opus_mkv() {
        let info = MediaInfo {
            format_name: Some("matroska,webm".into()),
            streams: vec![
                stream(0, "video", "vp9"),
                stream(1, "audio", "opus"),
                stream(2, "audio", "opus"),
            ],
            vfr: Some(true),
            ..Default::default()
        };
        let f = findings(&info, Kind::Video, C2pa::Unknown, "webm");
        assert_eq!(
            labels(&f),
            [
                "WebM file",
                "VP9 video",
                "Variable frame rate",
                "Opus audio",
                "2 audio tracks"
            ]
        );
        assert_eq!(
            findings(&info, Kind::Video, C2pa::Unknown, "mkv")[0].label,
            "MKV file"
        );
        let s = summary(&f, Kind::Video);
        assert!(s.starts_with("Premiere can't open this kind of file."));
    }

    #[test]
    fn clean_mp4() {
        let info = MediaInfo {
            format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".into()),
            streams: vec![stream(0, "video", "h264"), stream(1, "audio", "aac")],
            vfr: Some(false),
            ..Default::default()
        };
        let f = findings(&info, Kind::Video, C2pa::Absent, "mp4");
        assert_eq!(labels(&f), ["Premiere-ready", "No C2PA"]);
        assert_eq!(
            summary(&f, Kind::Video),
            "Ready for Premiere. Nothing needs fixing."
        );
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_fps(60.0), "60");
        assert_eq!(fmt_fps(60000.0 / 1001.0), "59.94");
        assert_eq!(fmt_clock(11564.25), "3:12:44");
        assert_eq!(fmt_bytes(15_248_112_640), "14.2 GB");
    }
}
