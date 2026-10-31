//! Saving an image from a link or the clipboard, ready for Premiere.
//!
//! A link can be the image itself, a Google or Bing Images result (the
//! original's address is inside it), or a web page, whose preview image
//! (`og:image`) is taken. Formats Premiere can't open (WebP, AVIF, HEIC, SVG,
//! JPEG XL…) are saved as PNG, which keeps any transparency.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use reqwest::Url;
use serde::Serialize;
use tauri::AppHandle;

use crate::media;
use crate::tools::{Tool, command, run};

const MAX_IMAGE: u64 = 100 * 1024 * 1024;
const MAX_PAGE: u64 = 5 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(45);
const MAX_REDIRECTS: usize = 8;
const CONVERT_TIMEOUT: Duration = Duration::from_secs(120);
/// A longest side under this looks soft when it fills a 1080p frame.
const SMALL: u32 = 1000;
/// Some sites refuse requests that don't look like a browser.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedImage {
    /// Refers to this image in later calls (open, show, find a bigger one).
    pub id: String,
    pub path: PathBuf,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// What it arrived as ("WebP"), when it was converted to PNG.
    pub converted_from: Option<String>,
    /// Worth knowing: it's small, or it's only a preview.
    pub notes: Vec<String>,
    /// The image's own address, for a reverse image search. None for pastes.
    pub source: Option<String>,
}

/// Images saved this session, so the UI can open them by id and never by path.
#[derive(Default)]
pub struct SavedImages(Mutex<HashMap<String, SavedImage>>);

impl SavedImages {
    pub fn get(&self, id: &str) -> Result<SavedImage, String> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "That image is no longer listed.".to_string())
    }

    fn add(&self, image: &SavedImage) {
        self.0
            .lock()
            .unwrap()
            .insert(image.id.clone(), image.clone());
    }
}

/// Google and Bing image results carry the original's address in the link.
fn original_url(link: &Url) -> Result<Url, String> {
    let host = link.host_str().unwrap_or_default();
    for (key, value) in link.query_pairs() {
        if matches!(key.as_ref(), "imgurl" | "mediaurl")
            && let Ok(url) = Url::parse(&value)
            && matches!(url.scheme(), "http" | "https")
        {
            return Ok(url);
        }
    }
    let google = host.starts_with("google.") || host.contains(".google.");
    if google && link.path() == "/search" {
        return Err("That's a Google search page, not an image. Click the picture you want, then right-click the large preview and choose Copy image address.".into());
    }
    Ok(link.clone())
}

/// Google's small search previews.
fn is_thumbnail(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    (host.starts_with("encrypted-tbn") && host.ends_with(".gstatic.com"))
        || url.query().is_some_and(|q| q.contains("tbn:"))
}

/// The value of `name="…"` in one HTML tag, if it's there.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name) {
        let start = from + at;
        from = start + name.len();
        // Whole attribute names only ("content", not "data-content").
        let before = lower[..start].chars().last();
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = lower[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let offset = tag.len() - rest.len();
        let quote = rest.chars().next()?;
        let value = if quote == '"' || quote == '\'' {
            let body = &tag[offset + 1..];
            &body[..body.find(quote)?]
        } else {
            let body = &tag[offset..];
            &body[..body
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(body.len())]
        };
        return Some(value.replace("&amp;", "&").replace("&#38;", "&"));
    }
    None
}

/// A web page's preview image: Open Graph first, then Twitter's, then image_src.
fn page_image(html: &str, base: &Url) -> Option<Url> {
    const WANTED: [&str; 4] = [
        "og:image:secure_url",
        "og:image",
        "twitter:image",
        "twitter:image:src",
    ];
    let mut found: Vec<(usize, String)> = Vec::new();
    for tag in html.split('<').skip(1) {
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let lower = tag.get(..5).unwrap_or_default().to_ascii_lowercase();
        if lower.starts_with("meta") {
            let key = attr(tag, "property").or_else(|| attr(tag, "name"));
            if let (Some(key), Some(value)) = (key, attr(tag, "content"))
                && let Some(rank) = WANTED.iter().position(|w| key.eq_ignore_ascii_case(w))
            {
                found.push((rank, value));
            }
        } else if lower.starts_with("link")
            && attr(tag, "rel").is_some_and(|r| r.eq_ignore_ascii_case("image_src"))
            && let Some(href) = attr(tag, "href")
        {
            found.push((WANTED.len(), href));
        }
    }
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().find_map(|(_, value)| {
        base.join(value.trim())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
    })
}

