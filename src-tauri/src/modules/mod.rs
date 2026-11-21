//! Tool modules: each thing the toolkit can do (remux, download, strip…) is one
//! [`Module`]. The queue only knows about modules through this trait, so adding
//! a feature means adding a module, not touching the runner or the screens.
//! See docs/modules.md for the contract.

mod audio_extract;
mod clip;
mod compress;
pub mod download;
mod encode_tools;
mod frame_grab;
mod image_tools;
pub use image_tools::svg_size_args;
mod merge;
mod proxy;
mod remux;
pub mod schema;
mod strip;
mod untrunc;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::encoders::{Codec, Encoders};
use crate::media::MediaInfo;
use crate::progress::ProgressKind;
use crate::settings::{Settings, SettingsStore};
use crate::tools::Tool;
pub use schema::Field;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Download,
    Video,
    Image,
}

/// What a module works on, so the UI offers the right picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Accepts {
    Video,
    Image,
    Link,
}

/// Which queue lane a job waits in. Downloads run in parallel; anything that
/// leans on the CPU or GPU runs one at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    Download,
    Process,
}

/// One thing a job works on: a local file or a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobInput {
    File(PathBuf),
    Url(String),
}

impl JobInput {
    /// Validates raw input from the UI. Files must exist; links must be http(s).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let lower = raw.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(
                    "That link has spaces in it. Copy it again from the address bar.".into(),
                );
            }
            if raw.len() > 4096 {
                return Err("That link is far too long to be a video link.".into());
            }
            return Ok(JobInput::Url(raw.to_string()));
        }
        // Canonical absolute paths can never be mistaken for a tool flag.
        let name = raw.rsplit(['\\', '/']).next().unwrap_or(raw);
        let path = dunce::canonicalize(raw).map_err(|_| {
            format!("Can't find {name}. It may have been moved, renamed or deleted.")
        })?;
        if !path.is_file() {
            return Err(format!("{name} is a folder, not a file."));
        }
        Ok(JobInput::File(path))
    }

    pub fn file(&self) -> Option<&Path> {
        match self {
            JobInput::File(p) => Some(p),
            JobInput::Url(_) => None,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            JobInput::File(p) => p.display().to_string(),
            JobInput::Url(u) => u.clone(),
        }
    }

    /// Short name for job titles: the file name, or the link.
    pub fn short_name(&self) -> String {
        match self {
            JobInput::File(p) => p
                .file_name()
                .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into()),
            JobInput::Url(u) => u.clone(),
        }
    }
}

/// One tool invocation. Arguments are a vector, never a shell string (BRIEF §13.1).
#[derive(Clone, Debug)]
pub struct Step {
    pub label: String,
    pub tool: Tool,
    pub args: Vec<String>,
    pub progress: ProgressKind,
    /// Indexes into `args` hidden from Show command and logs (e.g. a cookies file path).
    pub redact: Vec<usize>,
}

impl Step {
    /// The command as text, for Show command. Display only; never executed.
    pub fn command_line(&self) -> String {
        let mut parts = vec![self.tool.name().to_lowercase()];
        for (i, arg) in self.args.iter().enumerate() {
            // exiftool reads its arguments from a file (see exif.rs); show them instead.
            if self.tool == Tool::Exiftool && i > 0 && self.args[i - 1] == "-@" {
                parts.pop();
                let inner = std::fs::read_to_string(arg).unwrap_or_default();
                parts.extend(inner.lines().map(quote));
                continue;
            }
            parts.push(if self.redact.contains(&i) {
                "<hidden>".to_string()
            } else {
                quote(arg)
            });
        }
        parts.join(" ")
    }
}

fn quote(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|c| c.is_whitespace() || "\"'&|;$()<>^%".contains(c))
    {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

/// What a module wants run for one input.
#[derive(Default)]
pub struct Plan {
    pub title: String,
    pub steps: Vec<Step>,
    /// Files the job creates. Deleted again if the job fails or is cancelled.
    pub outputs: Vec<PathBuf>,
    /// For tools that only know their output names once running: a file they
    /// write the final paths to, one per line. Those become the job's outputs.
    pub output_list: Option<PathBuf>,
    /// Scratch files (cookie copies, output lists), deleted when the job ends.
    pub temp_files: Vec<PathBuf>,
    /// Nothing to do, and why (e.g. "already Premiere-safe"). No job is queued.
    pub skip: Option<String>,
    /// Many outputs from one step (a playlist): a failure that still saved some
    /// files finishes with a warning and keeps them, and saving none isn't an error.
    pub partial_ok: bool,
    /// Folders the job writes to besides its outputs' own (a download folder,
    /// when names aren't known yet). The job won't start, and stops, when their
    /// drive is nearly full.
    pub writes_to: Vec<PathBuf>,
}

/// What a module gets while planning: tool locations, settings and output paths.
pub struct PlanContext<'a> {
    pub app: &'a AppHandle,
    pub reserved: &'a Mutex<HashSet<PathBuf>>,
    pub temp_dir: &'a Path,
}

