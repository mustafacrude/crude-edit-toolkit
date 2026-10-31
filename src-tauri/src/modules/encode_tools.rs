//! The three tools built on the shared encode panel: Make Premiere-safe (the
//! one-click preset), Constant frame rate, and Re-encode (everything adjustable).

use super::schema::Field;
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, Suggestion};
use crate::encode::{self, EncodeSettings};
use crate::encoders::{self, Codec};
use crate::media::{self, MediaInfo, codec_label};
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::tools::Tool;

/// Video codecs Premiere edits without trouble.
const FRIENDLY_VIDEO: &[&str] = &["h264", "hevc", "prores", "dnxhd"];
/// Audio Premiere reads.
const FRIENDLY_AUDIO: &[&str] = &["aac", "mp3", "ac3", "eac3"];

/// What stands between a file and a clean Premiere import.
#[derive(Default)]
struct Problems {
    video_codec: Option<String>,
    vfr: bool,
    audio_codec: Option<String>,
    container: bool,
}

impl Problems {
    fn of(info: &MediaInfo) -> Self {
        let video = info.video().next();
        Self {
            video_codec: video
                .map(|v| v.codec())
                .filter(|c| !FRIENDLY_VIDEO.contains(c))
                .map(codec_label),
            vfr: info.vfr == Some(true),
            audio_codec: info
                .audio()
                .map(|a| a.codec())
                .find(|c| !FRIENDLY_AUDIO.contains(c) && !c.starts_with("pcm_"))
                .map(codec_label),
            container: !info.is_mp4_family(),
        }
    }

    fn any(&self) -> bool {
        self.video_codec.is_some() || self.vfr || self.audio_codec.is_some() || self.container
    }

    /// "converts the VP9 video to H.264, fixes the variable frame rate and makes the Opus audio AAC"
    fn fixes(&self) -> String {
        let mut parts = Vec::new();
        if let Some(c) = &self.video_codec {
            parts.push(format!("converts the {c} video to H.264"));
        }
        if self.vfr {
            parts.push("fixes the variable frame rate".to_string());
        }
        if let Some(c) = &self.audio_codec {
            parts.push(format!("makes the {c} audio AAC"));
        }
        match parts.len() {
            0 => "makes an MP4 Premiere opens".into(),
            1 => parts.remove(0),
            _ => {
                let last = parts.pop().unwrap_or_default();
                format!("{} and {last}", parts.join(", "))
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Probes `input` and plans one encode with `settings`.
fn plan_encode(
    ctx: &PlanContext,
    input: &JobInput,
    settings: &EncodeSettings,
    suffix: &str,
    verb: &str,
    info: Option<MediaInfo>,
) -> Result<Plan, String> {
    let file = input.file().ok_or("This tool works on files, not links.")?;
    let info = match info {
        Some(i) => i,
        None => media::probe(&ctx.tool(Tool::Ffprobe)?, file)?,
    };
    let video = info
        .video()
        .next()
        .ok_or("This file has no video to convert.")?;
    let encoder = ctx.encoder(settings.codec);
    let output = ctx.output_next_to(file, suffix, "mp4");

    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-n", "-i"]
        .map(String::from)
        .to_vec();
    args.push(file.display().to_string());
    args.extend(encode::map_args(&info)?);
    args.extend(encode::video_args(&encoder, settings, video)?);
    args.extend(encode::audio_args(settings));
    args.extend(["-map_metadata", "0", "-movflags", "+faststart"].map(String::from));
    args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
    args.push(output.display().to_string());

    Ok(Plan {
        title: format!("{verb} {}", input.short_name()),
        steps: vec![Step {
            label: format!("Encoding with {}", encoders::label(&encoder)),
            tool: Tool::Ffmpeg,
            args,
            progress: ProgressKind::Ffmpeg {
                duration: info.duration,
            },
            redact: vec![],
        }],
        outputs: vec![output],
        ..Default::default()
    })
}

fn encode_schema() -> Vec<Field> {
    vec![Field::Encode { key: "encode" }]
}

pub struct PremiereSafe;

impl Module for PremiereSafe {
    fn id(&self) -> &'static str {
        "premiere-safe"
    }

    fn name(&self) -> &'static str {
        "Make Premiere-safe"
    }

    fn description(&self) -> &'static str {
        "Converts any video into an H.264 MP4 at a constant frame rate with AAC 48 kHz audio, so it drops onto a Premiere timeline without sync or codec problems. Keeps every audio track."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        encode_schema()
    }

    /// Suggested when the video codec itself is the problem. (A friendly codec
    /// with a variable frame rate gets Constant frame rate; one in the wrong
    /// container gets Remux.)
    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        if info.is_image() || info.video().next().is_none() {
            return None;
        }
        let problems = Problems::of(info);
        problems.video_codec.as_ref()?;
        Some(Suggestion {
            reason: format!(
                "{}. Re-encodes the whole video, so it takes a few minutes.",
                capitalize(&problems.fixes())
            ),
            priority: 95,
        })
    }

