//! untrunc: getting it, and keeping it up to date. It repairs cut-off MP4 and
//! MOV recordings (a camera that lost power, a crashed recording).
//!
//! Unlike the other tools it isn't shipped inside the app. It is GPL software
//! built with its own copy of FFmpeg, so the toolkit leaves it with its author:
//! the first time it's wanted, one button downloads the author's Windows build
//! from github.com/anthwlock/untrunc, and the same button updates it later.
//!
//! The build there is rebuilt on every change and always sits under the release
//! named "latest", so a version is a commit. The download is checked against the
//! SHA-256 GitHub lists for the file, unpacked into the app's data folder (only
//! untrunc.exe and the libraries it needs), and run once before it is used.
//! Only github.com hosts are contacted.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::tool_update::{self, UpdateStatus, Updating};
use crate::tools::{self, Tool};

const RELEASE: &str = "https://api.github.com/repos/anthwlock/untrunc/releases/tags/latest";
/// The release's files all start with this; nothing else is downloaded.
const DOWNLOAD_PREFIX: &str = "https://github.com/anthwlock/untrunc/releases/download/latest/";
const ASSET: &str = "untrunc_x64.zip";
/// The real file is about 13 MB.
const MAX_ZIP: u64 = 80 * 1024 * 1024;
const MAX_SMALL: u64 = 1024 * 1024;
/// The biggest file inside (a video library) is about 24 MB.
const MAX_FILE: u64 = 80 * 1024 * 1024;
const MAX_TOTAL: u64 = 200 * 1024 * 1024;

/// Which downloaded copy is in use.
#[derive(Default, Serialize, Deserialize)]
struct Active {
    untrunc: Option<String>,
}

fn base(app: &AppHandle) -> Option<PathBuf> {
    Some(tool_update::root(app)?.join("untrunc"))
}

fn exe_in(dir: &Path) -> PathBuf {
    dir.join("untrunc.exe")
}

fn read_active(root: &Path) -> Active {
    std::fs::read_to_string(root.join("untrunc-active.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_active(root: &Path, active: &Active) -> Result<(), String> {
    let json = serde_json::to_string_pretty(active).map_err(|e| e.to_string())?;
    let tmp = root.join("untrunc-active.json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Couldn't save untrunc: {e}"))?;
    std::fs::rename(&tmp, root.join("untrunc-active.json"))
        .map_err(|e| format!("Couldn't save untrunc: {e}"))
}

/// A commit id as git prints it: 7 to 40 lowercase hex digits. It names a folder,
/// so nothing else is accepted.
fn is_commit(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The commit in untrunc's version line:
/// `version 'v1-87a5d92-dirty' using ffmpeg '3.3.4' Lavc57.89.100` gives `87a5d92`.
pub fn commit_of(line: &str) -> Option<String> {
    let quoted = line.split('\'').nth(1)?;
    quoted
        .split('-')
        .map(|part| part.strip_prefix('g').filter(|p| is_commit(p)).unwrap_or(part))
        .find(|part| is_commit(part))
        .map(String::from)
}

/// The downloaded untrunc.exe, if there is a good one.
pub fn installed_path(app: &AppHandle) -> Option<PathBuf> {
    let root = tool_update::root(app)?;
    let commit = read_active(&root).untrunc.filter(|c| is_commit(c))?;
    let exe = exe_in(&base(app)?.join(commit));
    exe.is_file().then_some(exe)
}

/// Where untrunc would be, for the "missing" message when it hasn't been downloaded.
pub fn expected_path(app: &AppHandle) -> PathBuf {
    exe_in(&base(app).unwrap_or_default())
}

pub fn is_installed(app: &AppHandle) -> bool {
    installed_path(app).is_some()
}

#[derive(Deserialize)]
struct Release {
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// "sha256:<hex>", as GitHub computes it for every uploaded file.
    digest: Option<String>,
}

/// What the "latest" release is: the commit it was built from and its download.
struct Latest {
    commit: String,
    date: Option<String>,
    url: String,
    sha256: String,
}

/// The value after a bold label in the release notes ("**Head:** 87a5d9…").
fn note_value<'a>(notes: &'a str, label: &str) -> Option<&'a str> {
    let start = notes.find(label)? + label.len();
    notes[start..].split_whitespace().next()
}

