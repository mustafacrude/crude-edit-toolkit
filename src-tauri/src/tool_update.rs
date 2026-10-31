//! Updating yt-dlp from inside the app. Sites change often and an old yt-dlp is
//! what breaks first, so this is the one bundled tool that can be refreshed
//! between app releases.
//!
//! The new copy is downloaded from yt-dlp's own GitHub release, checked against
//! the release's SHA2-256SUMS file, and run once (`--version`) before it is used.
//! It lives in the app's data folder; the copy bundled with the app is never
//! touched, so going back is just forgetting the new one. Only github.com hosts
//! are contacted.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use crate::tools::{self, Tool};

const RELEASES: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
/// The release's files all start with this; nothing else is downloaded.
const DOWNLOAD_PREFIX: &str = "https://github.com/yt-dlp/yt-dlp/releases/download/";
const SUMS: &str = "SHA2-256SUMS";
/// The real file is about 24 MB.
const MAX_BINARY: u64 = 120 * 1024 * 1024;
const MAX_SMALL: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

#[cfg(windows)]
const ASSET: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const ASSET: &str = "yt-dlp_macos";

/// One update at a time.
#[derive(Default)]
pub struct Updating(Mutex<()>);

/// Which downloaded copy is in use. Absent: the one bundled with the app.
#[derive(Default, Serialize, Deserialize)]
struct Active {
    /// The app version this was chosen under. A new app release brings its own
    /// yt-dlp, so an older downloaded copy is dropped.
    app_version: String,
    #[serde(rename = "yt-dlp")]
    yt_dlp: Option<String>,
}

fn root(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("tools"))
}

fn exe_in(dir: &Path) -> PathBuf {
    dir.join(format!("yt-dlp{}", std::env::consts::EXE_SUFFIX))
}

fn read_active(root: &Path) -> Active {
    std::fs::read_to_string(root.join("active.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_active(root: &Path, active: &Active) -> Result<(), String> {
    let json = serde_json::to_string_pretty(active).map_err(|e| e.to_string())?;
    let tmp = root.join("active.json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Couldn't save the update: {e}"))?;
    std::fs::rename(&tmp, root.join("active.json"))
        .map_err(|e| format!("Couldn't save the update: {e}"))
}

/// The downloaded yt-dlp to use instead of the bundled one, if there is a good one.
pub fn override_path(app: &AppHandle) -> Option<PathBuf> {
    let root = root(app)?;
    let active = read_active(&root);
    if active.app_version != env!("CARGO_PKG_VERSION") {
        return None;
    }
    let exe = exe_in(&root.join("yt-dlp").join(active.yt_dlp?));
    exe.is_file().then_some(exe)
}

/// Stops using a downloaded copy and removes it, so the bundled one runs again.
pub fn reset(app: &AppHandle) -> Result<(), String> {
    let Some(root) = root(app) else {
        return Ok(());
    };
    write_active(
        &root,
        &Active {
            app_version: env!("CARGO_PKG_VERSION").into(),
            yt_dlp: None,
        },
    )?;
    let _ = std::fs::remove_dir_all(root.join("yt-dlp"));
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub tool: &'static str,
    pub current: Option<String>,
    pub latest: String,
    pub available: bool,
    /// A downloaded copy is in use (so "Use the built-in one" makes sense).
    pub downloaded: bool,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn client() -> Result<reqwest::Client, String> {
    // Redirects may only stay within GitHub's own hosts.
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        let ok = attempt.previous().len() < 6
            && attempt.url().scheme() == "https"
            && attempt.url().host_str().is_some_and(|h| {
                h == "github.com"
                    || h.ends_with(".github.com")
                    || h.ends_with(".githubusercontent.com")
            });
        if ok { attempt.follow() } else { attempt.stop() }
    });
    reqwest::Client::builder()
        .user_agent("Crudes-Edit-Toolkit")
        .timeout(TIMEOUT)
        .redirect(policy)
        .build()
        .map_err(|e| e.to_string())
}

fn offline() -> String {
    "Couldn't reach GitHub to look for an update. Check your internet connection and try again."
        .into()
}

