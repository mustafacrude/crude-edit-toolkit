//! Make proxies: small edit-friendly copies for Premiere's Attach Proxies. Same
//! frame rate, duration and audio channels as the original (Premiere refuses to
//! attach a proxy that differs), in a Proxies folder beside it.

use super::schema::{Field, choice};
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, Suggestion, opt_str};
use crate::encode;
use crate::media::{self, MediaInfo};
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::tools::Tool;

pub struct Proxies;

impl Module for Proxies {
    fn id(&self) -> &'static str {
        "proxy"
    }

    fn name(&self) -> &'static str {
        "Make proxies"
    }

    fn description(&self) -> &'static str {
        "Makes a small, fast-to-edit copy in a Proxies folder next to the original. In Premiere, right-click the clip → Proxy → Attach Proxies, and it edits smoothly but exports at full quality."
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
                    choice("prores", "ProRes Proxy"),
                    choice("dnxhr", "DNxHR LB"),
                ],
                default: "prores",
                help: Some("Both work in Premiere on Windows and Mac."),
            },
            Field::Select {
                key: "size",
                label: "Size",
                options: vec![choice("half", "Half"), choice("quarter", "Quarter")],
                default: "half",
                help: None,
            },
        ]
    }

    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        let video = info.video().next()?;
        (video.height? >= 2160).then(|| Suggestion {
            reason: "4K footage. Proxies are small copies Premiere edits smoothly, then swaps back to the original for export.".into(),
            priority: 30,
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("This tool works on files, not links.")?;
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        let divisor = if opt_str(options, "size", "half") == "quarter" {
            8
        } else {
            4
        };
        let codec: &[&str] = if opt_str(options, "format", "prores") == "dnxhr" {
            &[
                "-c:v",
                "dnxhd",
                "-profile:v",
                "dnxhr_lb",
                "-pix_fmt",
                "yuv422p",
            ]
        } else {
            &[
                "-c:v",
                "prores_ks",
                "-profile:v",
                "0",
                "-pix_fmt",
                "yuv422p10le",
            ]
        };

        let output = ctx.output_in(file, "Proxies", "_Proxy", "mov");
        let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-n", "-i"]
            .map(String::from)
            .to_vec();
        args.push(file.display().to_string());
        args.extend(encode::map_args(&info)?);
        // Even dimensions, a half or quarter of the original's.
        args.extend([
            "-vf".into(),
            format!("scale=trunc(iw/{divisor})*2:trunc(ih/{divisor})*2"),
        ]);
        args.extend(codec.iter().map(|s| s.to_string()));
        args.extend(["-c:a", "pcm_s16le", "-ar", "48000", "-map_metadata", "0"].map(String::from));
        args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
        args.push(output.display().to_string());

        Ok(Plan {
            title: format!("Proxy: {}", input.short_name()),
            steps: vec![Step {
                label: "Making proxy".into(),
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
