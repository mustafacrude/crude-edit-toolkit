//! The shared encode settings panel (BRIEF §8): frame rate, quality or bitrate,
//! keyframes and audio, turned into ffmpeg arguments for whichever encoder this
//! machine has. Used by Make Premiere-safe, Constant frame rate, Re-encode and
//! exact clips.

use serde::{Deserialize, Serialize};

use crate::encoders::Codec;
use crate::media::{MediaInfo, Stream};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum RateControl {
    /// Constant quality: the encoder spends bits where the footage needs them.
    /// `cq` is on the x264 CRF scale (16 visually lossless … 27 small).
    Quality { cq: u8 },
    /// Average and peak bitrate, for a predictable file size.
    #[serde(rename_all = "camelCase")]
    Bitrate { target_mbps: f64, max_mbps: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EncodeSettings {
    pub codec: Codec,
    /// None: match the source's own rate. Otherwise "59.94", "30", "60000/1001"…
    pub fps: Option<String>,
    pub rate: RateControl,
    /// A keyframe this often, for smoother scrubbing in Premiere. None: encoder default.
    pub keyframe_secs: Option<f64>,
    pub audio_kbps: u32,
}

impl Default for EncodeSettings {
    /// The Premiere-safe preset (BRIEF §10).
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            fps: None,
            rate: RateControl::Quality { cq: 19 },
            keyframe_secs: Some(1.0),
            audio_kbps: 320,
        }
    }
}