    /// Option `onlyIfNeeded`: skip files that are already fine (used after downloads).
    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let settings = EncodeSettings::from_options(options, "encode")?;
        let only_if_needed = options
            .get("onlyIfNeeded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut info = None;
        if only_if_needed {
            let file = input.file().ok_or("This tool works on files, not links.")?;
            let ffprobe = ctx.tool(Tool::Ffprobe)?;
            let mut probed = media::probe(&ffprobe, file)?;
            probed.vfr = media::sample_vfr(&ffprobe, file, 20);
            if !Problems::of(&probed).any() {
                return Ok(Plan {
                    skip: Some(format!("{} is already Premiere-safe.", input.short_name())),
                    ..Default::default()
                });
            }
            info = Some(probed);
        }
        plan_encode(
            ctx,
            input,
            &settings,
            "_premiere",
            "Make Premiere-safe:",
            info,
        )
    }
}

pub struct ConstantFrameRate;

impl Module for ConstantFrameRate {
    fn id(&self) -> &'static str {
        "cfr"
    }

    fn name(&self) -> &'static str {
        "Constant frame rate"
    }

    fn description(&self) -> &'static str {
        "Re-encodes a variable-frame-rate recording (phones, some screen recorders) at a steady frame rate, so its audio stays in sync in Premiere."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        encode_schema()
    }

    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        let problems = Problems::of(info);
        if !problems.vfr || problems.video_codec.is_some() {
            return None;
        }
        let rate = info
            .video()
            .next()
            .and_then(|v| v.nominal_fps())
            .map(|f| format!(" at {} fps", (f * 100.0).round() / 100.0))
            .unwrap_or_default();
        let audio = problems
            .audio_codec
            .map(|c| format!(" Also makes the {c} audio AAC."))
            .unwrap_or_default();
        Some(Suggestion {
            reason: format!(
                "Re-encodes{rate} so the audio stays in sync in Premiere.{audio} Takes a few minutes."
            ),
            priority: 96,
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let settings = EncodeSettings::from_options(options, "encode")?;
        plan_encode(ctx, input, &settings, "_cfr", "Constant frame rate:", None)
    }
}

pub struct Reencode;

impl Module for Reencode {
    fn id(&self) -> &'static str {
        "reencode"
    }

    fn name(&self) -> &'static str {
        "Re-encode"
    }

    fn description(&self) -> &'static str {
        "Converts to H.264 or HEVC with every setting yours to choose: frame rate, quality or bitrate, keyframes and audio."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        encode_schema()
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let settings = EncodeSettings::from_options(options, "encode")?;
        let suffix = match settings.codec {
            Codec::H264 => "_h264",
            Codec::Hevc => "_hevc",
        };
        plan_encode(ctx, input, &settings, suffix, "Re-encode", None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Stream;

    fn info(container: &str, video: &str, audio: &str, vfr: bool) -> MediaInfo {
        let s = |kind: &str, codec: &str| Stream {
            codec_type: Some(kind.into()),
            codec_name: Some(codec.into()),
            r_frame_rate: Some("60/1".into()),
            ..Default::default()
        };
        MediaInfo {
            format_name: Some(container.into()),
            streams: vec![s("video", video), s("audio", audio)],
            vfr: Some(vfr),
            ..Default::default()
        }
    }

    #[test]
    fn suggestions_split_the_work() {
        let vp9 = info("matroska,webm", "vp9", "opus", true);
        let s = PremiereSafe.suggest(&vp9).unwrap();
        assert_eq!(
            s.reason,
            "Converts the VP9 video to H.264, fixes the variable frame rate and makes the Opus audio AAC. Re-encodes the whole video, so it takes a few minutes."
        );
        assert!(ConstantFrameRate.suggest(&vp9).is_none());

        let phone = info("mov,mp4,m4a,3gp,3g2,mj2", "h264", "aac", true);
        assert!(PremiereSafe.suggest(&phone).is_none());
        assert!(
            ConstantFrameRate
                .suggest(&phone)
                .unwrap()
                .reason
                .starts_with("Re-encodes at 60 fps")
        );

        let clean = info("mov,mp4,m4a,3gp,3g2,mj2", "h264", "aac", false);
        assert!(!Problems::of(&clean).any());
        assert!(
            PremiereSafe.suggest(&clean).is_none() && ConstantFrameRate.suggest(&clean).is_none()
        );
    }
}
