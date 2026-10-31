//! Frame grab: a still image at each timestamp.

use super::schema::{Field, choice};
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, opt_lines, opt_str};
use crate::media;
use crate::progress::ProgressKind;
use crate::timecode::{file_stamp, parse_time, secs};
use crate::tools::Tool;

pub struct FrameGrab;

impl Module for FrameGrab {
    fn id(&self) -> &'static str {
        "frame-grab"
    }

    fn name(&self) -> &'static str {
        "Frame grab"
    }

    fn description(&self) -> &'static str {
        "Saves the frame at each timestamp as a full-resolution PNG or WebP, for thumbnails and stills."
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
                key: "times",
                label: "Timestamps",
                placeholder: "1:02:10\n2:15:03.5",
                help: Some("One per line."),
            },
            Field::Select {
                key: "format",
                label: "Format",
                options: vec![choice("png", "PNG"), choice("webp", "WebP")],
                default: "png",
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
        let times = opt_lines(options, "times")
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .enumerate()
            .map(|(i, l)| {
                parse_time(&l).ok_or_else(|| {
                    format!(
                        "Line {}: \"{}\" isn't a time. Use something like 1:02:10.",
                        i + 1,
                        l.trim()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if times.is_empty() {
            return Err("Add at least one timestamp, like 1:02:10.".into());
        }
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        info.video().next().ok_or("This file has no video.")?;
        if let Some(duration) = info.duration
            && let Some((i, _)) = times.iter().enumerate().find(|(_, t)| **t >= duration)
        {
            return Err(format!("Line {} is after the video ends.", i + 1));
        }
        let webp = opt_str(options, "format", "png") == "webp";

        let mut plan = Plan {
            title: format!(
                "Grab {} from {}",
                if times.len() == 1 {
                    "1 frame".to_string()
                } else {
                    format!("{} frames", times.len())
                },
                input.short_name()
            ),
            ..Default::default()
        };
        for t in &times {
            let output = ctx.output_next_to(
                file,
                &format!("_{}", file_stamp(*t)),
                if webp { "webp" } else { "png" },
            );
            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-nostdin".into(),
                "-n".into(),
                "-ss".into(),
                secs(*t),
                "-i".into(),
                file.display().to_string(),
                "-frames:v".into(),
                "1".into(),
                "-update".into(),
                "1".into(),
            ];
            if webp {
                args.extend(["-c:v", "libwebp", "-quality", "92"].map(String::from));
            }
            args.push(output.display().to_string());
            plan.steps.push(Step {
                label: format!("Grabbing {}", file_stamp(*t).replace('-', ":")),
                tool: Tool::Ffmpeg,
                args,
                progress: ProgressKind::None,
                redact: vec![],
            });
            plan.outputs.push(output);
        }
        Ok(plan)
    }
}