async fn body(client: &reqwest::Client, url: &str, cap: u64) -> Result<Vec<u8>, String> {
    let mut response = client.get(url).send().await.map_err(|_| offline())?;
    if !response.status().is_success() {
        return Err(
            if response.status().as_u16() == 403 || response.status().as_u16() == 429 {
                "GitHub is limiting requests from this connection right now. Try again in a little while.".into()
            } else {
                format!(
                    "GitHub answered with an error ({}). Try again later.",
                    response.status().as_u16()
                )
            },
        );
    }
    if response.content_length().is_some_and(|n| n > cap) {
        return Err(
            "The update file is far bigger than expected, so it was not downloaded.".into(),
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| offline())? {
        if bytes.len() as u64 + chunk.len() as u64 > cap {
            return Err(
                "The update file is far bigger than expected, so it was not downloaded.".into(),
            );
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// The hash listed for `file` in a SHA2-256SUMS file ("<hash>  <name>" per line).
fn hash_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == file && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Is `a` older than `b`? Versions look like "2026.10.21" or "2026.10.21.1".
fn is_older(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (mut x, mut y) = (parts(a), parts(b));
    let len = x.len().max(y.len());
    x.resize(len, 0);
    y.resize(len, 0);
    x < y
}

async fn latest_release(client: &reqwest::Client) -> Result<Release, String> {
    let json = body(client, RELEASES, MAX_SMALL).await?;
    serde_json::from_slice(&json).map_err(|_| "GitHub's answer wasn't what was expected.".into())
}

/// Is there a newer yt-dlp than the one in use? Installs nothing.
pub async fn check(app: &AppHandle) -> Result<UpdateStatus, String> {
    let client = client()?;
    let release = latest_release(&client).await?;
    let current = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || tools::inspect(&app, Tool::YtDlp).version)
            .await
            .map_err(|e| e.to_string())?
    };
    let latest = release.tag_name;
    Ok(UpdateStatus {
        tool: "yt-dlp",
        available: current.as_deref().is_none_or(|c| is_older(c, &latest)),
        current,
        latest,
        downloaded: override_path(app).is_some(),
    })
}

/// Downloads the newest yt-dlp, verifies it and switches to it. Returns its version,
/// or None when the one in use is already the newest.
pub async fn update(app: &AppHandle, lock: &Updating) -> Result<Option<String>, String> {
    let _one_at_a_time = lock.0.lock().await;
    let client = client()?;
    let release = latest_release(&client).await?;
    let version = release.tag_name.clone();
    // It names a folder, so it must be plain numbers between single dots: never "..".
    if !version
        .split('.')
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("GitHub reported a version number the toolkit didn't recognise.".into());
    }
    let current = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || tools::inspect(&app, Tool::YtDlp).version)
            .await
            .map_err(|e| e.to_string())?
    };
    if current.as_deref().is_some_and(|c| !is_older(c, &version)) {
        return Ok(None);
    }

    let find = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name && a.browser_download_url.starts_with(DOWNLOAD_PREFIX))
            .map(|a| a.browser_download_url.clone())
    };
    let binary_url = find(ASSET).ok_or("This release has no download for this computer.")?;
    let sums_url =
        find(SUMS).ok_or("This release has no checksum file, so it wasn't downloaded.")?;

    let sums = String::from_utf8_lossy(&body(&client, &sums_url, MAX_SMALL).await?).into_owned();
    let expected =
        hash_for(&sums, ASSET).ok_or("The release's checksum file doesn't list the download.")?;
    let bytes = body(&client, &binary_url, MAX_BINARY).await?;
    if hex(&Sha256::digest(&bytes)) != expected {
        return Err("The downloaded file didn't match its checksum, so it was thrown away.".into());
    }

    let root = root(app).ok_or("Couldn't find the app's data folder.")?;
    let dir = root.join("yt-dlp").join(&version);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't save the update: {e}"))?;
    let exe = exe_in(&dir);
    std::fs::write(&exe, &bytes).map_err(|e| format!("Couldn't save the update: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }

    // It has to run, and say it's the version that was asked for.
    let tested = {
        let exe = exe.clone();
        tauri::async_runtime::spawn_blocking(move || self_test(&exe))
            .await
            .map_err(|e| e.to_string())?
    };
    if tested.as_deref() != Some(version.as_str()) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(
            "The new yt-dlp didn't start properly, so it wasn't used. The old one still works."
                .into(),
        );
    }

    let previous = read_active(&root);
    write_active(
        &root,
        &Active {
            app_version: env!("CARGO_PKG_VERSION").into(),
            yt_dlp: Some(version.clone()),
        },
    )?;
    // Keep only what's in use: the one before it was tested but no longer needed.
    if let Some(old) = previous.yt_dlp.filter(|o| *o != version) {
        let _ = std::fs::remove_dir_all(root.join("yt-dlp").join(old));
    }
    Ok(Some(version))
}

fn self_test(exe: &Path) -> Option<String> {
    let out = tools::run(
        tools::command(exe).arg("--version"),
        Duration::from_secs(30),
    )
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_checksum_line_for_a_file() {
        let sums = format!(
            "{}  yt-dlp\n{}  yt-dlp.exe\n{}  yt-dlp.exe.old\n",
            "a".repeat(64),
            "B".repeat(64),
            "c".repeat(64)
        );
        assert_eq!(hash_for(&sums, "yt-dlp.exe"), Some("b".repeat(64)));
        assert_eq!(hash_for(&sums, "missing"), None);
        assert_eq!(hash_for("short  yt-dlp.exe", "yt-dlp.exe"), None);
    }

    #[test]
    fn only_plain_versions_may_name_a_folder() {
        let ok = |v: &str| {
            v.split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        };
        assert!(ok("2026.10.21") && ok("2026.10.21.1"));
        for bad in ["", ".", "..", "1..2", ".1", "1.", "../x", "1.2.x"] {
            assert!(!ok(bad), "{bad}");
        }
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(is_older("2026.10.21", "2026.10.26"));
        assert!(is_older("2026.9.30", "2026.10.1"));
        assert!(is_older("2026.10.21", "2026.10.21.1"));
        assert!(!is_older("2026.10.26", "2026.10.26"));
        assert!(!is_older("2026.11.1", "2026.10.26"));
    }
}
