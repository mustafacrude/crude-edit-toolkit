//! User preferences, stored as JSON in the app's config folder.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Friends land here on first launch (BRIEF §11.2).
    #[default]
    Simple,
    Pro,
}

/// How downloads sign in for members-only, sub-only or age-restricted videos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignIn {
    #[default]
    None,
    /// `--cookies-from-browser <browser>`
    Browser,
    /// A cookies.txt file the user exported.
    File,
}

pub const BROWSERS: &[&str] = &[
    "firefox", "chrome", "edge", "brave", "opera", "vivaldi", "safari",
];

/// Fields the naming template may use, as `{field}`.
pub const NAMING_FIELDS: &[&str] = &["title", "uploader", "upload_date", "id", "site"];
pub const DEFAULT_NAMING: &str = "{uploader}_{upload_date}_{title}";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub mode: Mode,
    /// Downloads that may run at once. Encodes always run one at a time.
    pub download_concurrency: u8,
    /// Where downloads go. Filled with Videos\Crude Toolkit on first run.
    pub download_dir: String,
    /// e.g. "{uploader}_{upload_date}_{title}"
    pub naming_template: String,
    pub sign_in: SignIn,
    pub browser: String,
    /// Only the path is stored; the file itself is never copied here or logged.
    pub cookies_file: Option<String>,
    /// "auto" (best working GPU encoder), a family ("nvenc", "amf", "qsv",
    /// "videotoolbox") or "software".
    pub encoder: String,
    /// Share of the CPU all tools may use together, in percent: 100, 75 or 50.
    /// Tools always run at low priority; this also hard-caps them (Windows).
    pub cpu_limit: u8,
    /// Speed limit for each download, in MB/s. 0: no limit.
    pub speed_limit_mb: f64,
    /// The first-run setup has been finished (or skipped).
    pub first_run_done: bool,
    /// A desktop notification when the queue finishes while the window isn't in front.
    pub notify_when_done: bool,
    /// Look for a newer app and a newer yt-dlp at launch (at most once a day).
    /// Nothing is installed without a click.
    pub check_for_updates: bool,
}

pub const ENCODER_CHOICES: &[&str] = &["auto", "nvenc", "amf", "qsv", "videotoolbox", "software"];

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Simple,
            download_concurrency: 3,
            download_dir: String::new(),
            naming_template: DEFAULT_NAMING.into(),
            sign_in: SignIn::None,
            // Chrome and Edge on Windows encrypt cookies so other apps can't read them.
            browser: "firefox".into(),
            cookies_file: None,
            encoder: "auto".into(),
            cpu_limit: 100,
            speed_limit_mb: 0.0,
            first_run_done: false,
            notify_when_done: true,
            check_for_updates: true,
        }
    }
}

impl Settings {
    fn validate(&mut self) -> Result<(), String> {
        self.download_concurrency = self.download_concurrency.clamp(1, 6);
        self.cpu_limit = match self.cpu_limit {
            0..=62 => 50,
            63..=87 => 75,
            _ => 100,
        };
        if !self.speed_limit_mb.is_finite() || self.speed_limit_mb < 0.0 {
            self.speed_limit_mb = 0.0;
        }
        self.speed_limit_mb = self.speed_limit_mb.min(1000.0);
        if self.naming_template.chars().count() > 200 {
            return Err("The naming template is too long; keep it under 200 characters.".into());
        }
        naming_to_ytdlp(&self.naming_template)?;
        if !BROWSERS.contains(&self.browser.as_str()) {
            return Err(format!("Unknown browser: {}", self.browser));
        }
        if !ENCODER_CHOICES.contains(&self.encoder.as_str()) {
            return Err(format!("Unknown encoder choice: {}", self.encoder));
        }
        if self.download_dir.trim().is_empty() {
            return Err("Choose a folder for downloads.".into());
        }
        if !Path::new(self.download_dir.trim()).is_absolute() {
            return Err(
                "The download folder has to be a full path, like C:\\Users\\you\\Videos.".into(),
            );
        }
        Ok(())
    }
}

/// Turns "{uploader}_{upload_date}_{title}" into a yt-dlp output template.
/// Only known fields are allowed, and no folders, so a template can't write elsewhere.
pub fn naming_to_ytdlp(template: &str) -> Result<String, String> {
    render_naming(
        template,
        |text| text.replace('%', "%%"),
        |field| {
            match field {
                "title" => "%(title)s",
                "uploader" => "%(uploader,channel,creator|unknown)s",
                "upload_date" => "%(upload_date|nodate)s",
                "id" => "%(id)s",
                _ => "%(extractor_key)s",
            }
            .into()
        },
    )
}

