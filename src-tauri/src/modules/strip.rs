//! Strip metadata: a copy of the file with its metadata, chapters and any C2PA
//! manifest removed, without re-encoding.

use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, Suggestion};
use crate::media::{self, MediaInfo};
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::tools::Tool;

pub struct StripMetadata;

impl Module for StripMetadata {
    fn id(&self) -> &'static str {
        "strip-metadata"
    }

    fn name(&self) -> &'static str {
        "Strip metadata"
    }

    fn description(&self) -> &'static str {
        "Makes a copy with metadata, chapters and Content Credentials (C2PA) removed. Nothing is re-encoded, so it takes seconds and the picture is identical."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        if info.is_image() || info.c2pa != Some(true) {
            return None;
        }
        Some(Suggestion {
            reason: "Removes the Content Credentials (C2PA) and other hidden metadata without re-encoding. Takes seconds.".into(),
            priority: 60,
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        _options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("This tool works on files, not links.")?;
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let matroska = matches!(ext.as_str(), "mkv" | "webm");
        let ext = if matroska || info.is_mp4_family() {
            ext
        } else {
            "mp4".into()
        };

        let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-n", "-i"]
            .map(String::from)
            .to_vec();
        args.push(file.display().to_string());
        // Real video and audio only: cover art and data tracks are metadata too.
        for s in info.video().chain(info.audio()) {
            args.extend(["-map".into(), format!("0:{}", s.index)]);
        }
        if matroska {
            args.extend(["-map", "0:s?"].map(String::from));
        }
        if args.len() == 5 {
            return Err("This file has no video or audio.".into());
        }
        args.extend(
            [
                "-c",
                "copy",
                // Global, per-stream and chapter metadata all off; bitexact stops
                // ffmpeg writing its own encoder tags.
                "-map_metadata",
                "-1",
                "-map_metadata:s",
                "-1",
                "-map_chapters",
                "-1",
                "-fflags",
                "+bitexact",
                "-flags:v",
                "+bitexact",
                "-flags:a",
                "+bitexact",
            ]
            .map(String::from),
        );
        if !matroska {
            args.extend(["-movflags", "+faststart"].map(String::from));
        }
        args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
        let output = ctx.output_next_to(file, "_clean", &ext);
        args.push(output.display().to_string());

        Ok(Plan {
            title: format!("Strip metadata: {}", input.short_name()),
            steps: vec![Step {
                label: "Stripping".into(),
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
