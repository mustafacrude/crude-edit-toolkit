//! Repair damaged video: rebuilds a recording that was cut off (power lost, a
//! crash, a card pulled out) with untrunc. It needs a second file, a working
//! video from the same camera with the same settings, to learn how the camera
//! lays its files out.
//!
//! untrunc is downloaded on request rather than bundled (see untrunc_update.rs).

use std::path::Path;

use super::schema::Field;
use super::{Category, JobInput, Lane, Module, Plan, PlanContext, Step, opt_str};
use crate::media;
use crate::progress::ProgressKind;
use crate::tools::Tool;
use crate::untrunc_update;

pub struct Untrunc;

/// The containers untrunc understands; the repaired copy keeps the original's.
const EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "3gp", "3g2"];

impl Module for Untrunc {
    fn id(&self) -> &'static str {
        "untrunc"
    }

    fn name(&self) -> &'static str {
        "Repair damaged video"
    }

    fn description(&self) -> &'static str {
        "Rebuilds an MP4 or MOV recording that was cut off, so it opens again. Choose a working video from the same camera with the same settings: untrunc learns from it how the camera writes its files."
    }

    fn category(&self) -> Category {
        Category::Video
    }

    fn available(&self) -> bool {
        cfg!(windows)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn options_schema(&self) -> Vec<Field> {
        vec![
            Field::File {
                key: "reference",
                label: "Working example",
                help: Some(
                    "A video that plays normally, recorded by the same camera with the same settings. The more alike, the better the repair.",
                ),
                required: true,
            },
            Field::Toggle {
                key: "skip",
                label: "Skip over damaged sections",
                default: false,
                help: Some(
                    "Try this if the repair stops early. It's slower, and the picture may glitch where the damage was.",
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
        let file = input.file().ok_or("Repair works on files, not links.")?;
        if !untrunc_update::is_installed(ctx.app) {
            return Err(
                "untrunc hasn't been downloaded yet. Use the Download button above the options."
                    .into(),
            );
        }
        let reference = reference_file(opt_str(options, "reference", ""))?;
        if reference == file {
            return Err(
                "The working example has to be a different file from the damaged one.".into(),
            );
        }
        // If the example can't be read either, nothing can be learned from it.
        media::probe(&ctx.tool(Tool::Ffprobe)?, &reference).map_err(|_| {
            format!(
                "The working example, {}, can't be read. Choose a video that plays normally.",
                reference.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            )
        })?;

        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .filter(|e| EXTENSIONS.contains(&e.as_str()))
            .unwrap_or_else(|| "mp4".into());
        let output = ctx.output_next_to(file, " (repaired)", &ext);

        // -n: never wait for a key press. -dst with a file name picks the output
        // name; untrunc would otherwise make "<name>.mp4_fixed.mp4" beside the original.
        let mut args = vec!["-n".to_string()];
        if options
            .get("skip")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            args.push("-s".into());
        }
        args.extend([
            "-dst".into(),
            output.display().to_string(),
            reference.display().to_string(),
            file.display().to_string(),
        ]);

        Ok(Plan {
            title: format!("Repair {}", input.short_name()),
            steps: vec![Step {
                label: "Repairing".into(),
                tool: Tool::Untrunc,
                args,
                progress: ProgressKind::Untrunc,
                redact: vec![],
            }],
            outputs: vec![output],
            ..Default::default()
        })
    }
}

/// The chosen working example as a file that exists, or a sentence saying what's wrong.
fn reference_file(raw: &str) -> Result<std::path::PathBuf, String> {
    if raw.trim().is_empty() {
        return Err(
            "Choose a working video from the same camera first. untrunc learns from it how the camera writes its files."
                .into(),
        );
    }
    match JobInput::parse(raw) {
        Ok(JobInput::File(path)) => Ok(path),
        Ok(JobInput::Url(_)) => Err("The working example has to be a file on this computer.".into()),
        Err(e) => Err(format!("The working example: {e}")),
    }
}

#[allow(dead_code)]
fn _extension_check(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_working_example_must_be_a_real_file() {
        assert!(reference_file("").unwrap_err().contains("Choose a working video"));
        assert!(reference_file("   ").is_err());
        assert!(reference_file("https://example.com/a.mp4").is_err());
        assert!(reference_file("-n").unwrap_err().contains("working example"));
        let here = std::env::current_exe().unwrap();
        assert_eq!(reference_file(&here.display().to_string()).unwrap(), dunce::canonicalize(&here).unwrap());
    }

    #[test]
    fn the_form_asks_for_the_example_and_offers_skipping() {
        let schema = Untrunc.options_schema();
        assert!(matches!(
            schema[0],
            Field::File { key: "reference", required: true, .. }
        ));
        assert!(matches!(schema[1], Field::Toggle { key: "skip", default: false, .. }));
    }
}