fn parse_latest(release: Release) -> Result<Latest, String> {
    let odd = || "GitHub's answer wasn't what was expected.".to_string();
    let notes = release.body.unwrap_or_default();
    let commit = note_value(&notes, "**Head:**")
        .map(str::to_ascii_lowercase)
        .filter(|c| is_commit(c))
        .ok_or_else(odd)?;
    // "2026-11-01T12:25:20+01:00": the day is enough.
    let date = note_value(&notes, "**Date:**")
        .and_then(|d| d.get(..10))
        .filter(|d| d.bytes().enumerate().all(|(i, b)| matches!(i, 4 | 7) == (b == b'-') && (b == b'-' || b.is_ascii_digit())))
        .map(String::from);
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == ASSET && a.browser_download_url.starts_with(DOWNLOAD_PREFIX))
        .ok_or("The newest untrunc has no Windows download.")?;
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
        .ok_or("GitHub didn't list a checksum for the download, so it wasn't used.")?;
    Ok(Latest {
        commit,
        date,
        url: asset.browser_download_url,
        sha256,
    })
}

async fn latest(client: &reqwest::Client) -> Result<Latest, String> {
    let json = tool_update::body(client, RELEASE, MAX_SMALL).await?;
    let release: Release = serde_json::from_slice(&json)
        .map_err(|_| "GitHub's answer wasn't what was expected.".to_string())?;
    parse_latest(release)
}

fn not_here() -> String {
    "untrunc is only available on Windows for now.".into()
}

async fn current_version(app: &AppHandle) -> Result<Option<String>, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || tools::inspect(&app, Tool::Untrunc).version)
        .await
        .map_err(|e| e.to_string())
}

/// Is there a newer untrunc than the one in use (or none yet)? Installs nothing.
pub async fn check(app: &AppHandle) -> Result<UpdateStatus, String> {
    if !cfg!(windows) {
        return Err(not_here());
    }
    let client = tool_update::client()?;
    let newest = latest(&client).await?;
    let current = if is_installed(app) {
        current_version(app).await?
    } else {
        None
    };
    Ok(UpdateStatus {
        tool: "untrunc",
        available: current
            .as_deref()
            .is_none_or(|c| !newest.commit.starts_with(c)),
        current,
        latest: newest.commit[..7].to_string(),
        downloaded: true,
        released: newest.date,
    })
}

/// Downloads the newest untrunc, verifies it and switches to it. Used for the
/// first download as well as for updates. Returns the new version, or None when
/// the one in use is already the newest.
pub async fn update(app: &AppHandle, lock: &Updating) -> Result<Option<String>, String> {
    if !cfg!(windows) {
        return Err(not_here());
    }
    let _one_at_a_time = lock.acquire().await;
    let client = tool_update::client()?;
    let newest = latest(&client).await?;
    let version = newest.commit[..7].to_string();

    if is_installed(app) {
        let current = current_version(app).await?;
        if current
            .as_deref()
            .is_some_and(|c| newest.commit.starts_with(c))
        {
            return Ok(None);
        }
    }

    let bytes = tool_update::body(&client, &newest.url, MAX_ZIP).await?;
    if tool_update::hex(&Sha256::digest(&bytes)) != newest.sha256 {
        return Err("The downloaded file didn't match its checksum, so it was thrown away.".into());
    }

    let base = base(app).ok_or("Couldn't find the app's data folder.")?;
    let dir = base.join(&version);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't save untrunc: {e}"))?;
    let unpacked = {
        let dir = dir.clone();
        tauri::async_runtime::spawn_blocking(move || unpack(&bytes, &dir))
            .await
            .map_err(|e| e.to_string())?
    };
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }

    // It has to run, and say it was built from the commit that was asked for.
    let exe = exe_in(&dir);
    let tested = {
        let exe = exe.clone();
        tauri::async_runtime::spawn_blocking(move || self_test(&exe))
            .await
            .map_err(|e| e.to_string())?
    };
    if !tested.is_some_and(|c| newest.commit.starts_with(&c)) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(
            "The new untrunc didn't start properly, so it wasn't used. Nothing was changed.".into(),
        );
    }

    let root = tool_update::root(app).ok_or("Couldn't find the app's data folder.")?;
    let previous = read_active(&root).untrunc;
    write_active(
        &root,
        &Active {
            untrunc: Some(version.clone()),
        },
    )?;
    // Keep only what's in use.
    if let Some(old) = previous.filter(|o| *o != version && is_commit(o)) {
        let _ = std::fs::remove_dir_all(base.join(old));
    }
    Ok(Some(version))
}

fn self_test(exe: &Path) -> Option<String> {
    let out = tools::run(tools::command(exe).arg("-V"), Duration::from_secs(30)).ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    out.status
        .success()
        .then(|| text.lines().find_map(commit_of))
        .flatten()
}

/// Whether a file inside the download is one untrunc needs: its program and the
/// libraries beside it. The window version, and anything unexpected, is left out.
fn wanted(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let plain = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'));
    plain && (lower == "untrunc.exe" || lower.ends_with(".dll"))
}