/// The file name (no extension) yt-dlp gives a video with these details, for
/// files another tool writes beside it (Twitch chat). `value` gets a field name
/// and returns its value, with upload_date as YYYYMMDD.
pub fn naming_render(
    template: &str,
    value: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let name = render_naming(template, str::to_string, |field| {
        let v = value(field).filter(|v| !v.trim().is_empty());
        safe_file_name(&v.unwrap_or_else(|| {
            match field {
                "uploader" => "unknown",
                "upload_date" => "nodate",
                _ => "NA",
            }
            .into()
        }))
    })?;
    // Like yt-dlp's --trim-filenames 160.
    Ok(name
        .chars()
        .take(160)
        .collect::<String>()
        .trim()
        .to_string())
}

/// Swaps characters Windows forbids in file names for look-alikes, as yt-dlp's
/// --windows-filenames does.
pub fn safe_file_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '"' => '\u{ff02}',
            '*' => '\u{ff0a}',
            ':' => '\u{ff1a}',
            '<' => '\u{ff1c}',
            '>' => '\u{ff1e}',
            '?' => '\u{ff1f}',
            '|' => '\u{ff5c}',
            '/' => '\u{29f8}',
            '\\' => '\u{29f9}',
            c => c,
        })
        .collect()
}

/// Checks the naming template, mapping its literal text and each `{field}`.
fn render_naming(
    template: &str,
    literal: impl Fn(&str) -> String,
    field: impl Fn(&str) -> String,
) -> Result<String, String> {
    let template = template.trim();
    if template.is_empty() {
        return Err("The naming template can't be empty.".into());
    }
    if template.contains(['/', '\\']) {
        return Err("The naming template can't contain folders (/ or \\).".into());
    }
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&literal(&rest[..open]));
        let close = rest[open..]
            .find('}')
            .ok_or("A { in the naming template is never closed.")?
            + open;
        let name = &rest[open + 1..close];
        if !NAMING_FIELDS.contains(&name) {
            return Err(format!(
                "{{{name}}} isn't a field the naming template knows. Use {}.",
                NAMING_FIELDS
                    .iter()
                    .map(|f| format!("{{{f}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        out.push_str(&field(name));
        rest = &rest[close + 1..];
    }
    out.push_str(&literal(rest));
    Ok(out)
}

pub struct SettingsStore {
    path: PathBuf,
    value: Mutex<Settings>,
}

impl SettingsStore {
    /// Loads settings, falling back to defaults if the file is missing or unreadable.
    pub fn load(path: PathBuf, default_download_dir: PathBuf) -> Self {
        let mut value: Settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if value.download_dir.trim().is_empty() {
            value.download_dir = default_download_dir.display().to_string();
        }
        if naming_to_ytdlp(&value.naming_template).is_err() {
            value.naming_template = DEFAULT_NAMING.into();
        }
        Self {
            path,
            value: Mutex::new(value),
        }
    }

    pub fn get(&self) -> Settings {
        self.value.lock().unwrap().clone()
    }

    pub fn set(&self, mut new: Settings) -> Result<Settings, String> {
        new.validate()?;
        let json = serde_json::to_string_pretty(&new).map_err(|e| e.to_string())?;
        // Write then rename, so a crash mid-write never leaves a half-written file.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("Couldn't save settings: {e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("Couldn't save settings: {e}"))?;
        *self.value.lock().unwrap() = new.clone();
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naming_template() {
        assert_eq!(
            naming_to_ytdlp("{uploader}_{upload_date}_{title}").unwrap(),
            "%(uploader,channel,creator|unknown)s_%(upload_date|nodate)s_%(title)s"
        );
        assert_eq!(naming_to_ytdlp("100% {id}").unwrap(), "100%% %(id)s");
        assert!(naming_to_ytdlp("{title").is_err());
        assert!(naming_to_ytdlp("{views}").unwrap_err().contains("{views}"));
        assert!(naming_to_ytdlp("..\\{title}").is_err());
        assert!(naming_to_ytdlp("   ").is_err());
    }

    #[test]
    fn naming_rendered() {
        let name = naming_render(DEFAULT_NAMING, |f| match f {
            "uploader" => Some("Twitch".into()),
            "title" => Some("Q&A: what's next?".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(name, "Twitch_nodate_Q&A\u{ff1a} what's next\u{ff1f}");
    }
}
