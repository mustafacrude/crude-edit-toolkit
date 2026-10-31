//! Files handed to the app from outside: Explorer's "Send to Crude's Toolkit"
//! (the file is passed as an argument), or Finder's "Open With" on a Mac.
//!
//! Both ways go through one waiting list: launches add to it, and the screen
//! takes everything on it, once, when it starts and whenever it's nudged. Explorer
//! starts one process per selected file, all at once, some before the screen has
//! loaded, so nothing may depend on the screen already listening.

use std::path::Path;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct Opened(Mutex<Vec<String>>);

/// The existing files and folders among a program's arguments. The first one is
/// the program itself, and anything that isn't a path (a flag) is ignored.
pub fn paths_from_args(args: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for arg in args.iter().skip(1) {
        if arg.starts_with('-') || !Path::new(arg).exists() || found.contains(arg) {
            continue;
        }
        found.push(arg.clone());
    }
    found
}

/// Remembers files from this launch's own arguments.
pub fn remember(app: &AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    app.state::<Opened>().0.lock().unwrap().extend(paths);
}

/// Adds files to the waiting list, nudges the screen to collect them, and brings
/// the window forward.
pub fn deliver(app: &AppHandle, paths: Vec<String>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if !paths.is_empty() {
        remember(app, paths);
        let _ = app.emit("files-opened", ());
    }
}

/// Files waiting from this launch, once.
#[tauri::command]
pub fn take_opened_files(opened: State<Opened>) -> Vec<String> {
    std::mem::take(&mut *opened.0.lock().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_paths_count() {
        let dir = std::env::temp_dir();
        let here = dir.display().to_string();
        let args = vec![
            "app.exe".to_string(),
            "--flag".to_string(),
            here.clone(),
            here.clone(),
            "C:/no/such/file.mkv".to_string(),
        ];
        assert_eq!(paths_from_args(&args), vec![here]);
        assert!(paths_from_args(&["app.exe".to_string()]).is_empty());
    }
}