struct Fetched {
    bytes: Vec<u8>,
    content_type: String,
    url: Url,
}

fn unreachable(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "The site took too long to answer. Try again, or try another copy of the image.".into()
    } else {
        "Couldn't reach the site. Check your internet connection and the link.".into()
    }
}

/// Is this an address out on the internet? A page, a redirect or a site's thumbnail
/// can point at this computer or the home network (a router's admin page, a local
/// server), and even a plain GET there can do things. Those are fetched only when
/// the person typed that very address themselves.
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || a == 0
                // Carrier-grade NAT, 100.64.0.0/10.
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                // Unique local fc00::/7 and link-local fe80::/10.
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80)
        }
    }
}

fn not_public() -> String {
    "That link leads to an address on this computer or your own network, so it wasn't fetched."
        .into()
}

/// A client for one request. For an address the person didn't type, the host is
/// looked up first and the client is pinned to the addresses that were checked, so
/// the site can't answer differently the second time (DNS rebinding). Redirects
/// are followed by hand in [`get`], so every hop is checked too.
async fn client_for(url: &Url, vet: bool, timeout: Duration) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none());
    if vet {
        let host = url.host_str().ok_or_else(not_public)?;
        let port = url.port_or_known_default().unwrap_or(443);
        let literal = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .ok();
        let addrs: Vec<SocketAddr> = match literal {
            Some(ip) => vec![SocketAddr::new(ip, port)],
            None => {
                let name = host.to_ascii_lowercase();
                if name == "localhost" || name.ends_with(".localhost") {
                    return Err(not_public());
                }
                tokio::net::lookup_host((host, port))
                    .await
                    .map_err(|_| {
                        "Couldn't find that site. Check your internet connection and the link."
                            .to_string()
                    })?
                    .collect()
            }
        };
        if addrs.is_empty() || addrs.iter().any(|a| !is_public(a.ip())) {
            return Err(not_public());
        }
        if literal.is_none() {
            builder = builder.resolve_to_addrs(host, &addrs);
        }
    }
    builder.build().map_err(|e| e.to_string())
}

/// Fetches `url`, up to `cap` bytes. `vet_first` checks the first address too; it's
/// off only for the one link the person typed. Every redirect and every address a
/// page names is always checked.
async fn get(url: &Url, cap: u64, timeout: Duration, vet_first: bool) -> Result<Fetched, String> {
    let mut current = url.clone();
    let mut response = None;
    for hop in 0..=MAX_REDIRECTS {
        let client = client_for(&current, vet_first || hop > 0, timeout).await?;
        let reply = client
            .get(current.clone())
            .send()
            .await
            .map_err(unreachable)?;
        if !reply.status().is_redirection() {
            response = Some(reply);
            break;
        }
        current = reply
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|to| current.join(to).ok())
            .filter(|next| matches!(next.scheme(), "http" | "https"))
            .ok_or("The link sends you somewhere the toolkit can't follow.")?;
    }
    let mut response = response.ok_or("The link sends you around in circles.")?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "The site refused to hand the image over. Open it in your browser, right-click it and choose Copy image, then paste it here.".into(),
            404 | 410 => "The site says that image doesn't exist. Check the link.".into(),
            code => format!("The site answered with an error ({code}). Try again later."),
        });
    }
    if response.content_length().is_some_and(|n| n > cap) {
        return Err(too_big(cap));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let url = response.url().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
        if bytes.len() as u64 + chunk.len() as u64 > cap {
            return Err(too_big(cap));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Fetched {
        bytes,
        content_type,
        url,
    })
}

/// A plain download of an address a site named (a link's thumbnail), up to `cap`
/// bytes, as a browser would ask for it. Never from this computer or the home network.
pub async fn download(url: &Url, cap: u64) -> Result<Vec<u8>, String> {
    Ok(get(url, cap, Duration::from_secs(20), true).await?.bytes)
}

