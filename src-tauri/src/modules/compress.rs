//! Compress to size: a two-pass H.264 encode aimed at a file size, for upload
//! caps like Discord's. Two passes with x264 hit the size far more reliably than
//! a single hardware pass, so this one uses the CPU.

use super::schema::{Field, ShowWhen, choice};
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, opt_str};
use crate::media;
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::timecode::file_stamp;
use crate::tools::Tool;

pub struct Compress;

/// Aim this far under the cap: containers and rate control both overshoot a little.
const MARGIN: f64 = 0.94;
const AUDIO_KBPS: f64 = 128.0;

/// The height to scale down to, so a low bitrate still looks clean.
fn height_cap(video_kbps: f64) -> Option<u32> {
    match video_kbps {
        k if k >= 6000.0 => None,
        k if k >= 3000.0 => Some(1080),
        k if k >= 1500.0 => Some(720),
        k if k >= 700.0 => Some(480),
        _ => Some(360),
    }
}

impl Module for Compress {
    fn id(&self) -> &'static str {
        "compress"
    }

    fn name(&self) -> &'static str {
        "Compress to size"
    }

    fn description(&self) -> &'static str {
        "Shrinks a clip to fit an upload limit, like Discord's 10 MB. Lowers the resolution when it has to, so it stays watchable."
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
                key: "target",
                label: "Size",
                options: vec![
                    choice("10", "10 MB"),
                    choice("50", "50 MB"),
                    choice("500", "500 MB"),
                    choice("custom", "Custom"),
                ],
                default: "10",
                help: Some("Discord: 10 MB free, 50 MB with Nitro Basic, 500 MB with Nitro."),
            },
            Field::Number {
                key: "customMb",
                label: "Custom size",
                min: 1.0,
                max: 4000.0,
                step: 1.0,
                unit: "MB",
                default: 25.0,
                help: None,
                show_when: Some(ShowWhen {
                    key: "target",
                    value: "custom",
                }),
                slider: false,
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
        let target_mb: f64 = match opt_str(options, "target", "10") {
            "custom" => options
                .get("customMb")
                .and_then(|v| v.as_f64())
                .filter(|v| (1.0..=4000.0).contains(v))
                .ok_or("Enter a size between 1 and 4000 MB.")?,
            preset => preset.parse().map_err(|_| "Unknown size.")?,
        };
        let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
        let video = info.video().next().ok_or("This file has no video.")?;
        let duration = info
            .duration
            .filter(|d| *d > 0.0)
            .ok_or("Couldn't tell how long this video is.")?;
        let has_audio = info.audio().next().is_some();

        let total_kbps = target_mb * 1_000_000.0 * 8.0 * MARGIN / 1000.0 / duration;
        let video_kbps = (total_kbps - if has_audio { AUDIO_KBPS } else { 0.0 }).floor();
        if video_kbps < 150.0 {
            return Err(format!(
                "At {} long this won't fit in {target_mb} MB and still be watchable. Clip it shorter first, or choose a bigger size.",
                file_stamp(duration).replace('-', ":")
            ));
        }

        let mut filters = Vec::new();
        if let (Some(cap), Some(h)) = (height_cap(video_kbps), video.height)
            && h > cap
        {
            filters.push(format!("scale=-2:{cap}"));
        }
        // Low bitrates look far better at 30 fps than at 60.
        if video_kbps < 1500.0 && video.nominal_fps().is_some_and(|f| f > 31.0) {
            filters.push("fps=30".into());
        }

        let passlog = ctx.temp_path("x264pass", "log");
        let passlog_prefix = passlog.with_extension("");
        let rate = format!("{video_kbps}k");
        let common = |pass: &str| -> Vec<String> {
            let mut a: Vec<String> = vec![
                "-hide_banner".into(),
                "-nostdin".into(),
                "-i".into(),
                file.display().to_string(),
                "-map".into(),
                format!("0:{}", video.index),
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "medium".into(),
                "-b:v".into(),
                rate.clone(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-pass".into(),
                pass.into(),
                "-passlogfile".into(),
                passlog_prefix.display().to_string(),
            ];
            if !filters.is_empty() {
                a.extend(["-vf".into(), filters.join(",")]);
            }
            a
        };

        let mut first = common("1");
        first.extend(["-an", "-f", "null"].map(String::from));
        first.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
        first.push("-".into());

        let output = ctx.output_next_to(file, &format!("_{target_mb}MB"), "mp4");
        let mut second = common("2");
        second.insert(2, "-n".into());
        if has_audio {
            second.extend(
                [
                    "-map", "0:a:0", "-c:a", "aac", "-b:a", "128k", "-ac", "2", "-ar", "48000",
                ]
                .map(String::from),
            );
        }
        second.extend(["-movflags", "+faststart"].map(String::from));
        second.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
        second.push(output.display().to_string());

        let mut log0 = passlog_prefix.clone().into_os_string();
        log0.push("-0.log");
        let mut mbtree = log0.clone();
        mbtree.push(".mbtree");

        Ok(Plan {
            title: format!("Compress to {target_mb} MB: {}", input.short_name()),
            steps: vec![
                Step {
                    label: "Analysing (pass 1 of 2)".into(),
                    tool: Tool::Ffmpeg,
                    args: first,
                    progress: ProgressKind::Ffmpeg {
                        duration: Some(duration),
                    },
                    redact: vec![],
                },
                Step {
                    label: "Compressing (pass 2 of 2)".into(),
                    tool: Tool::Ffmpeg,
                    args: second,
                    progress: ProgressKind::Ffmpeg {
                        duration: Some(duration),
                    },
                    redact: vec![],
                },
            ],
            outputs: vec![output],
            temp_files: vec![log0.into(), mbtree.into()],
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_follow_bitrate() {
        assert_eq!(height_cap(8000.0), None);
        assert_eq!(height_cap(2500.0), Some(720));
        assert_eq!(height_cap(200.0), Some(360));
    }
}