impl PlanContext<'_> {
    pub fn settings(&self) -> Settings {
        self.app.state::<SettingsStore>().get()
    }

    /// A unique scratch path for this job. Add it to `Plan::temp_files`.
    pub fn temp_path(&self, prefix: &str, ext: &str) -> PathBuf {
        self.temp_dir
            .join(format!("{prefix}-{}.{ext}", uuid::Uuid::new_v4()))
    }

    pub fn tool(&self, tool: Tool) -> Result<PathBuf, String> {
        let path = tool.path(self.app)?;
        if !path.exists() {
            return Err(format!(
                "{} is missing from the app folder. Reinstall the app to restore it.",
                tool.name()
            ));
        }
        Ok(path)
    }

    /// The video encoder for `codec` on this machine, honouring Settings.
    pub fn encoder(&self, codec: Codec) -> String {
        let preference = self.settings().encoder;
        self.app
            .state::<Encoders>()
            .pick(self.app, codec, &preference)
    }

    /// A new, unused path in the `Toolkit` folder next to `input` (BRIEF §11.10).
    /// Never returns a file that exists or that another queued job will write.
    pub fn output_next_to(&self, input: &Path, suffix: &str, ext: &str) -> PathBuf {
        self.output_in(input, "Toolkit", suffix, ext)
    }

    /// Like [`Self::output_next_to`], in a differently named folder (e.g. "Proxies").
    pub fn output_in(&self, input: &Path, folder: &str, suffix: &str, ext: &str) -> PathBuf {
        let dir = input.parent().unwrap_or(Path::new(".")).join(folder);
        let stem = input
            .file_stem()
            .map_or_else(|| "output".into(), |s| s.to_string_lossy());
        self.unique_in(&dir, &format!("{stem}{suffix}"), ext)
    }

    /// A new, unused `dir/name.ext`, or `dir/name (2).ext` and so on.
    pub fn unique_in(&self, dir: &Path, name: &str, ext: &str) -> PathBuf {
        let mut reserved = self.reserved.lock().unwrap();
        let mut n = 1;
        loop {
            let name = match n {
                1 => format!("{name}.{ext}"),
                _ => format!("{name} ({n}).{ext}"),
            };
            let candidate = dir.join(name);
            if !candidate.exists() && !reserved.contains(&candidate) {
                reserved.insert(candidate.clone());
                return candidate;
            }
            n += 1;
        }
    }
}

/// A module offering to fix something about a dropped file.
pub struct Suggestion {
    /// Plain-language why, shown under the module's name.
    pub reason: String,
    /// 0..=100. The highest-priority suggestion is shown as recommended.
    pub priority: u8,
}

pub trait Module: Send + Sync {
    /// Stable id, stored in job history. Never rename one.
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// One or two plain sentences: what it does and what you get.
    fn description(&self) -> &'static str;
    fn category(&self) -> Category;
    /// False when the tool can't run on this computer (untrunc is Windows-only for now);
    /// it's then left out of the tool lists.
    fn available(&self) -> bool {
        true
    }
    fn lane(&self) -> Lane;
    fn accepts(&self) -> Accepts {
        Accepts::Video
    }
    /// The settings form. Options arrive in `plan` keyed by each field's `key`;
    /// a missing key means the field's default.
    fn options_schema(&self) -> Vec<Field> {
        Vec::new()
    }
    /// Whether this module fixes something about an inspected file, and why.
    fn suggest(&self, _info: &MediaInfo) -> Option<Suggestion> {
        None
    }
    /// Works out the tool runs for one input. Runs off the UI thread and may probe the file.
    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String>;
    /// True for modules that turn several inputs into one job (Trim & merge);
    /// they implement `plan_combined` instead of `plan`.
    fn combines_inputs(&self) -> bool {
        false
    }
    fn plan_combined(
        &self,
        _ctx: &PlanContext,
        _inputs: &[JobInput],
        _options: &serde_json::Value,
    ) -> Result<Plan, String> {
        Err("This tool works on one file at a time.".into())
    }
}

/// Reads `options[key]` as a string, or `default`.
pub fn opt_str<'a>(options: &'a serde_json::Value, key: &str, default: &'a str) -> &'a str {
    options.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

/// Reads `options[key]` as lines: an array of strings, or one string split on newlines.
pub fn opt_lines(options: &serde_json::Value, key: &str) -> Vec<String> {
    match options.get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Some(serde_json::Value::String(s)) => s.lines().map(String::from).collect(),
        _ => Vec::new(),
    }
}

/// Turns a user's label into something safe inside a file name.
pub fn file_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(60)
        .collect()
}

static MODULES: &[&dyn Module] = &[
    &download::Download,
    &encode_tools::PremiereSafe,
    &encode_tools::ConstantFrameRate,
    &remux::Remux,
    &encode_tools::Reencode,
    &clip::Clip,
    &merge::TrimMerge,
    &strip::StripMetadata,
    &frame_grab::FrameGrab,
    &audio_extract::AudioExtract,
    &compress::Compress,
    &proxy::Proxies,
    &untrunc::Untrunc,
    &image_tools::PremiereImage,
    &image_tools::StripImage,
    &image_tools::ConvertWebp,
];

pub fn all() -> &'static [&'static dyn Module] {
    MODULES
}

pub fn find(id: &str) -> Option<&'static dyn Module> {
    MODULES.iter().copied().find(|m| m.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_and_redacts() {
        let step = Step {
            label: "Test".into(),
            tool: Tool::Ffmpeg,
            args: vec![
                "-i".into(),
                "D:\\Raw\\my clip; rm -rf $(x).mkv".into(),
                "--cookies".into(),
                "C:\\secret.txt".into(),
            ],
            progress: ProgressKind::None,
            redact: vec![3],
        };
        assert_eq!(
            step.command_line(),
            "ffmpeg -i \"D:\\Raw\\my clip; rm -rf $(x).mkv\" --cookies <hidden>"
        );
    }

    #[test]
    fn rejects_bad_inputs() {
        assert!(JobInput::parse("https://youtu.be/abc").is_ok());
        assert!(JobInput::parse("https://youtu.be/a b").is_err());
        assert!(JobInput::parse("ftp://example.com/x").is_err());
        assert!(JobInput::parse("-version").is_err());
    }
}