impl EncodeSettings {
    /// Reads `options[key]`, falling back to the preset for anything missing.
    pub fn from_options(options: &serde_json::Value, key: &str) -> Result<Self, String> {
        match options.get(key) {
            None | Some(serde_json::Value::Null) => Ok(Self::default()),
            Some(v) => {
                let s: Self = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Bad encode settings: {e}"))?;
                s.validate()?;
                Ok(s)
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        if let Some(fps) = &self.fps {
            fps_ratio(fps).ok_or_else(|| {
                format!("\"{fps}\" isn't a frame rate. Use a number like 30 or 59.94.")
            })?;
        }
        match self.rate {
            RateControl::Quality { cq } if !(1..=51).contains(&cq) => {
                Err("Quality must be between 1 and 51.".into())
            }
            RateControl::Bitrate {
                target_mbps,
                max_mbps,
            } if !(0.1..=500.0).contains(&target_mbps) || max_mbps < target_mbps => Err(
                "Bitrate must be between 0.1 and 500 Mb/s, with the max at least the target."
                    .into(),
            ),
            _ => Ok(()),
        }
    }
}

/// "59.94" → "60000/1001" (the NTSC rates people round), "30" → "30",
/// "24000/1001" stays. None for anything that isn't 1–240 fps.
pub fn fps_ratio(fps: &str) -> Option<String> {
    let fps = fps.trim();
    let value = match fps.split_once('/') {
        Some(_) => crate::media::parse_ratio(fps)?,
        None => fps.parse::<f64>().ok()?,
    };
    if !(1.0..=240.0).contains(&value) {
        return None;
    }
    if fps.contains('/') {
        return Some(fps.to_string());
    }
    for (rounded, exact) in [
        (23.976, "24000/1001"),
        (29.97, "30000/1001"),
        (59.94, "60000/1001"),
        (119.88, "120000/1001"),
    ] {
        if (value - rounded).abs() < 0.005 {
            return Some(exact.into());
        }
    }
    Some(fps.to_string())
}

/// Encoder families take quality and pixel formats differently.
#[derive(Clone, Copy, PartialEq)]
enum Family {
    Nvenc,
    Amf,
    Qsv,
    VideoToolbox,
    Software,
}

fn family(encoder: &str) -> Family {
    match encoder.rsplit('_').next() {
        Some("nvenc") => Family::Nvenc,
        Some("amf") => Family::Amf,
        Some("qsv") => Family::Qsv,
        Some("videotoolbox") => Family::VideoToolbox,
        _ => Family::Software,
    }
}

/// Video arguments for `video` (an input stream) through `encoder`: codec, rate
/// control, pixel format, constant frame rate and keyframes.
pub fn video_args(
    encoder: &str,
    settings: &EncodeSettings,
    video: &Stream,
) -> Result<Vec<String>, String> {
    let fam = family(encoder);
    let mut a: Vec<String> = vec!["-c:v".into(), encoder.into()];

    match (&settings.rate, fam) {
        (RateControl::Quality { cq }, Family::Nvenc) => a.extend(
            [
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-rc",
                "vbr",
                "-cq",
                &cq.to_string(),
                "-b:v",
                "0",
            ]
            .map(String::from),
        ),
        (RateControl::Quality { cq }, Family::Amf) => a.extend([
            "-quality".into(),
            "quality".into(),
            "-rc".into(),
            "cqp".into(),
            "-qp_i".into(),
            cq.to_string(),
            "-qp_p".into(),
            (cq + 2).to_string(),
            "-qp_b".into(),
            (cq + 4).to_string(),
        ]),
        (RateControl::Quality { cq }, Family::Qsv) => a.extend([
            "-preset".into(),
            "slower".into(),
            "-global_quality".into(),
            cq.to_string(),
        ]),
        (RateControl::Quality { cq }, Family::VideoToolbox) => {
            // VideoToolbox's -q:v runs 1–100, higher is better.
            let q = (100.0 - f64::from(*cq) * 1.6).clamp(1.0, 100.0).round();
            a.extend(["-q:v".into(), q.to_string()]);
        }
        (RateControl::Quality { cq }, Family::Software) => a.extend([
            "-preset".into(),
            "medium".into(),
            "-crf".into(),
            cq.to_string(),
        ]),
        (
            RateControl::Bitrate {
                target_mbps,
                max_mbps,
            },
            fam,
        ) => {
            match fam {
                Family::Nvenc => {
                    a.extend(["-preset", "p5", "-tune", "hq", "-rc", "vbr"].map(String::from))
                }
                Family::Amf => {
                    a.extend(["-quality", "quality", "-rc", "vbr_peak"].map(String::from))
                }
                Family::Qsv => a.extend(["-preset", "slower"].map(String::from)),
                Family::Software => a.extend(["-preset", "medium"].map(String::from)),
                Family::VideoToolbox => {}
            }
            let kbps = |m: f64| format!("{}k", (m * 1000.0).round());
            a.extend([
                "-b:v".into(),
                kbps(*target_mbps),
                "-maxrate".into(),
                kbps(*max_mbps),
                "-bufsize".into(),
                kbps(max_mbps * 2.0),
            ]);
        }
    }

    // 10-bit HEVC when the source has it; everything else 8-bit 4:2:0, which
    // Premiere decodes everywhere.
    let ten_bit = settings.codec == Codec::Hevc && video.bit_depth().unwrap_or(8) > 8;
    let pix_fmt = match (fam, ten_bit) {
        (Family::Software, true) => "yuv420p10le",
        (_, true) => "p010le",
        (Family::Nvenc | Family::Software, false) => "yuv420p",
        (_, false) => "nv12",
    };
    a.extend(["-pix_fmt".into(), pix_fmt.into()]);
    if settings.codec == Codec::H264 {
        a.extend(["-profile:v", "high"].map(String::from));
    } else {
        // Without the hvc1 tag Premiere and macOS treat HEVC in MP4 as unplayable.
        a.extend(["-tag:v", "hvc1"].map(String::from));
    }

    // Constant frame rate, at the source's own rate unless one was chosen.
    let rate = match &settings.fps {
        Some(fps) => fps_ratio(fps).ok_or("Bad frame rate.")?,
        None => video
            .r_frame_rate
            .clone()
            .filter(|r| crate::media::parse_ratio(r).is_some())
            .ok_or(
                "Couldn't tell this video's frame rate. Choose one instead of \"Match source\".",
            )?,
    };
    a.extend(["-fps_mode".into(), "cfr".into(), "-r".into(), rate.clone()]);
    if let Some(secs) = settings.keyframe_secs {
        let fps = crate::media::parse_ratio(&rate).unwrap_or(30.0);
        a.extend([
            "-g".into(),
            ((fps * secs).round().max(1.0) as u32).to_string(),
        ]);
    }
    Ok(a)
}

/// Every audio track as AAC at 48 kHz.
pub fn audio_args(settings: &EncodeSettings) -> Vec<String> {
    vec![
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", settings.audio_kbps),
        "-ar".into(),
        "48000".into(),
    ]
}

/// The first real video stream and every audio track (BRIEF: keep all audio).
pub fn map_args(info: &MediaInfo) -> Result<Vec<String>, String> {
    let video = info.video().next().ok_or("This file has no video.")?;
    let mut a = vec!["-map".into(), format!("0:{}", video.index)];
    for audio in info.audio() {
        a.extend(["-map".into(), format!("0:{}", audio.index)]);
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(codec: &str, rate: &str, pix: &str) -> Stream {
        Stream {
            codec_type: Some("video".into()),
            codec_name: Some(codec.into()),
            r_frame_rate: Some(rate.into()),
            pix_fmt: Some(pix.into()),
            ..Default::default()
        }
    }

    #[test]
    fn nvenc_premiere_safe() {
        let args = video_args(
            "h264_nvenc",
            &EncodeSettings::default(),
            &video("vp9", "60/1", "yuv420p"),
        )
        .unwrap();
        assert_eq!(
            args.join(" "),
            "-c:v h264_nvenc -preset p5 -tune hq -rc vbr -cq 19 -b:v 0 -pix_fmt yuv420p -profile:v high -fps_mode cfr -r 60/1 -g 60"
        );
    }

    #[test]
    fn software_hevc_bitrate_10bit_fixed_rate() {
        let s = EncodeSettings {
            codec: Codec::Hevc,
            fps: Some("29.97".into()),
            rate: RateControl::Bitrate {
                target_mbps: 28.0,
                max_mbps: 40.0,
            },
            keyframe_secs: None,
            audio_kbps: 320,
        };
        let args = video_args("libx265", &s, &video("hevc", "60/1", "yuv420p10le")).unwrap();
        assert_eq!(
            args.join(" "),
            "-c:v libx265 -preset medium -b:v 28000k -maxrate 40000k -bufsize 80000k -pix_fmt yuv420p10le -tag:v hvc1 -fps_mode cfr -r 30000/1001"
        );
    }

    #[test]
    fn frame_rates() {
        assert_eq!(fps_ratio("59.94").as_deref(), Some("60000/1001"));
        assert_eq!(fps_ratio("30").as_deref(), Some("30"));
        assert_eq!(fps_ratio("24000/1001").as_deref(), Some("24000/1001"));
        assert_eq!(fps_ratio("0"), None);
        assert_eq!(fps_ratio("fast"), None);
    }

    #[test]
    fn options_fall_back_to_preset() {
        let s = EncodeSettings::from_options(&serde_json::json!({}), "encode").unwrap();
        assert_eq!(s, EncodeSettings::default());
        let s = EncodeSettings::from_options(
            &serde_json::json!({"encode": {"rate": {"mode": "bitrate", "targetMbps": 20, "maxMbps": 30}}}),
            "encode",
        )
        .unwrap();
        assert_eq!(
            s.rate,
            RateControl::Bitrate {
                target_mbps: 20.0,
                max_mbps: 30.0
            }
        );
        assert_eq!(s.codec, Codec::H264);
        assert!(
            EncodeSettings::from_options(&serde_json::json!({"encode": {"fps": "fast"}}), "encode")
                .is_err()
        );
    }
}
