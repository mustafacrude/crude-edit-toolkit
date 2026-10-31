//! Which hardware video encoders actually work on this machine. Listing them in
//! `ffmpeg -encoders` isn't enough (an NVENC build lists NVENC on a PC with no
//! NVIDIA card), so each candidate does a tiny test encode. The result is cached
//! in the app data folder and redone when ffmpeg changes (BRIEF §13.8).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::tools::{Tool, command, run};

/// A version check or five-frame test encode never needs longer; a GPU driver
/// that hangs gets stopped instead of holding up the app.
const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// In order of preference: NVIDIA, AMD, Intel, Apple.
const H264: &[&str] = &["h264_nvenc", "h264_amf", "h264_qsv", "h264_videotoolbox"];
const HEVC: &[&str] = &["hevc_nvenc", "hevc_amf", "hevc_qsv", "hevc_videotoolbox"];
pub const SOFTWARE_H264: &str = "libx264";
pub const SOFTWARE_HEVC: &str = "libx265";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Hevc,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncoderStatus {
    /// The ffmpeg version this was tested with.
    pub ffmpeg: String,
    /// Hardware encoders that passed the test, in preference order.
    pub h264: Vec<String>,
    pub hevc: Vec<String>,
}

pub struct Encoders {
    cache_path: PathBuf,
    status: Mutex<Option<EncoderStatus>>,
}

impl Encoders {
    pub fn new(cache_path: PathBuf) -> Self {
        let cached = std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        Self {
            cache_path,
            status: Mutex::new(cached),
        }
    }

    /// The tested encoders, testing now if there's no result for this ffmpeg yet.
    /// Blocking: call off the UI thread.
    pub fn status(&self, app: &AppHandle) -> EncoderStatus {
        let ffmpeg = Tool::Ffmpeg.path(app).unwrap_or_default();
        let version = ffmpeg_version(&ffmpeg);
        if let Some(s) = self.status.lock().unwrap().as_ref()
            && s.ffmpeg == version
        {
            return s.clone();
        }
        self.retest(app)
    }

    /// Runs the test encodes again, e.g. after a driver update.
    pub fn retest(&self, app: &AppHandle) -> EncoderStatus {
        let ffmpeg = Tool::Ffmpeg.path(app).unwrap_or_default();
        let status = EncoderStatus {
            ffmpeg: ffmpeg_version(&ffmpeg),
            h264: H264
                .iter()
                .filter(|e| works(&ffmpeg, e))
                .map(|e| e.to_string())
                .collect(),
            hevc: HEVC
                .iter()
                .filter(|e| works(&ffmpeg, e))
                .map(|e| e.to_string())
                .collect(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&status) {
            let _ = std::fs::write(&self.cache_path, json);
        }
        *self.status.lock().unwrap() = Some(status.clone());
        status
    }

    /// The encoder to use for `codec`: the user's choice if it works here,
    /// otherwise the first working hardware encoder, otherwise software.
    pub fn pick(&self, app: &AppHandle, codec: Codec, preference: &str) -> String {
        let status = self.status(app);
        let (hardware, software) = match codec {
            Codec::H264 => (&status.h264, SOFTWARE_H264),
            Codec::Hevc => (&status.hevc, SOFTWARE_HEVC),
        };
        // A preference names a family ("nvenc") or "software"; map it to this codec.
        if preference == "software" {
            return software.into();
        }
        if let Some(chosen) = hardware
            .iter()
            .find(|e| e.ends_with(&format!("_{preference}")))
        {
            return chosen.clone();
        }
        hardware.first().cloned().unwrap_or_else(|| software.into())
    }
}

/// e.g. "9.0.2", from the first line of `ffmpeg -version`.
fn ffmpeg_version(ffmpeg: &Path) -> String {
    run(
        command(ffmpeg).args(["-hide_banner", "-version"]),
        TEST_TIMEOUT,
    )
    .ok()
    .and_then(|o| {
        String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .nth(2)
            .map(String::from)
    })
    .unwrap_or_default()
}

/// A five-frame encode to nowhere. Fails fast when the hardware isn't there.
fn works(ffmpeg: &Path, encoder: &str) -> bool {
    run(
        command(ffmpeg).args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=320x240:r=30",
            "-frames:v",
            "5",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ]),
        TEST_TIMEOUT,
    )
    .is_ok_and(|o| o.status.success())
}

/// A friendly name for an encoder id, for Settings and Show command readers.
pub fn label(encoder: &str) -> &'static str {
    match encoder.rsplit('_').next().unwrap_or("") {
        "nvenc" => "NVIDIA (NVENC)",
        "amf" => "AMD (AMF)",
        "qsv" => "Intel (Quick Sync)",
        "videotoolbox" => "Apple (VideoToolbox)",
        _ => "Software (CPU)",
    }
}

/// Starts the test in the background at launch so the first encode doesn't wait.
pub fn warm_up(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Encoders>().status(&app);
    });
}
