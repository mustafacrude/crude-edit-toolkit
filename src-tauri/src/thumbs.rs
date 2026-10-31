//! Small previews for cards: a frame from a video, a shrunk image, or a link's
//! own thumbnail. Each comes back as a JPEG data URL, so the UI needs no access
//! to the file system or the web, and the page's CSP stays as it is.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use reqwest::Url;
use tauri::AppHandle;

use crate::inspect::IMAGE_EXTENSIONS;
use crate::tools::{Tool, command, run};

/// Wide enough for a 208 px card at 2× pixel density.
const WIDTH: u32 = 416;
const TIMEOUT: Duration = Duration::from_secs(20);
/// A link's thumbnail is a few hundred KB; anything far bigger isn't one.
const MAX_REMOTE: u64 = 15 * 1024 * 1024;
/// Previews kept in memory (about 30 KB each).
const KEEP: usize = 400;

#[derive(Default)]
pub struct Thumbs(Mutex<HashMap<String, String>>);

impl Thumbs {
    fn get(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(key).cloned()
    }

    fn put(&self, key: String, value: String) {
        let mut map = self.0.lock().unwrap();
        if map.len() >= KEEP {
            map.clear();
        }
        map.insert(key, value);
    }
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// A preview format: JPEG for photos and video frames, PNG for pictures that
/// may be see-through (JPEG would flatten their transparency).
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Jpeg,
    Png,
}

/// ffmpeg writes one small preview to stdout. `at` seeks into a video first.
fn render(ffmpeg: &Path, input: &Path, at: Option<f64>, kind: Kind) -> Option<Vec<u8>> {
    let mut cmd = command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-v", "error"]);
    if let Some(secs) = at {
        cmd.args(["-ss", &format!("{secs:.2}")]);
    }
    cmd.args(["-protocol_whitelist", "file"]);
    cmd.arg("-i").arg(input);
    cmd.args([
        "-frames:v",
        "1",
        "-vf",
        &format!("scale='min({WIDTH},iw)':-2"),
        "-f",
        "image2pipe",
    ]);
    match kind {
        Kind::Jpeg => cmd.args(["-c:v", "mjpeg", "-q:v", "5", "-"]),
        Kind::Png => cmd.args(["-c:v", "png", "-pix_fmt", "rgba", "-"]),
    };
    let out = run(&mut cmd, TIMEOUT).ok()?;
    (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
}

fn data_url(bytes: &[u8], kind: Kind) -> String {
    format!(
        "data:image/{};base64,{}",
        if kind == Kind::Png { "png" } else { "jpeg" },
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// JPEGs can't be see-through, so their previews stay JPEG; anything else gets PNG.
fn kind_for_image(head: &[u8]) -> Kind {
    if head.starts_with(&[0xFF, 0xD8]) {
        Kind::Jpeg
    } else {
        Kind::Png
    }
}

/// A preview of a file on disk: a frame a tenth of the way into a video (past
/// a black intro), or the image itself.
pub fn of_file(app: &AppHandle, cache: &Thumbs, path: &str) -> Option<String> {
    let file = Path::new(path);
    let modified = std::fs::metadata(file).and_then(|m| m.modified()).ok()?;
    let key = format!("{path}|{modified:?}");
    if let Some(hit) = cache.get(&key) {
        return Some(hit);
    }
    let ffmpeg = Tool::Ffmpeg.path(app).ok()?;
    let (bytes, kind) = if is_image(file) {
        let mut head = [0u8; 2];
        let _ = std::fs::File::open(file).and_then(|mut f| std::io::Read::read(&mut f, &mut head));
        let kind = kind_for_image(&head);
        (render(&ffmpeg, file, None, kind)?, kind)
    } else {
        let ffprobe = Tool::Ffprobe.path(app).ok()?;
        let duration = crate::media::probe(&ffprobe, file)
            .ok()
            .and_then(|i| i.duration)
            .unwrap_or(0.0);
        let at = (duration * 0.1).min(60.0);
        let frame = render(&ffmpeg, file, (at > 0.5).then_some(at), Kind::Jpeg)
            .or_else(|| render(&ffmpeg, file, None, Kind::Jpeg))?;
        (frame, Kind::Jpeg)
    };
    let url = data_url(&bytes, kind);
    cache.put(key, url.clone());
    Some(url)
}

/// A link's thumbnail (the address yt-dlp reported), fetched and shrunk.
pub async fn of_url(
    app: &AppHandle,
    cache: &Thumbs,
    temp_dir: &Path,
    link: &str,
) -> Option<String> {
    let url = Url::parse(link)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))?;
    if let Some(hit) = cache.get(link) {
        return Some(hit);
    }
    let bytes = crate::fetch_image::download(&url, MAX_REMOTE).await.ok()?;
    let kind = kind_for_image(&bytes);
    let temp = temp_dir.join(format!("thumb-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temp, bytes).ok()?;
    let ffmpeg = Tool::Ffmpeg.path(app).ok()?;
    let rendered = {
        let temp = temp.clone();
        tauri::async_runtime::spawn_blocking(move || render(&ffmpeg, &temp, None, kind))
            .await
            .ok()
            .flatten()
    };
    let _ = std::fs::remove_file(&temp);
    let url = data_url(&rendered?, kind);
    cache.put(link.to_string(), url.clone());
    Some(url)
}
