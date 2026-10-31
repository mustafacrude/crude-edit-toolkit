mod c2pa;
mod db;
mod encode;
mod encoders;
mod errors;
mod exif;
mod fetch_image;
mod guard;
mod inspect;
mod jobs;
mod media;
mod modules;
mod opened;
mod progress;
mod settings;
mod thumbs;
mod timecode;
mod tool_update;
mod tools;
mod updates;

use std::sync::Arc;

use tokio::sync::Semaphore;

use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use jobs::{Job, Queue};
use settings::{Settings, SettingsStore};

/// The licence texts that come with the app, for Settings > About.
#[tauri::command]
async fn licenses_text(app: AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("licenses")
        .join("NOTICES.txt");
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::read_to_string(path)
            .map_err(|_| "The licence list isn't installed with this copy of the app.".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Version check for every bundled tool, run in parallel off the UI thread.
#[tauri::command]
async fn tool_versions(app: AppHandle) -> Vec<tools::ToolInfo> {
    let handles: Vec<_> = tools::Tool::ALL
        .into_iter()
        .map(|tool| {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || tools::inspect(&app, tool))
        })
        .collect();

    let mut infos = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(info) = handle.await {
            infos.push(info);
        }
    }
    infos
}

#[tauri::command]
fn get_settings(store: State<SettingsStore>) -> Settings {
    store.get()
}

#[tauri::command]
fn set_settings(store: State<SettingsStore>, settings: Settings) -> Result<Settings, String> {
    let saved = store.set(settings)?;
    guard::set_cpu_limit(saved.cpu_limit);
    Ok(saved)
}

