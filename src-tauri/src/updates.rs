//! Looking for a newer app and a newer yt-dlp. Nothing is installed by itself:
//! the check at launch only tells the screen what it found, and installing
//! happens when someone presses the button.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::settings::SettingsStore;
use crate::tool_update;
use crate::untrunc_update;

/// A launch check at most this often, so GitHub's limit on anonymous requests is never an issue.
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Let the app finish opening before it goes online.
const SETTLE: Duration = Duration::from_secs(8);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// A newer version of the app.
    pub app: Option<AppUpdate>,
    /// A newer yt-dlp.
    pub yt_dlp: Option<tool_update::UpdateStatus>,
    /// A newer untrunc, when it has been downloaded (nobody is nagged about a tool they never got).
    pub untrunc: Option<tool_update::UpdateStatus>,
}

#[derive(Clone, Serialize)]
pub struct AppUpdate {
    pub version: String,
    pub notes: Option<String>,
}

/// Is there a newer app? An unreachable or empty release page counts as "no".
pub async fn check_app(app: &AppHandle) -> Result<Option<AppUpdate>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(found) => Ok(found.map(|u| AppUpdate {
            version: u.version,
            notes: u.body.filter(|n| !n.trim().is_empty()),
        })),
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => Ok(None),
        Err(_) => Err(
            "Couldn't check for a new version of the app. Check your internet connection and try again.".into(),
        ),
    }
}

/// Downloads the newest app, installs it and restarts into it.
pub async fn install_app(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|_| {
            "Couldn't reach the update server. Check your internet connection.".to_string()
        })?
        .ok_or("The app is already the newest version.")?;
    let progress = app.clone();
    let mut got: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                got += chunk as u64;
                let _ = progress.emit("app-update-progress", (got, total));
            },
            || {},
        )
        .await
        .map_err(|_| {
            "The update couldn't be installed. The current version still works.".to_string()
        })?;
    // Take any running jobs down cleanly, then start the new version.
    app.state::<std::sync::Arc<crate::jobs::Queue>>().shutdown();
    app.restart();
}

/// Looks for both, ignoring each one that can't be reached.
pub async fn check_all(app: &AppHandle) -> Result<Found, String> {
    let untrunc = async {
        if untrunc_update::is_installed(app) {
            untrunc_update::check(app).await.ok().filter(|t| t.available)
        } else {
            None
        }
    };
    let (app_update, tool, untrunc) =
        tokio::join!(check_app(app), tool_update::check(app), untrunc);
    if app_update.is_err() && tool.is_err() {
        return Err(app_update.err().unwrap_or_default());
    }
    Ok(Found {
        app: app_update.ok().flatten(),
        yt_dlp: tool.ok().filter(|t| t.available),
        untrunc,
    })
}

fn stamp_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("update-check.txt"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The check at launch: once a day, only if the setting is on, and silent when it finds
/// nothing or can't connect. What it finds is announced to the screen.
pub async fn launch_check(app: AppHandle) {
    if !app.state::<SettingsStore>().get().check_for_updates {
        return;
    }
    let Some(stamp) = stamp_path(&app) else {
        return;
    };
    let last: u64 = std::fs::read_to_string(&stamp)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if now_secs().saturating_sub(last) < EVERY.as_secs() {
        return;
    }
    tokio::time::sleep(SETTLE).await;
    let Ok(found) = check_all(&app).await else {
        return;
    };
    let _ = std::fs::write(&stamp, now_secs().to_string());
    if found.app.is_some() || found.yt_dlp.is_some() || found.untrunc.is_some() {
        let _ = app.emit("updates-found", found);
    }
}
