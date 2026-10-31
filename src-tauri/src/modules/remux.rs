//! Remux: re-wrap into MP4 without touching the video. Audio Premiere can't use
//! (Opus and friends) becomes AAC 48 kHz; every audio track is kept.

use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, Suggestion};
use crate::media::{self, MediaInfo, codec_label};
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::tools::Tool;

pub struct Remux;

/// Video codecs that can be stream-copied into an MP4.
const MP4_VIDEO: &[&str] = &["h264", "hevc", "av1", "vp9", "mpeg4"];
/// Of those, the ones Premiere edits happily. Only these get a remux suggestion.
const PREMIERE_VIDEO: &[&str] = &["h264", "hevc"];
/// Audio codecs copied as-is; anything else is converted to AAC.
const KEEP_AUDIO: &[&str] = &["aac", "mp3"];

impl Module for Remux {
    fn id(&self) -> &'static str {
        "remux"
    }

    fn name(&self) -> &'static str {
        "Remux to MP4"
    }

    fn description(&self) -> &'static str {
        "Re-wraps MKV and other files into an MP4 Premiere opens, in seconds, without touching the picture. Audio Premiere can't read becomes AAC, and every audio track is kept."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    /// Suggested when the video is already Premiere-friendly and only the container
    /// or the audio is in the way (BRIEF §7).
    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        if info.is_image() {
            return None;
        }
        let video: Vec<_> = info.video().collect();
        if video.is_empty() || !video.iter().all(|v| PREMIERE_VIDEO.contains(&v.codec())) {
            return None;
        }
        let odd_audio: Vec<&str> = info
            .audio()
            .map(|a| a.codec())
            .filter(|c| !KEEP_AUDIO.contains(c))
            .collect();
        if info.is_mp4_family() && odd_audio.is_empty() {
            return None;
        }

        let mut reason = format!(
            "The video is already {}, so this only re-wraps it as MP4. Lossless, and takes seconds.",
            codec_label(video[0].codec())
        );
        if let Some(audio) = odd_audio.first() {
            reason.push_str(&format!(" The {} audio becomes AAC.", codec_label(audio)));
        }
        let vfr = info.vfr == Some(true);
        if vfr {
            reason.push_str(" It doesn't fix the variable frame rate.");
        }
        Some(Suggestion {
            reason,
            priority: if vfr { 40 } else { 90 },
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        _options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("Remux works on files, not links.")?;
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        let output = ctx.output_next_to(file, "", "mp4");

        let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-n", "-i"]
            .map(String::from)
            .to_vec();
        args.push(file.display().to_string());
        args.extend(stream_args(&info)?);
        args.extend(["-map_metadata", "0", "-movflags", "+faststart"].map(String::from));
        args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
        args.push(output.display().to_string());

        Ok(Plan {
            title: format!("Remux {}", input.short_name()),
            steps: vec![Step {
                label: "Remuxing".into(),
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
}

/// -map and codec arguments for every stream worth keeping.
pub(super) fn stream_args(info: &MediaInfo) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    for v in info.video() {
        if !MP4_VIDEO.contains(&v.codec()) {
            return Err(format!(
                "The video is {}, which can't go into an MP4 as-is. Use Re-encode instead.",
                v.codec().to_uppercase()
            ));
        }
        args.extend(["-map".into(), format!("0:{}", v.index)]);
    }
    for a in info.audio() {
        args.extend(["-map".into(), format!("0:{}", a.index)]);
    }
    if args.is_empty() {
        return Err("This file has no video or audio to remux.".into());
    }

    args.extend(["-c:v", "copy"].map(String::from));
    // HEVC needs the hvc1 tag or Premiere and macOS treat it as unplayable.
    if info.video().any(|v| v.codec() == "hevc") {
        args.extend(["-tag:v", "hvc1"].map(String::from));
    }
    for (out_index, a) in info.audio().enumerate() {
        if KEEP_AUDIO.contains(&a.codec()) {
            args.extend([format!("-c:a:{out_index}"), "copy".into()]);
        } else {
            args.extend([
                format!("-c:a:{out_index}"),
                "aac".into(),
                format!("-b:a:{out_index}"),
                "320k".into(),
                format!("-ar:a:{out_index}"),
                "48000".into(),
            ]);
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Stream;

    fn stream(index: u32, kind: &str, codec: &str) -> Stream {
        Stream {
            index,
            codec_type: Some(kind.into()),
            codec_name: Some(codec.into()),
            ..Default::default()
        }
    }

    #[test]
    fn keeps_every_audio_track_and_converts_opus() {
        let info = MediaInfo {
            duration: Some(10.0),
            streams: vec![
                stream(0, "video", "h264"),
                stream(1, "audio", "aac"),
                stream(2, "audio", "opus"),
            ],
            ..Default::default()
        };
        let args = stream_args(&info).unwrap().join(" ");
        assert_eq!(
            args,
            "-map 0:0 -map 0:1 -map 0:2 -c:v copy -c:a:0 copy -c:a:1 aac -b:a:1 320k -ar:a:1 48000"
        );
    }

    #[test]
    fn refuses_prores() {
        let info = MediaInfo {
            streams: vec![stream(0, "video", "prores")],
            ..Default::default()
        };
        assert!(stream_args(&info).unwrap_err().contains("PRORES"));
    }

    #[test]
    fn suggests_only_when_it_helps() {
        let mkv = |audio: &str| MediaInfo {
            format_name: Some("matroska,webm".into()),
            streams: vec![stream(0, "video", "h264"), stream(1, "audio", audio)],
            vfr: Some(false),
            ..Default::default()
        };
        let s = Remux.suggest(&mkv("opus")).unwrap();
        assert!(s.reason.contains("Opus audio becomes AAC"));
        assert!(s.priority >= 80);

        // Already an MP4 with AAC: nothing to do.
        let mut mp4 = mkv("aac");
        mp4.format_name = Some("mov,mp4,m4a,3gp,3g2,mj2".into());
        assert!(Remux.suggest(&mp4).is_none());

        // VP9 would remux, but Premiere still can't edit it, so don't suggest.
        let mut vp9 = mkv("opus");
        vp9.streams[0].codec_name = Some("vp9".into());
        assert!(Remux.suggest(&vp9).is_none());

        // Variable frame rate: still useful, but not the fix that matters most.
        let mut vfr = mkv("aac");
        vfr.vfr = Some(true);
        let s = Remux.suggest(&vfr).unwrap();
        assert!(s.priority < 50 && s.reason.contains("doesn't fix the variable frame rate"));
    }
}