fn too_big(cap: u64) -> String {
    format!(
        "That's bigger than {} MB, which is more than any picture needs. It may be a video or a download page.",
        cap / 1024 / 1024
    )
}

fn looks_like_html(fetched: &Fetched) -> bool {
    if fetched.content_type.starts_with("text/html")
        || fetched.content_type.starts_with("application/xhtml")
    {
        return true;
    }
    let head = String::from_utf8_lossy(&fetched.bytes[..fetched.bytes.len().min(512)])
        .trim_start()
        .to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

/// A readable name from the link: its file name, or the site if that's a hash.
fn name_from(url: &Url) -> String {
    let segment = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or_default();
    let decoded = percent_decode(segment);
    let stem = Path::new(&decoded)
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .unwrap_or_default();
    // Long runs of hex or digits are ids, not names.
    let meaningful = stem.chars().filter(|c| c.is_alphabetic()).count() >= 3
        && !(stem.len() >= 16 && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    if meaningful {
        stem.chars().take(80).collect()
    } else {
        let host = url.host_str().unwrap_or("image");
        format!("Image from {}", host.trim_start_matches("www."))
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `dir/name.ext`, or `dir/name (2).ext` and so on: never overwrites.
fn unique(dir: &Path, name: &str, ext: &str) -> PathBuf {
    (1..)
        .map(|n| match n {
            1 => dir.join(format!("{name}.{ext}")),
            n => dir.join(format!("{name} ({n}).{ext}")),
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{name}.{ext}")))
}

/// Formats Premiere imports as they are, and the extension they're saved with.
fn keep_as(codec: &str) -> Option<&'static str> {
    Some(match codec {
        "png" => "png",
        "mjpeg" => "jpg",
        "gif" => "gif",
        "bmp" => "bmp",
        "tiff" => "tif",
        "psd" => "psd",
        _ => return None,
    })
}

fn format_label(codec: &str, info: &media::MediaInfo) -> String {
    match codec {
        "webp" => "WebP".into(),
        "av1" => "AVIF".into(),
        "hevc" => "HEIC".into(),
        "svg" => "SVG".into(),
        "jpegxl" => "JPEG XL".into(),
        "qoi" => "QOI".into(),
        _ => info
            .format_long_name
            .clone()
            .unwrap_or_else(|| codec.to_uppercase()),
    }
}

pub struct Context<'a> {
    pub app: &'a AppHandle,
    pub temp_dir: &'a Path,
    pub download_dir: &'a Path,
    pub saved: &'a SavedImages,
}

/// Downloads the image a link points at and saves it, converted if needed.
pub async fn from_link(ctx: Context<'_>, link: &str) -> Result<SavedImage, String> {
    let link = Url::parse(link.trim())
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or("That doesn't look like a complete link. Copy it again from the address bar.")?;
    let url = original_url(&link)?;
    // The link as typed may be anywhere (a NAS on the home network, say). An address
    // inside a Google or Bing result counts as typed; anything a page names doesn't.
    let mut fetched = get(&url, MAX_IMAGE, TIMEOUT, false).await?;
    if looks_like_html(&fetched) {
        if fetched.bytes.len() as u64 > MAX_PAGE {
            return Err(too_big(MAX_PAGE));
        }
        let html = String::from_utf8_lossy(&fetched.bytes);
        let image = page_image(&html, &fetched.url).ok_or(
            "That page doesn't name a main picture. Right-click the image you want and choose Copy image address (or Copy image), then paste that here.",
        )?;
        fetched = get(&image, MAX_IMAGE, TIMEOUT, true).await?;
        if looks_like_html(&fetched) {
            return Err("That link leads to another web page, not an image. Right-click the image and choose Copy image address.".into());
        }
    }

    let mut notes = Vec::new();
    if is_thumbnail(&fetched.url) {
        notes.push("This is Google's small preview, not the original. Click the result, wait for the large preview, then copy its address instead.".to_string());
    }
    let source = fetched.url.to_string();
    let name = name_from(&fetched.url);
    let saved = save(&ctx, &fetched.bytes, &name, Some(source), notes).await?;
    Ok(saved)
}

/// Saves an image copied to the clipboard (a browser's Copy image).
pub async fn from_bytes(ctx: Context<'_>, bytes: Vec<u8>) -> Result<SavedImage, String> {
    if bytes.is_empty() {
        return Err("There's no image on the clipboard.".into());
    }
    if bytes.len() as u64 > MAX_IMAGE {
        return Err(too_big(MAX_IMAGE));
    }
    save(&ctx, &bytes, "Pasted image", None, Vec::new()).await
}

async fn save(
    ctx: &Context<'_>,
    bytes: &[u8],
    name: &str,
    source: Option<String>,
    mut notes: Vec<String>,
) -> Result<SavedImage, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let temp = ctx.temp_dir.join(format!("image-{id}"));
    std::fs::write(&temp, bytes).map_err(|e| format!("Couldn't save it: {e}"))?;
    let result = convert(ctx, &temp, name).await;
    let _ = std::fs::remove_file(&temp);
    let (path, converted_from, width, height) = result?;

    if let (Some(w), Some(h)) = (width, height)
        && w.max(h) < SMALL
    {
        notes.push(format!(
            "It's only {w}×{h}, so it will look soft if it fills the frame. A bigger copy may exist."
        ));
    }
    let image = SavedImage {
        id,
        path,
        width,
        height,
        converted_from,
        notes,
        source,
    };
    ctx.saved.add(&image);
    Ok(image)
}

type Converted = (PathBuf, Option<String>, Option<u32>, Option<u32>);

async fn convert(ctx: &Context<'_>, temp: &Path, name: &str) -> Result<Converted, String> {
    let ffprobe = Tool::Ffprobe.path(ctx.app)?;
    let ffmpeg = Tool::Ffmpeg.path(ctx.app)?;
    let dir = ctx.download_dir.join("Images");
    let temp = temp.to_path_buf();
    let name = crate::settings::safe_file_name(name);
    tauri::async_runtime::spawn_blocking(move || -> Result<Converted, String> {
        let info = media::probe(&ffprobe, &temp)
            .map_err(|_| "That isn't an image the toolkit can read.".to_string())?;
        if !info.is_image() {
            return Err(
                if info.video().next().is_some() || info.audio().next().is_some() {
                    "That's a video or sound, not an image. Use Download for videos.".into()
                } else {
                    "That isn't an image the toolkit can read.".into()
                },
            );
        }
        let codec = info
            .streams
            .first()
            .map(|s| s.codec())
            .unwrap_or("unknown")
            .to_string();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;

        let (path, converted_from) = if let Some(ext) = keep_as(&codec) {
            let path = unique(&dir, &name, ext);
            std::fs::copy(&temp, &path).map_err(|e| format!("Couldn't save it: {e}"))?;
            (path, None)
        } else {
            let path = unique(&dir, &name, "png");
            let mut cmd = command(&ffmpeg);
            cmd.args(["-hide_banner", "-nostdin", "-v", "error", "-n"]);
            if codec == "svg" {
                cmd.args(crate::modules::svg_size_args(&info));
            }
            // ffmpeg turns the picture upright and assembles a HEIC's tiles itself.
            // A downloaded file is untrusted: it may only be read as a file.
            cmd.args(["-protocol_whitelist", "file"]);
            cmd.arg("-i").arg(&temp);
            cmd.args(["-frames:v", "1", "-map_metadata", "-1", "-update", "1"]);
            cmd.arg(&path);
            let out = run(&mut cmd, CONVERT_TIMEOUT)
                .map_err(|e| format!("Couldn't start ffmpeg: {e}"))?;
            if !out.status.success() || !path.is_file() {
                let _ = std::fs::remove_file(&path);
                return Err(format!(
                    "Couldn't convert this {} to PNG.",
                    format_label(&codec, &info)
                ));
            }
            (path, Some(format_label(&codec, &info)))
        };
        // Measure what was saved: a HEIC's first stream is just one tile.
        let saved = media::probe(&ffprobe, &path).ok();
        let first = saved.as_ref().and_then(|i| i.streams.first());
        Ok((
            dunce::simplified(&path).to_path_buf(),
            converted_from,
            first.and_then(|s| s.width),
            first.and_then(|s| s.height),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Google Lens, searching for other copies of this image.
pub fn lens_url(source: &str) -> Result<Url, String> {
    Url::parse_with_params("https://lens.google.com/uploadbyurl", [("url", source)])
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_results_lead_to_the_original() {
        let google = Url::parse("https://www.google.com/imgres?imgurl=https%3A%2F%2Fexample.com%2Fcat.webp&imgrefurl=https%3A%2F%2Fexample.com%2F&tbnid=x").unwrap();
        assert_eq!(
            original_url(&google).unwrap().as_str(),
            "https://example.com/cat.webp"
        );
        let bing = Url::parse("https://www.bing.com/images/search?view=detailV2&mediaurl=https%3A%2F%2Fcdn.example.org%2Fa.png").unwrap();
        assert_eq!(
            original_url(&bing).unwrap().as_str(),
            "https://cdn.example.org/a.png"
        );
        let search = Url::parse("https://www.google.com/search?q=cat&udm=2").unwrap();
        assert!(
            original_url(&search)
                .unwrap_err()
                .contains("Copy image address")
        );
        let plain = Url::parse("https://example.com/a.jpg").unwrap();
        assert_eq!(original_url(&plain).unwrap(), plain);
    }

    #[test]
    fn only_internet_addresses_count_as_public() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        for public in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "100.63.0.1",
            "2606:4700::1111",
        ] {
            assert!(is_public(ip(public)), "{public}");
        }
        for private in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "::",
            "fe80::1",
            "fd12:3456::1",
            "::ffff:192.168.0.1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public(ip(private)), "{private}");
        }
    }

    #[tokio::test]
    async fn a_named_address_on_this_computer_is_not_fetched() {
        let t = Duration::from_secs(2);
        for url in [
            "http://127.0.0.1:9/a.png",
            "http://[::1]/a.png",
            "http://localhost/a.png",
            "http://printer.localhost/a.png",
            "http://192.168.0.1/a.png",
            "http://2130706433/a.png",
        ] {
            let url = Url::parse(url).unwrap();
            let err = client_for(&url, true, t).await.unwrap_err();
            assert!(err.contains("own network"), "{url}: {err}");
        }
        // The one the person typed is allowed through to the network.
        assert!(
            client_for(&Url::parse("http://127.0.0.1/a.png").unwrap(), false, t)
                .await
                .is_ok()
        );
    }

    #[test]
    fn spots_google_previews() {
        assert!(is_thumbnail(
            &Url::parse("https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9Gc").unwrap()
        ));
        assert!(!is_thumbnail(
            &Url::parse("https://upload.wikimedia.org/a.png").unwrap()
        ));
    }

    #[test]
    fn finds_a_pages_main_picture() {
        let base = Url::parse("https://example.com/post/1").unwrap();
        let html = r#"<html><head>
            <meta name="twitter:image" content="https://cdn.example.com/t.jpg">
            <meta property="og:image" content="/img/main.png?w=1200&amp;h=630" />
            <meta data-property="og:image" content="wrong">
        </head></html>"#;
        assert_eq!(
            page_image(html, &base).unwrap().as_str(),
            "https://example.com/img/main.png?w=1200&h=630"
        );
        let html = "<link rel='image_src' href='https://example.com/x.jpg'>";
        assert_eq!(
            page_image(html, &base).unwrap().as_str(),
            "https://example.com/x.jpg"
        );
        assert!(page_image("<p>no pictures</p>", &base).is_none());
    }

    #[test]
    fn names_come_from_the_link() {
        let named = Url::parse("https://example.com/files/Blue%20Logo%20(final).webp").unwrap();
        assert_eq!(name_from(&named), "Blue Logo (final)");
        let hashed =
            Url::parse("https://cdn.example.com/a37fe850868f9db7ab3e98fbc65cd428.jpg").unwrap();
        assert_eq!(name_from(&hashed), "Image from cdn.example.com");
        let bare = Url::parse("https://www.example.com/").unwrap();
        assert_eq!(name_from(&bare), "Image from example.com");
    }

    #[test]
    fn premiere_formats_are_kept() {
        assert_eq!(keep_as("png"), Some("png"));
        assert_eq!(keep_as("mjpeg"), Some("jpg"));
        assert_eq!(keep_as("webp"), None);
        assert_eq!(keep_as("av1"), None);
    }
}
