//! Clip parts: paste a list of time ranges, get one file per range. Fast cuts
//! copy the streams (lossless, starts at the nearest keyframe); exact cuts
//! re-encode with the Premiere-safe preset (frame-accurate).

use super::schema::{Field, choice};
use super::{
    Category, JobInput, Lane, Module, Plan, PlanContext, Step, file_label, opt_lines, opt_str,
    remux,
};
use crate::encode::{self, EncodeSettings};
use crate::encoders::{self, Codec};
use crate::media;
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::timecode::{file_stamp, parse_ranges, secs};
use crate::tools::Tool;

pub struct Clip;

impl Module for Clip {
    fn id(&self) -> &'static str {
        "clip"
    }

    fn name(&self) -> &'static str {
        "Clip parts"
    }

    fn description(&self) -> &'static str {
        "Paste time ranges, one per line, and get one file for each. Add a label after a range to name the file."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        vec![
            Field::Lines {
                key: "ranges",
                label: "Ranges",
                placeholder: "1:02:10-1:03:45 the clutch\n2:15:00-2:16:30",
                help: Some("One per line: start-end, then an optional label for the file name."),
            },
            Field::Select {
                key: "cut",
                label: "Cut",
                options: vec![choice("fast", "Fast"), choice("exact", "Exact")],
                default: "fast",
                help: Some(
                    "Fast copies without re-encoding but starts at the nearest keyframe, up to a couple of seconds early. Exact is frame-accurate and re-encodes as Premiere-safe H.264.",
                ),
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
        let ranges = parse_ranges(&opt_lines(options, "ranges"))?;
        if ranges.is_empty() {
            return Err("Add at least one time range, like 1:02:10-1:03:45.".into());
        }
        let exact = opt_str(options, "cut", "fast") == "exact";
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        if let Some(duration) = info.duration
            && let Some((i, _)) = ranges.iter().enumerate().find(|(_, r)| r.start >= duration)
        {
            return Err(format!(
                "Line {} starts after the video ends ({}).",
                i + 1,
                file_stamp(duration).replace('-', ":")
            ));
        }

        // Fast cuts keep the container when it's already MP4/MOV, otherwise they
        // remux to MP4 (converting audio Premiere can't read).
        let source_ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let (ext, stream_args) = if exact {
            ("mp4".to_string(), Vec::new())
        } else if info.is_mp4_family() {
            let mut a = Vec::new();
            for s in info.video().chain(info.audio()) {
                a.extend(["-map".into(), format!("0:{}", s.index)]);
            }
            a.extend(["-c".into(), "copy".into()]);
            (source_ext, a)
        } else {
            let a = remux::stream_args(&info)
                .map_err(|e| format!("{e} Or choose Exact cut, which re-encodes."))?;
            ("mp4".to_string(), a)
        };

        let settings = EncodeSettings::default();
        let encoder = if exact {
            ctx.encoder(Codec::H264)
        } else {
            String::new()
        };
        let mut plan = Plan {
            title: format!(
                "Clip {} from {}",
                if ranges.len() == 1 {
                    "1 part".to_string()
                } else {
                    format!("{} parts", ranges.len())
                },
                input.short_name()
            ),
            ..Default::default()
        };
        for (i, range) in ranges.iter().enumerate() {
            let name = match range
                .label
                .as_deref()
                .map(file_label)
                .filter(|l| !l.is_empty())
            {
                Some(label) => format!("_{label}"),
                None => format!("_{}_{}", file_stamp(range.start), file_stamp(range.end)),
            };
            let output = ctx.output_next_to(file, &name, &ext);
            let length = range.end - range.start;

            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-nostdin".into(),
                "-n".into(),
                "-ss".into(),
                secs(range.start),
                "-i".into(),
                file.display().to_string(),
                "-t".into(),
                secs(length),
            ];
            if exact {
                let video = info.video().next().ok_or("This file has no video.")?;
                args.extend(encode::map_args(&info)?);
                args.extend(encode::video_args(&encoder, &settings, video)?);
                args.extend(encode::audio_args(&settings));
            } else {
                args.extend(stream_args.iter().cloned());
                args.extend(["-avoid_negative_ts", "make_zero"].map(String::from));
            }
            args.extend(["-map_metadata", "0", "-movflags", "+faststart"].map(String::from));
            args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
            args.push(output.display().to_string());

            plan.steps.push(Step {
                label: format!(
                    "{} part {} of {}",
                    if exact {
                        format!("Encoding with {},", encoders::label(&encoder))
                    } else {
                        "Cutting".into()
                    },
                    i + 1,
                    ranges.len()
                ),
                tool: Tool::Ffmpeg,
                args,
                progress: ProgressKind::Ffmpeg {
                    duration: Some(length),
                },
                redact: vec![],
            });
            plan.outputs.push(output);
        }
        Ok(plan)
    }
}