/// How many link lookups and file inspections may run at once. Pasting fifty
/// links or dropping a big folder queues them here instead of starting fifty
/// yt-dlp or ffprobe processes together.
struct Lookups {
    links: Semaphore,
    files: Semaphore,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleInfo {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: modules::Category,
    accepts: modules::Accepts,
    combines_inputs: bool,
    schema: Vec<modules::Field>,
}

/// Every tool and its settings form, for the tool screens.
#[tauri::command]
fn list_modules() -> Vec<ModuleInfo> {
    modules::all()
        .iter()
        .map(|m| ModuleInfo {
            id: m.id(),
            name: m.name(),
            description: m.description(),
            category: m.category(),
            accepts: m.accepts(),
            combines_inputs: m.combines_inputs(),
            schema: m.options_schema(),
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EncoderView {
    /// What "auto" picks for H.264 right now, e.g. "h264_nvenc".
    h264: String,
    /// A friendly name for it, e.g. "NVIDIA (NVENC)".
    label: String,
    /// Hardware families that passed the test, e.g. ["nvenc", "amf"].
    hardware: Vec<String>,
}

fn encoder_view(app: &AppHandle, status: encoders::EncoderStatus) -> EncoderView {
    let preference = app.state::<SettingsStore>().get().encoder;
    let h264 = app
        .state::<encoders::Encoders>()
        .pick(app, encoders::Codec::H264, &preference);
    EncoderView {
        label: encoders::label(&h264).into(),
        h264,
        hardware: status
            .h264
            .iter()
            .filter_map(|e| e.rsplit('_').next().map(String::from))
            .collect(),
    }
}

/// Which video encoder this machine uses. Tests the GPU the first time.
#[tauri::command]
async fn encoder_status(app: AppHandle) -> EncoderView {
    tauri::async_runtime::spawn_blocking(move || {
        let status = app.state::<encoders::Encoders>().status(&app);
        encoder_view(&app, status)
    })
    .await
    .expect("encoder test thread")
}

/// Tests the hardware encoders again, e.g. after a driver update.
#[tauri::command]
async fn retest_encoders(app: AppHandle) -> EncoderView {
    tauri::async_runtime::spawn_blocking(move || {
        let status = app.state::<encoders::Encoders>().retest(&app);
        encoder_view(&app, status)
    })
    .await
    .expect("encoder test thread")
}

#[tauri::command]
fn list_jobs(queue: State<Arc<Queue>>) -> Vec<Job> {
    queue.list()
}

#[tauri::command]
async fn start_jobs(
    queue: State<'_, Arc<Queue>>,
    module: String,
    inputs: Vec<String>,
    options: Option<serde_json::Value>,
) -> Result<Vec<String>, String> {
    let queue = Arc::clone(&queue);
    queue
        .submit(&module, inputs, options.unwrap_or(serde_json::Value::Null))
        .await
}

#[tauri::command]
fn cancel_job(queue: State<Arc<Queue>>, id: String) {
    queue.cancel(&id);
}

#[tauri::command]
async fn retry_job(queue: State<'_, Arc<Queue>>, id: String) -> Result<Vec<String>, String> {
    let queue = Arc::clone(&queue);
    queue.retry(&id).await
}

#[tauri::command]
fn clear_finished(queue: State<Arc<Queue>>) -> Result<(), String> {
    queue.clear_finished()
}

#[tauri::command]
fn job_log(queue: State<Arc<Queue>>, id: String) -> String {
    queue.log(&id)
}

/// Files that are safe to hand to the default app. A site chooses the extension of
/// what it serves, so anything else (a program, a script, a shortcut) is shown in
/// its folder and never run from here.
fn safe_to_open(path: &std::path::Path) -> bool {
    use inspect::{AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, VIDEO_EXTENSIONS};
    const TEXT: &[&str] = &["json", "srt", "vtt", "ass", "lrc", "txt", "description"];
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // SVG pictures can carry script; they're converted to PNG, never opened as SVG.
    ext != "svg"
        && [VIDEO_EXTENSIONS, IMAGE_EXTENSIONS, AUDIO_EXTENSIONS, TEXT]
            .iter()
            .any(|list| list.contains(&ext.as_str()))
}

/// Opens a job's output in its default app. Only paths a job produced can be opened,
/// and only kinds of file that are safe to open.
#[tauri::command]
fn open_job_output(app: AppHandle, queue: State<Arc<Queue>>, id: String) -> Result<(), String> {
    let path = queue.output(&id)?;
    if !safe_to_open(&path) {
        return Err("That kind of file isn't opened from here. Use Show in folder instead.".into());
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Couldn't open it: {e}"))
}

#[tauri::command]
fn reveal_job_output(app: AppHandle, queue: State<Arc<Queue>>, id: String) -> Result<(), String> {
    let path = queue.output(&id)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| format!("Couldn't show it: {e}"))
}

/// Native file picker, run from Rust so the UI needs no file-system permissions.
/// `filter` is "video" for video-only tools; anything else offers all media.
#[tauri::command]
async fn pick_files(app: AppHandle, filter: Option<String>) -> Vec<String> {
    use inspect::{AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, VIDEO_EXTENSIONS};
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file();
        let dialog = if filter.as_deref() == Some("video") {
            dialog.add_filter("Videos", VIDEO_EXTENSIONS)
        } else if filter.as_deref() == Some("image") {
            dialog.add_filter("Images", IMAGE_EXTENSIONS)
        } else {
            let all: Vec<&str> = [VIDEO_EXTENSIONS, IMAGE_EXTENSIONS, AUDIO_EXTENSIONS].concat();
            dialog
                .add_filter("Videos, images and audio", &all)
                .add_filter("Videos", VIDEO_EXTENSIONS)
                .add_filter("Images", IMAGE_EXTENSIONS)
                .add_filter("Audio", AUDIO_EXTENSIONS)
        };
        dialog.blocking_pick_files()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    picked
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
        .collect()
}

/// Probes a dropped file and explains what Premiere will struggle with.
#[tauri::command]
async fn inspect_file(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    lookups: State<'_, Lookups>,
    path: String,
) -> Result<inspect::Inspection, String> {
    let temp_dir = queue.temp_dir().to_path_buf();
    let _turn = lookups.files.acquire().await.map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || inspect::inspect(&app, &temp_dir, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// Saves the image a link points at (or a page's main picture) into the
/// downloads folder's Images folder, as PNG if Premiere couldn't open it.
#[tauri::command]
async fn save_image_link(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    store: State<'_, SettingsStore>,
    lookups: State<'_, Lookups>,
    saved: State<'_, fetch_image::SavedImages>,
    url: String,
) -> Result<fetch_image::SavedImage, String> {
    let _turn = lookups.links.acquire().await.map_err(|e| e.to_string())?;
    let download_dir = std::path::PathBuf::from(store.get().download_dir);
    let ctx = fetch_image::Context {
        app: &app,
        temp_dir: queue.temp_dir(),
        download_dir: &download_dir,
        saved: &saved,
    };
    fetch_image::from_link(ctx, &url).await
}

/// Saves an image pasted from the clipboard. The body is the image's bytes.
#[tauri::command]
async fn save_pasted_image(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    store: State<'_, SettingsStore>,
    lookups: State<'_, Lookups>,
    saved: State<'_, fetch_image::SavedImages>,
    request: tauri::ipc::Request<'_>,
) -> Result<fetch_image::SavedImage, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("There's no image on the clipboard.".into());
    };
    let bytes = bytes.clone();
    let _turn = lookups.files.acquire().await.map_err(|e| e.to_string())?;
    let download_dir = std::path::PathBuf::from(store.get().download_dir);
    let ctx = fetch_image::Context {
        app: &app,
        temp_dir: queue.temp_dir(),
        download_dir: &download_dir,
        saved: &saved,
    };
    fetch_image::from_bytes(ctx, bytes).await
}

/// `action` is "open", "reveal" or "lens" (search Google Lens for bigger copies).
#[tauri::command]
fn saved_image_action(
    app: AppHandle,
    saved: State<fetch_image::SavedImages>,
    id: String,
    action: String,
) -> Result<(), String> {
    let image = saved.get(&id)?;
    let opener = app.opener();
    match action.as_str() {
        "open" => opener
            .open_path(image.path.to_string_lossy(), None::<&str>)
            .map_err(|e| format!("Couldn't open it: {e}")),
        "reveal" => opener
            .reveal_item_in_dir(&image.path)
            .map_err(|e| format!("Couldn't show it: {e}")),
        "lens" => {
            let source = image
                .source
                .ok_or("A pasted image has no address to search with.")?;
            opener
                .open_url(fetch_image::lens_url(&source)?.as_str(), None::<&str>)
                .map_err(|e| format!("Couldn't open your browser: {e}"))
        }
        _ => Err("Unknown action.".into()),
    }
}

/// A small JPEG preview (as a data URL) of a file on disk, or of a link's
/// thumbnail address. None when there's nothing to show.
#[tauri::command]
async fn thumbnail(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    lookups: State<'_, Lookups>,
    cache: State<'_, thumbs::Thumbs>,
    path: Option<String>,
    url: Option<String>,
) -> Result<Option<String>, String> {
    let _turn = lookups.files.acquire().await.map_err(|e| e.to_string())?;
    if let Some(url) = url {
        return Ok(thumbs::of_url(&app, &cache, queue.temp_dir(), &url).await);
    }
    let Some(path) = path else { return Ok(None) };
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app2.state::<thumbs::Thumbs>();
        thumbs::of_file(&app2, &cache, &path)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Looks for a newer app and a newer yt-dlp (installs nothing).
#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<updates::Found, String> {
    updates::check_all(&app).await
}

/// Downloads the newest yt-dlp, checks it and switches to it. Returns the new version,
/// or None when the one in use is already the newest.
#[tauri::command]
async fn update_yt_dlp(
    app: AppHandle,
    lock: State<'_, tool_update::Updating>,
) -> Result<Option<String>, String> {
    tool_update::update(&app, &lock).await
}

/// Goes back to the yt-dlp that came with the app.
#[tauri::command]
fn use_bundled_yt_dlp(app: AppHandle) -> Result<(), String> {
    tool_update::reset(&app)
}

/// Downloads the newest app and restarts into it.
#[tauri::command]
async fn install_app_update(app: AppHandle) -> Result<(), String> {
    updates::install_app(&app).await
}

/// Turns dropped folders into the media files inside them.
#[tauri::command]
async fn expand_paths(paths: Vec<String>, kind: Option<String>) -> inspect::Expanded {
    tauri::async_runtime::spawn_blocking(move || inspect::expand(paths, kind.as_deref()))
        .await
        .unwrap_or_default()
}

/// A link's title and formats, for the preview before downloading.
#[tauri::command]
async fn fetch_link_info(
    app: AppHandle,
    queue: State<'_, Arc<Queue>>,
    settings: State<'_, SettingsStore>,
    lookups: State<'_, Lookups>,
    url: String,
    sign_in: bool,
) -> Result<modules::download::LinkInfo, String> {
    let temp_dir = queue.temp_dir().to_path_buf();
    let settings = settings.get();
    let _turn = lookups.links.acquire().await.map_err(|e| e.to_string())?;
    modules::download::link_info(&app, &temp_dir, &settings, &url, sign_in).await
}

/// How many videos playlist downloads remember as already downloaded.
#[tauri::command]
fn download_archive_count(app: AppHandle) -> Result<usize, String> {
    let path = modules::download::archive_path(&app)?;
    Ok(std::fs::read_to_string(path)
        .map(|text| text.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0))
}

/// Forgets them, so the next playlist download gets every video again.
#[tauri::command]
fn forget_download_archive(app: AppHandle) -> Result<(), String> {
    let path = modules::download::archive_path(&app)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Couldn't clear the list: {e}")),
    }
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
        .await
        .ok()
        .flatten()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
}

/// Picks a cookies.txt and checks it's the right kind of file. Only the path is kept.
#[tauri::command]
async fn pick_cookies_file(app: AppHandle) -> Result<Option<String>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("cookies.txt", &["txt"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked.and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    modules::download::check_cookies_file(&path)?;
    Ok(Some(path.display().to_string()))
}

/// Writes panics to `logs/crash.log`, so a crash a friend hits can be looked at.
fn log_panics(logs_dir: std::path::PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write as _;
        let path = logs_dir.join("crash.log");
        // Keep it small: start over past 1 MB.
        let append = std::fs::metadata(&path).is_ok_and(|m| m.len() < 1024 * 1024);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&path)
        {
            let when = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = writeln!(
                f,
                "[{when}] v{} {info}\n{}\n",
                env!("CARGO_PKG_VERSION"),
                std::backtrace::Backtrace::force_capture()
            );
        }
        default(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Managed before anything runs: a second launch can hand over its files
        // while this one is still starting, before setup has run.
        .manage(opened::Opened::default())
        // First, so a second launch hands over to the running app instead of
        // starting another copy that would share (and empty) its scratch folder.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Files sent from Explorer arrive as arguments of the second launch.
            opened::deliver(app, opened::paths_from_args(&args));
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let config_dir = app.path().app_config_dir()?;
            let logs_dir = data_dir.join("logs");
            std::fs::create_dir_all(&logs_dir)?;
            std::fs::create_dir_all(&config_dir)?;
            log_panics(logs_dir.clone());

            let temp_dir = data_dir.join("tmp");
            std::fs::create_dir_all(&temp_dir)?;
            let default_downloads = app
                .path()
                .video_dir()
                .or_else(|_| app.path().download_dir())?
                .join("Crude Toolkit");

            app.manage(SettingsStore::load(
                config_dir.join("settings.json"),
                default_downloads,
            ));
            guard::set_cpu_limit(app.state::<SettingsStore>().get().cpu_limit);
            app.manage(fetch_image::SavedImages::default());
            app.manage(thumbs::Thumbs::default());
            app.manage(tool_update::Updating::default());
            tauri::async_runtime::spawn(updates::launch_check(app.handle().clone()));
            opened::remember(
                app.handle(),
                opened::paths_from_args(&std::env::args().collect::<Vec<_>>()),
            );
            app.manage(Lookups {
                links: Semaphore::new(3),
                files: Semaphore::new(3),
            });
            let db = db::Db::open_or_reset(&data_dir.join("history.sqlite"))?;
            app.manage(Queue::new(app.handle().clone(), db, logs_dir, temp_dir));
            app.manage(encoders::Encoders::new(data_dir.join("encoders.json")));
            encoders::warm_up(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tool_versions,
            licenses_text,
            get_settings,
            set_settings,
            list_modules,
            list_jobs,
            start_jobs,
            cancel_job,
            retry_job,
            clear_finished,
            job_log,
            open_job_output,
            reveal_job_output,
            pick_files,
            inspect_file,
            expand_paths,
            save_image_link,
            save_pasted_image,
            saved_image_action,
            thumbnail,
            opened::take_opened_files,
            check_for_updates,
            update_yt_dlp,
            use_bundled_yt_dlp,
            install_app_update,
            fetch_link_info,
            download_archive_count,
            forget_download_archive,
            encoder_status,
            retest_encoders,
            pick_folder,
            pick_cookies_file,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| match event {
            RunEvent::ExitRequested { .. } => app.state::<Arc<Queue>>().shutdown(),
            // Finder's "Open With" (and dropping a file on the Dock icon).
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                let paths: Vec<String> = urls
                    .iter()
                    .filter_map(|u| u.to_file_path().ok())
                    .map(|p| p.display().to_string())
                    .collect();
                opened::deliver(app, paths);
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::safe_to_open;
    use std::path::Path;

    #[test]
    fn only_media_and_text_are_opened() {
        for ok in [
            "a.mp4",
            "A.MKV",
            "b.m4a",
            "c.png",
            "d.srt",
            "e.chat.json",
            "f.description",
        ] {
            assert!(safe_to_open(Path::new(ok)), "{ok}");
        }
        for bad in [
            "a.exe",
            "a.bat",
            "a.cmd",
            "a.ps1",
            "a.lnk",
            "a.url",
            "a.js",
            "a.vbs",
            "a.msi",
            "a.scr",
            "a.hta",
            "a.jar",
            "a.html",
            "a.svg",
            "noextension",
            "a.mp4.exe",
        ] {
            assert!(!safe_to_open(Path::new(bad)), "{bad}");
        }
    }
}
