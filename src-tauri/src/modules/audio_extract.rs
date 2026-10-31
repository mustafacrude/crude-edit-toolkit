//! Extract audio: each audio track as its own file. WAV for editing, MP3 or
//! M4A for reference.

use super::schema::{Field, choice};
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, file_label, opt_str};
use crate::media;
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::tools::Tool;

pub struct AudioExtract;

impl Module for AudioExtract {
    fn id(&self) -> &'static str {
        "audio-extract"
    }

    fn name(&self) -> &'static str {
        "Extract audio"
    }

    fn description(&self) -> &'static str {
        "Saves the audio on its own: 24-bit WAV for editing, or MP3/M4A for reference. Multi-track recordings give one file per track."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        vec![
            Field::Select {
                key: "format",
                label: "Format",
                options: vec![
                    choice("wav", "WAV"),
                    choice("mp3", "MP3"),
                    choice("m4a", "M4A (AAC)"),
                ],
                default: "wav",
                help: Some(
                    "WAV is lossless and best for editing. MP3 and M4A are small, for listening or sending.",
                ),
            },
            Field::Select {
                key: "tracks",
                label: "Tracks",
                options: vec![choice("all", "Every track"), choice("first", "First only")],
                default: "all",
                help: None,
            },
        ]
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("This tool works on files, not links.")?;
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        let mut tracks: Vec<_> = info.audio().collect();
        if tracks.is_empty() {
            return Err("This file has no audio.".into());
        }
        if opt_str(options, "tracks", "all") == "first" {
            tracks.truncate(1);
        }
        let format = opt_str(options, "format", "wav");
        let (ext, codec): (&str, &[&str]) = match format {
            "mp3" => ("mp3", &["-c:a", "libmp3lame", "-b:a", "320k"]),
            "m4a" => ("m4a", &["-c:a", "aac", "-b:a", "256k"]),
            _ => ("wav", &["-c:a", "pcm_s24le"]),
        };

        let mut plan = Plan {
            title: format!("Extract audio: {}", input.short_name()),
            ..Default::default()
        };
        let many = tracks.len() > 1;
        for (n, track) in tracks.iter().enumerate() {
            // "_track2 Mic" when the recording named its tracks, as OBS can.
            let suffix = match (many, track.tags.get("title").map(|t| file_label(t))) {
                (false, _) => "_audio".to_string(),
                (true, Some(title)) if !title.is_empty() => format!("_track{} {title}", n + 1),
                (true, _) => format!("_track{}", n + 1),
            };
            let output = ctx.output_next_to(file, &suffix, ext);
            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-nostdin".into(),
                "-n".into(),
                "-i".into(),
                file.display().to_string(),
                "-map".into(),
                format!("0:{}", track.index),
                "-vn".into(),
            ];
            args.extend(codec.iter().map(|s| s.to_string()));
            args.extend(["-ar", "48000"].map(String::from));
            args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
            args.push(output.display().to_string());
            plan.steps.push(Step {
                label: if many {
                    format!("Extracting track {} of {}", n + 1, tracks.len())
                } else {
                    "Extracting".into()
                },
                tool: Tool::Ffmpeg,
                args,
                progress: ProgressKind::Ffmpeg {
                    duration: info.duration,
                },
                redact: vec![],
            });
            plan.outputs.push(output);
        }
        Ok(plan)
    }
}