/// Unpacks the wanted files, flat, into `dir`. Folder names inside the zip are
/// ignored, so nothing can land outside it.
fn unpack(bytes: &[u8], dir: &Path) -> Result<(), String> {
    let broken = || "The download wasn't a valid untrunc package.".to_string();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| broken())?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| broken())?;
        if !entry.is_file() {
            continue;
        }
        // Names that try to climb out of the folder are skipped outright.
        let climbs = entry
            .name()
            .map_or(true, |n| n.contains('\\') || n.split('/').any(|part| part == ".."));
        if climbs {
            continue;
        }
        let Some(name) = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            continue;
        };
        if !wanted(&name) {
            continue;
        }
        let mut data = Vec::new();
        (&mut entry)
            .take(MAX_FILE + 1)
            .read_to_end(&mut data)
            .map_err(|_| broken())?;
        total += data.len() as u64;
        if data.len() as u64 > MAX_FILE || total > MAX_TOTAL {
            return Err("The untrunc download is far bigger than expected, so it was not used.".into());
        }
        std::fs::write(dir.join(&name), data).map_err(|e| format!("Couldn't save untrunc: {e}"))?;
    }
    if !exe_in(dir).is_file() {
        return Err(broken());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_the_commit_from_the_version_line() {
        let line = "version 'v1-87a5d92-dirty' using ffmpeg '3.3.4' Lavc57.89.100";
        assert_eq!(commit_of(line).as_deref(), Some("87a5d92"));
        assert_eq!(
            commit_of("version 'v1.2-14-g87a5d92' using ffmpeg '3.3.4'").as_deref(),
            Some("87a5d92")
        );
        assert_eq!(commit_of("version 'unknown' using ffmpeg '3.3.4'"), None);
        assert_eq!(commit_of("no quotes here"), None);
    }

    #[test]
    fn only_plain_commit_ids_may_name_a_folder() {
        assert!(is_commit("87a5d92") && is_commit(&"a".repeat(40)));
        for bad in ["", "87a5d9", "..", "../87a5d92", "87A5D92", "87a5d9z", &"a".repeat(41)] {
            assert!(!is_commit(bad), "{bad}");
        }
    }

    fn release(body: &str, digest: Option<&str>, url: &str) -> Release {
        Release {
            body: Some(body.into()),
            assets: vec![Asset {
                name: ASSET.into(),
                browser_download_url: url.into(),
                digest: digest.map(String::from),
            }],
        }
    }

    const NOTES: &str = "Automatic windows build by GitHub Actions.\n\n**Head:** 87a5d9202ae48495b9aa6b5880649dbdf32c82d0\n**Date:** 2026-11-01T12:25:20+01:00";

    #[test]
    fn reads_the_latest_release() {
        let digest = format!("sha256:{}", "A".repeat(64));
        let url = format!("{DOWNLOAD_PREFIX}{ASSET}");
        let latest = parse_latest(release(NOTES, Some(&digest), &url)).ok().unwrap();
        assert_eq!(latest.commit, "87a5d9202ae48495b9aa6b5880649dbdf32c82d0");
        assert_eq!(latest.date.as_deref(), Some("2026-11-01"));
        assert_eq!(latest.sha256, "a".repeat(64));
    }

    #[test]
    fn refuses_a_release_it_cannot_check() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let url = format!("{DOWNLOAD_PREFIX}{ASSET}");
        // No checksum listed.
        assert!(parse_latest(release(NOTES, None, &url)).is_err());
        // A download from somewhere other than the release.
        let elsewhere = format!("https://example.com/{ASSET}");
        assert!(parse_latest(release(NOTES, Some(&digest), &elsewhere)).is_err());
        // Notes that don't name a commit.
        assert!(parse_latest(release("**Head:** ../../x", Some(&digest), &url)).is_err());
        assert!(parse_latest(release("nothing here", Some(&digest), &url)).is_err());
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut out);
        for (name, data) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn unpacks_only_the_program_and_its_libraries() {
        let dir = std::env::temp_dir().join(format!("untrunc-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = zip_of(&[
            ("untrunc_x64/untrunc.exe", b"exe"),
            ("untrunc_x64/untrunc-gui.exe", b"gui"),
            ("untrunc_x64/AVCODEC-57.DLL", b"dll"),
            ("untrunc_x64/readme.txt", b"txt"),
            ("../escape.dll", b"nope"),
            ("untrunc_x64/..\\evil.dll", b"nope"),
        ]);
        unpack(&bytes, &dir).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["AVCODEC-57.DLL", "untrunc.exe"]);
        assert!(!dir.parent().unwrap().join("escape.dll").exists());

        // A package without the program is no package.
        let other = std::env::temp_dir().join(format!("untrunc-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&other).unwrap();
        assert!(unpack(&zip_of(&[("a/AVUTIL-55.DLL", b"dll")]), &other).is_err());
        assert!(unpack(b"not a zip", &other).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }
}
