//! Trim & merge: join several files into one, each optionally trimmed. When
//! every file has the same codecs, size and frame rate the join is lossless (the
//! concat demuxer, cutting at keyframes); otherwise everything is re-encoded to
//! match the first file.

use std::fmt::Write as _;
use std::path::Path;

use super::schema::Field;
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, remux};
use crate::encode::{self, EncodeSettings};
use crate::encoders::{self, Codec};
use crate::media::{self, MediaInfo};
use crate::progress::{FFMPEG_PROGRESS_ARGS, ProgressKind};
use crate::timecode::{parse_time, secs};
use crate::tools::Tool;

pub struct TrimMerge;

struct Part {
    path: std::path::PathBuf,
    info: MediaInfo,
    start: Option<f64>,
    end: Option<f64>,
}

impl Part {
    fn length(&self) -> Option<f64> {
        let end = self.end.or(self.info.duration)?;
        Some(end - self.start.unwrap_or(0.0))
    }
}

/// The parts can be joined without re-encoding when their streams line up.
fn same_shape(parts: &[Part]) -> bool {
    let shape = |i: &MediaInfo| {
        let v = i.video().next().map(|v| {
            (
                v.codec().to_string(),
                v.width,
                v.height,
                v.pix_fmt.clone(),
                v.r_frame_rate.clone(),
            )
        });
        let a: Vec<_> = i
            .audio()
            .map(|a| (a.codec().to_string(), a.sample_rate.clone(), a.channels))
            .collect();
        (v, a)
    };
    let first = shape(&parts[0].info);
    parts.iter().all(|p| shape(&p.info) == first)
}

/// One line of a concat list. Single quotes are the only thing to escape.
fn concat_entry(path: &Path, start: Option<f64>, end: Option<f64>) -> String {
    let quoted = path.display().to_string().replace('\'', "'\\''");
    let mut s = format!("file '{quoted}'\n");
    if let Some(start) = start {
        let _ = writeln!(s, "inpoint {}", secs(start));
    }
    if let Some(end) = end {
        let _ = writeln!(s, "outpoint {}", secs(end));
    }
    s
}

impl Module for TrimMerge {
    fn id(&self) -> &'static str {
        "merge"
    }

    fn name(&self) -> &'static str {
        "Trim & merge"
    }

    fn description(&self) -> &'static str {
        "Joins clips into one file, in order, each trimmed if you like. Matching files join losslessly in seconds; mixed ones are re-encoded to match the first."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        vec![Field::TrimList { key: "clips" }]
    }

    fn combines_inputs(&self) -> bool {
        true
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        self.plan_combined(ctx, std::slice::from_ref(input), options)
    }

    fn plan_combined(
        &self,
        ctx: &PlanContext,
        inputs: &[JobInput],
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        if inputs.is_empty() {
            return Err("Choose the files to merge.".into());
        }
        let ffprobe = ctx.tool(Tool::Ffprobe)?;
        let trims = options.get("clips").and_then(|v| v.as_array());
        let time_at = |i: usize, key: &str| -> Result<Option<f64>, String> {
            let raw = trims
                .and_then(|t| t.get(i))
                .and_then(|c| c.get(key))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if raw.is_empty() {
                return Ok(None);
            }
            parse_time(raw)
                .map(Some)
                .ok_or_else(|| format!("File {}: \"{raw}\" isn't a time.", i + 1))
        };

        let mut parts = Vec::new();
        for (i, input) in inputs.iter().enumerate() {
            let path = input
                .file()
                .ok_or("Trim & merge works on files, not links.")?;
            let info = media::probe(&ffprobe, path)?;
            info.video()
                .next()
                .ok_or_else(|| format!("{} has no video.", input.short_name()))?;
            let (start, end) = (time_at(i, "start")?, time_at(i, "end")?);
            if let (Some(s), Some(e)) = (start, end)
                && e <= s
            {
                return Err(format!("File {}: the end must be after the start.", i + 1));
            }
            parts.push(Part {
                path: path.to_path_buf(),
                info,
                start,
                end,
            });
        }

        let first = &parts[0];
        let duration: Option<f64> = parts.iter().map(Part::length).sum();
        let output = ctx.output_next_to(&first.path, "_merged", "mp4");
        let mut args: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-n".into()];
        let mut plan = Plan {
            title: format!("Merge {} files", parts.len()),
            ..Default::default()
        };
        let label = if same_shape(&parts) {
            // Trimmed parts are cut first, losslessly, into scratch files. Trimming
            // inside the concat list instead (inpoint/outpoint) keeps the frames
            // before each cut, and their overlapping timestamps make the joined file
            // variable frame rate, which is exactly what Premiere chokes on.
            let mut text = String::new();
            for (i, p) in parts.iter().enumerate() {
                if p.start.is_none() && p.end.is_none() {
                    text.push_str(&concat_entry(&p.path, None, None));
                    continue;
                }
                let ext = p
                    .path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_else(|| "mkv".into());
                let cut = ctx.temp_path(&format!("part{}", i + 1), &ext);
                let mut cut_args: Vec<String> =
                    vec!["-hide_banner".into(), "-nostdin".into(), "-n".into()];
                if let Some(s) = p.start {
                    cut_args.extend(["-ss".into(), secs(s)]);
                }
                cut_args.extend(["-i".into(), p.path.display().to_string()]);
                if let Some(len) = p.end.map(|e| e - p.start.unwrap_or(0.0)) {
                    cut_args.extend(["-t".into(), secs(len)]);
                }
                for s in p.info.video().chain(p.info.audio()) {
                    cut_args.extend(["-map".into(), format!("0:{}", s.index)]);
                }
                cut_args
                    .extend(["-c", "copy", "-avoid_negative_ts", "make_zero"].map(String::from));
                cut_args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
                cut_args.push(cut.display().to_string());
                plan.steps.push(Step {
                    label: format!("Trimming file {} (no re-encode)", i + 1),
                    tool: Tool::Ffmpeg,
                    args: cut_args,
                    progress: ProgressKind::Ffmpeg {
                        duration: p.length(),
                    },
                    redact: vec![],
                });
                plan.temp_files.push(cut.clone());
                text.push_str(&concat_entry(&cut, None, None));
            }
            let list = ctx.temp_path("concat", "txt");
            std::fs::write(&list, text).map_err(|e| format!("Couldn't prepare the merge: {e}"))?;
            plan.temp_files.push(list.clone());
            args.extend(["-f", "concat", "-safe", "0", "-i"].map(String::from));
            args.push(list.display().to_string());
            // Same streams as the first file, audio Premiere can't read made AAC.
            args.extend(remux::stream_args(&first.info)?);
            "Joining (no re-encode)".to_string()
        } else {
            if parts.iter().any(|p| p.info.audio().next().is_none())
                && parts.iter().any(|p| p.info.audio().next().is_some())
            {
                return Err("Some of these files have audio and some don't. Merging a mix like that arrives in a later update.".into());
            }
            let video = first.info.video().next().ok_or("No video.")?;
            let (w, h) = (video.width.unwrap_or(1920), video.height.unwrap_or(1080));
            let rate = video.r_frame_rate.clone().unwrap_or_else(|| "30".into());
            let with_audio = first.info.audio().next().is_some();
            for p in &parts {
                if let Some(s) = p.start {
                    args.extend(["-ss".into(), secs(s)]);
                }
                if let Some(e) = p.end {
                    args.extend(["-to".into(), secs(e)]);
                }
                args.extend(["-i".into(), p.path.display().to_string()]);
            }
            // Scale and pad each clip to the first one's frame, then concatenate.
            let mut graph = String::new();
            let mut joined = String::new();
            for i in 0..parts.len() {
                let _ = write!(
                    graph,
                    "[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={rate},format=yuv420p[v{i}];"
                );
                let _ = write!(joined, "[v{i}]");
                if with_audio {
                    let _ = write!(
                        graph,
                        "[{i}:a:0]aresample=48000,aformat=channel_layouts=stereo[a{i}];"
                    );
                    let _ = write!(joined, "[a{i}]");
                }
            }
            let _ = write!(
                graph,
                "{joined}concat=n={}:v=1:a={}[v]{}",
                parts.len(),
                u8::from(with_audio),
                if with_audio { "[a]" } else { "" }
            );
            args.extend(["-filter_complex".into(), graph, "-map".into(), "[v]".into()]);
            if with_audio {
                args.extend(["-map".into(), "[a]".into()]);
            }
            let settings = EncodeSettings::default();
            let encoder = ctx.encoder(Codec::H264);
            args.extend(encode::video_args(&encoder, &settings, video)?);
            if with_audio {
                args.extend(encode::audio_args(&settings));
            }
            format!(
                "Merging, re-encoding with {} (the files don't match)",
                encoders::label(&encoder)
            )
        };

        args.extend(["-movflags", "+faststart"].map(String::from));
        args.extend(FFMPEG_PROGRESS_ARGS.map(String::from));
        args.push(output.display().to_string());
        plan.steps.push(Step {
            label,
            tool: Tool::Ffmpeg,
            args,
            progress: ProgressKind::Ffmpeg { duration },
            redact: vec![],
        });
        plan.outputs.push(output);
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_list_escapes_quotes() {
        assert_eq!(
            concat_entry(Path::new(r"D:\Raw\it's here.mkv"), Some(12.5), None),
            "file 'D:\\Raw\\it'\\''s here.mkv'\ninpoint 12.5\n"
        );
    }
}
