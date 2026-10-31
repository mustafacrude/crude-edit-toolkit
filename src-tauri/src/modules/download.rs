//! Download: a link in, an edit-ready file out, via yt-dlp. The default picks the
//! best H.264 + AAC so most downloads need no re-encode (BRIEF §6). Also whole
//! playlists and channels, and chat replays (YouTube via yt-dlp, Twitch via
//! TwitchDownloaderCLI). Sound-only sites like SoundCloud keep their own audio,
//! tagged, with its cover art.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncReadExt;

use super::{Accepts, Category, JobInput, Lane, Module, Plan, PlanContext, Step};
use crate::errors;
use crate::progress::{ProgressKind, YTDLP_PROGRESS_ARGS};
use crate::settings::{Settings, SignIn, naming_render, naming_to_ytdlp};
use crate::timecode::{Range, parse_ranges, secs};
use crate::tools::{self, Tool};

pub struct Download;

/// yt-dlp's own "-t mp4" sort: H.264 first, then quality, AAC audio.
const PREMIERE_SORT: &str = "vcodec:h264,lang,quality,res,fps,hdr:12,acodec:aac";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// Best H.264 + AAC: no re-encode needed.
    #[default]
    Premiere,
    /// Best available, whatever the codec.
    Highest,
    /// A specific format from the picker (`format_id`).
    Format,
    /// Just the sound, as a 320 kbps MP3.
    Mp3,
    /// The site's own audio, left as it is, with tags and cover art.
    Audio,
    /// Just the sound, uncompressed for the timeline.
    Wav,
}

impl Quality {
    fn is_audio(self) -> bool {
        matches!(self, Self::Mp3 | Self::Audio | Self::Wav)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadOptions {
    pub quality: Quality,
    pub format_id: Option<String>,
    /// One range per entry, e.g. "1:02:10-1:03:45".
    pub sections: Vec<String>,
    /// Re-encode around cuts for frame-accurate sections.
    pub exact_cuts: bool,
    pub thumbnail: bool,
    /// jpg (default), png or webp.
    pub thumbnail_format: Option<String>,
    pub subtitles: bool,
    pub chapters: bool,
    pub description: bool,
    pub info_json: bool,
    /// Use the sign-in method from Settings.
    pub sign_in: bool,
    /// Shown as the job title; the link is used if missing.
    pub title: Option<String>,
    /// The whole playlist or channel tab the link points at, into its own folder.
    pub playlist: bool,
    /// The list is newest first (a channel), so stop at the first video older
    /// than `date_after` instead of checking the whole channel.
    pub newest_first: bool,
    /// Only the first N videos of the list (on a channel, the newest N).
    pub max_items: Option<u32>,
    /// YYYY-MM-DD, inclusive.
    pub date_after: Option<String>,
    /// YYYY-MM-DD, inclusive.
    pub date_before: Option<String>,
    /// Download videos again even if an earlier playlist download saved them.
    pub redownload: bool,
    /// Also save the chat replay as JSON.
    pub chat: bool,
    /// Save only the chat replay, not the video.
    pub chat_only: bool,
    /// The lookup found sound but no picture.
    pub audio_only: bool,
    /// The link's details from the lookup, to name files yt-dlp doesn't write.
    pub meta: Option<LinkMeta>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LinkMeta {
    pub title: Option<String>,
    pub uploader: Option<String>,
    /// YYYY-MM-DD
    pub upload_date: Option<String>,
    pub id: Option<String>,
    pub site: Option<String>,
    /// Seconds, to catch a section that starts after the video ends.
    pub duration: Option<f64>,
    /// The file type of the best audio on offer, on a sound-only link.
    pub audio_ext: Option<String>,
}

impl Module for Download {
    fn id(&self) -> &'static str {
        "download"
    }

    fn name(&self) -> &'static str {
        "Download"
    }

    fn description(&self) -> &'static str {
        "Downloads a video from YouTube, Twitch and most other sites, as the best H.264 version by default so Premiere opens it as-is."
    }

    fn category(&self) -> Category {
        Category::Download
    }

    fn accepts(&self) -> Accepts {
        Accepts::Link
    }

    fn lane(&self) -> Lane {
        Lane::Download
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let JobInput::Url(url) = input else {
            return Err("Download needs a link, not a file.".into());
        };
        let mut opts: DownloadOptions = if options.is_null() {
            DownloadOptions::default()
        } else {
            serde_json::from_value(options.clone())
                .map_err(|e| format!("Bad download options: {e}"))?
        };
        // A link with no picture has no video formats to pick from, whatever was asked.
        if (opts.audio_only || is_soundcloud(url)) && !opts.quality.is_audio() {
            opts.quality = Quality::Audio;
        }
        let settings = ctx.settings();
        let dir = PathBuf::from(&settings.download_dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Couldn't create the download folder {}: {e}", dir.display()))?;

        let ranges = parse_ranges(&opts.sections).map_err(|e| format!("Sections: {e}"))?;
        if let Some(length) = opts.meta.as_ref().and_then(|m| m.duration) {
            // The tool would fail with a code; say what's wrong instead.
            if let Some(r) = ranges.iter().find(|r| r.start >= length) {
                return Err(format!(
                    "A section starts at {}, but this video is only {} long. Change the section and try again.",
                    clock(r.start),
                    clock(length)
                ));
            }
        }
        let chat = opts.chat || opts.chat_only;
        let twitch = is_twitch(url);
        if opts.playlist && !ranges.is_empty() {
            return Err("Sections work on one video at a time, not a whole playlist.".into());
        }
        if opts.playlist && chat && twitch {
            return Err(
                "Twitch chat can be saved for one video or clip at a time, not a whole channel."
                    .into(),
            );
        }

        let title = opts.title.as_deref().unwrap_or(url);
        let mut plan = Plan {
            title: if opts.chat_only {
                format!("Chat for {title}")
            } else {
                format!("Download {title}")
            },
            partial_ok: opts.playlist,
            writes_to: vec![dir.clone()],
            ..Default::default()
        };
        if !(twitch && opts.chat_only) {
            let step = ytdlp_step(ctx, &settings, &opts, url, &ranges, &dir, &mut plan)?;
            plan.steps.push(step);
        }
        if twitch && chat {
            let step = twitch_chat_step(ctx, &settings, &opts, url, &ranges, &dir, &mut plan)?;
            plan.steps.push(step);
        }
        Ok(plan)
    }
}

/// The yt-dlp run: the video (or a whole playlist), and YouTube chat with it.
fn ytdlp_step(
    ctx: &PlanContext,
    settings: &Settings,
    opts: &DownloadOptions,
    url: &str,
    ranges: &[Range],
    dir: &Path,
    plan: &mut Plan,
) -> Result<Step, String> {
    let output_list = ctx.temp_path("outputs", "txt");
    plan.output_list = Some(output_list.clone());
    plan.temp_files.push(output_list.clone());

    let mut args = base_args(ctx.app)?;
    let mut name = naming_to_ytdlp(&settings.naming_template)?;
    if !ranges.is_empty() {
        // Each section is its own file; keep their names apart.
        name.push_str(" [%(section_start)s-%(section_end)s]");
    }
    name.push_str(conversion_suffix(opts.quality));
    // A playlist gets a folder of its own.
    let folder = if opts.playlist {
        format!("%(playlist_title,playlist_id|Playlist)s{MAIN_SEPARATOR}")
    } else {
        String::new()
    };
    args.extend([
        "--no-overwrites".into(),
        "--windows-filenames".into(),
        "--trim-filenames".into(),
        "160".into(),
        "-P".into(),
        dir.display().to_string(),
        "-o".into(),
        format!("{folder}{name}.%(ext)s"),
        "--print-to-file".into(),
        "after_move:filepath".into(),
        template_path(&output_list),
    ]);

    if opts.playlist {
        args.extend(playlist_args(ctx.app, opts)?);
    } else {
        args.push("--no-playlist".into());
    }
    // Back off between retries (1, 2, 4… up to 30 s) instead of hammering a
    // site that's struggling or rate-limiting.
    args.extend(
        [
            "--retry-sleep",
            "http:exp=1:30",
            "--retry-sleep",
            "fragment:exp=1:30",
        ]
        .map(String::from),
    );
    if settings.speed_limit_mb > 0.0 {
        // Per download: yt-dlp can't share one budget between separate runs.
        let bytes = (settings.speed_limit_mb * 1024.0 * 1024.0).max(64.0 * 1024.0) as u64;
        args.extend(["--limit-rate".into(), bytes.to_string()]);
    }

    if opts.chat_only {
        args.push("--skip-download".into());
    } else {
        args.extend(quality_args(opts)?);
        if opts.quality.is_audio() {
            args.extend(audio_tag_args(opts, url));
        }
    }

    for range in ranges {
        args.extend([
            "--download-sections".into(),
            format!("*{}-{}", secs(range.start), secs(range.end)),
        ]);
    }
    // Keyframes are a picture thing: sound cuts where it's asked to.
    if !ranges.is_empty() && opts.exact_cuts && !opts.quality.is_audio() {
        args.push("--force-keyframes-at-cuts".into());
    }

    if opts.thumbnail {
        let format = opts
            .thumbnail_format
            .as_deref()
            .filter(|f| matches!(*f, "png" | "webp"))
            .unwrap_or("jpg");
        args.extend(["--write-thumbnail", "--convert-thumbnails", format].map(String::from));
    }

    // YouTube keeps a live stream's chat replay as a "live_chat" subtitle track.
    let youtube_chat = (opts.chat || opts.chat_only) && !is_twitch(url);
    let subtitles = opts.subtitles && !opts.chat_only;
    if subtitles || youtube_chat {
        args.push("--write-subs".into());
        if subtitles {
            args.extend(["--write-auto-subs", "--convert-subs", "srt"].map(String::from));
        }
        // English only, and not the dozens of auto-translated tracks ("en-de-DE",
        // "en-ar"…) that "en.*" would match: YouTube refuses after the first few.
        let langs = match (subtitles, youtube_chat) {
            (true, true) => "en,en-US,en-GB,en-orig,live_chat",
            (true, false) => "en,en-US,en-GB,en-orig",
            _ => "live_chat",
        };
        args.extend(["--sub-langs".into(), langs.into()]);
        args.extend(["--sleep-subtitles".into(), "1".into()]);
    }
    if youtube_chat {
        // "NA" when a video has no chat; collecting outputs skips it.
        args.extend([
            "--print-to-file".into(),
            "after_video:%(requested_subtitles.live_chat.filepath)s".into(),
            template_path(&output_list),
        ]);
    }

    if opts.chapters && !opts.chat_only {
        args.push("--embed-chapters".into());
        args.extend([
            "--print-to-file".into(),
            "after_move:%(chapters)j".into(),
            format!(
                "{}{MAIN_SEPARATOR}{folder}{name}.chapters.json",
                template_path(dir)
            ),
        ]);
    }
    if opts.description {
        args.push("--write-description".into());
    }
    if opts.info_json {
        args.push("--write-info-json".into());
    }

    let mut redact = Vec::new();
    if opts.sign_in {
        let sign_in = sign_in_args(settings, ctx.temp_path("cookies", "txt"))?;
        if let Some(i) = sign_in.secret_index {
            redact.push(args.len() + i);
        }
        plan.temp_files.extend(sign_in.temp);
        args.extend(sign_in.args);
    }

    // Everything after "--" is the link, never an option.
    args.extend(["--".into(), url.to_string()]);

    Ok(Step {
        label: if opts.chat_only {
            "Downloading chat".into()
        } else {
            "Downloading".into()
        },
        tool: Tool::YtDlp,
        args,
        progress: ProgressKind::YtDlp {
            items: opts.max_items.filter(|_| opts.playlist),
            // With a date range nobody knows how many videos will match.
            open_ended: opts.playlist && (opts.date_after.is_some() || opts.date_before.is_some()),
        },
        redact,
    })
}

/// A converted file needs a name of its own. yt-dlp takes any file with the name it's
/// about to download as already done, converts that, then deletes it, so an MP3 made
/// without this would eat the video or track saved under the same name before it.
fn conversion_suffix(quality: Quality) -> &'static str {
    match quality {
        Quality::Mp3 => " (MP3)",
        Quality::Wav => " (WAV)",
        _ => "",
    }
}

fn quality_args(opts: &DownloadOptions) -> Result<Vec<String>, String> {
    Ok(match opts.quality {
        Quality::Premiere => [
            "-S",
            PREMIERE_SORT,
            "--merge-output-format",
            "mp4",
            "--remux-video",
            "mp4",
        ]
        .map(String::from)
        .to_vec(),
        Quality::Highest => ["--merge-output-format", "mp4/mkv"]
            .map(String::from)
            .to_vec(),
        Quality::Format => {
            let id = opts
                .format_id
                .as_deref()
                .filter(|id| {
                    !id.is_empty()
                        && id
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
                })
                .ok_or("Pick a format first.")?;
            vec![
                "-f".into(),
                format!("{id}+bestaudio/{id}"),
                "--merge-output-format".into(),
                "mp4/mkv".into(),
            ]
        }
        // The best audio on offer, converted once. Title, uploader and cover go in the tags.
        Quality::Mp3 => [
            "-f",
            "bestaudio/best",
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "320K",
            "--embed-metadata",
            "--embed-thumbnail",
        ]
        .map(String::from)
        .to_vec(),
        Quality::Audio => {
            let ext = opts.meta.as_ref().and_then(|m| m.audio_ext.as_deref());
            let mut args: Vec<String> = [
                "-f",
                // A list's tracks aren't known yet, so its art-less originals sit out.
                if ext.is_some() {
                    "bestaudio/best"
                } else {
                    ANY_TRACK_AUDIO
                },
                "--extract-audio",
                "--audio-format",
                KEEP_AUDIO,
                "--audio-quality",
                "256K",
                "--embed-metadata",
            ]
            .map(String::from)
            .to_vec();
            if ext.is_none_or(holds_cover_art) {
                args.push("--embed-thumbnail".into());
            }
            // One track's art shouldn't stop the rest of a set.
            if opts.playlist {
                args.push("--ignore-errors".into());
            }
            args
        }
        // WAV has nowhere to put a cover.
        Quality::Wav => [
            "-f",
            "bestaudio/best",
            "--extract-audio",
            "--audio-format",
            "wav",
            "--postprocessor-args",
            "ExtractAudio+ffmpeg_o:-c:a pcm_s24le -ar 48000",
            "--embed-metadata",
        ]
        .map(String::from)
        .to_vec(),
    })
}

/// Left alone unless Premiere couldn't open it. FLAC, WAV, MP3 and AAC stay as they
/// are; Opus and Vorbis, which it can't read, become AAC.
const KEEP_AUDIO: &str = "opus>m4a/webm>m4a/ogg>m4a/aac>m4a/best";
/// Best audio, except the file types that can't hold cover art.
const ANY_TRACK_AUDIO: &str = "bestaudio[ext!=wav][ext!=aiff][ext!=aif][ext!=wma]/bestaudio/best";

/// Asking yt-dlp to embed art in any other file type fails the whole download.
fn holds_cover_art(ext: &str) -> bool {
    matches!(
        ext,
        "m4a" | "mp4" | "mp3" | "flac" | "opus" | "ogg" | "webm" | "aac"
    )
}

/// A SoundCloud set is an album: its name and running order go in the tags. Titles
/// stay whole, because "A - B" there is as often a track's own name as artist and title.
fn audio_tag_args(opts: &DownloadOptions, url: &str) -> Vec<String> {
    if !(opts.playlist && is_soundcloud(url) && (url.contains("/sets") || url.contains("/albums")))
    {
        return Vec::new();
    }
    [
        "--parse-metadata",
        "%(album,playlist_title)s:%(album)s",
        "--parse-metadata",
        "%(playlist_index)s:%(track_number)s",
    ]
    .map(String::from)
    .to_vec()
}

/// Which videos of a playlist or channel to get. Videos saved by an earlier
/// playlist download are skipped, using yt-dlp's download archive.
fn playlist_args(app: &AppHandle, opts: &DownloadOptions) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec![
        "--yes-playlist".into(),
        // A stream that's live right now would record until it ends.
        "--match-filters".into(),
        "!is_live".into(),
        // Pace a long list like a person would, so YouTube and Twitch don't
        // block the connection or ask to "confirm you're not a bot".
        "--sleep-requests".into(),
        "1".into(),
        "--sleep-interval".into(),
        "3".into(),
        "--max-sleep-interval".into(),
        "8".into(),
        "--sleep-subtitles".into(),
        "2".into(),
    ];
    if let Some(n) = opts.max_items {
        if n == 0 {
            return Err("Download at least one video.".into());
        }
        args.extend(["--playlist-items".into(), format!("1:{n}")]);
    }
    let after = opts.date_after.as_deref().map(ytdlp_date).transpose()?;
    let before = opts.date_before.as_deref().map(ytdlp_date).transpose()?;
    if let (Some(a), Some(b)) = (&after, &before)
        && a > b
    {
        return Err("The \"from\" date is after the \"to\" date.".into());
    }
    if let Some(after) = after {
        if opts.newest_first {
            // Everything past the first older video is older too. "?" lets a
            // video with no date through rather than stopping there.
            args.extend([
                "--break-match-filters".into(),
                format!("upload_date>=?{after}"),
            ]);
        } else {
            args.extend(["--dateafter".into(), after]);
        }
    }
    if let Some(before) = before {
        args.extend(["--datebefore".into(), before]);
    }
    if !opts.redownload {
        args.extend([
            "--download-archive".into(),
            archive_path(app)?.display().to_string(),
        ]);
    }
    Ok(args)
}

/// 59 → "0:59", 3725 → "1:02:05".
fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    match (s / 3600, s % 3600 / 60, s % 60) {
        (0, m, sec) => format!("{m}:{sec:02}"),
        (h, m, sec) => format!("{h}:{m:02}:{sec:02}"),
    }
}

/// "2026-10-01" → "20261001".
fn ytdlp_date(date: &str) -> Result<String, String> {
    let digits: String = date.trim().chars().filter(|c| *c != '-').collect();
    if digits.len() == 8 && digits.chars().all(|c| c.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err(format!("{date} isn't a date. Use YYYY-MM-DD."))
    }
}

/// Twitch chat via TwitchDownloaderCLI, named like the video beside it.
fn twitch_chat_step(
    ctx: &PlanContext,
    settings: &Settings,
    opts: &DownloadOptions,
    url: &str,
    ranges: &[Range],
    dir: &Path,
    plan: &mut Plan,
) -> Result<Step, String> {
    let meta = opts.meta.as_ref();
    let field = |f: &str| {
        let meta = meta?;
        match f {
            "title" => meta.title.clone().or_else(|| opts.title.clone()),
            "uploader" => meta.uploader.clone(),
            "upload_date" => meta.upload_date.as_ref().map(|d| d.replace('-', "")),
            "id" => meta.id.clone(),
            _ => meta.site.clone(),
        }
    };
    let mut name = naming_render(&settings.naming_template, field)?;
    // A title ending in "." would give "Title..chat.json".
    name = name.trim_end_matches(['.', ' ']).to_string();
    if let [range] = ranges {
        // Matches the video's " [start-end]" so the two pair up.
        name.push_str(&format!(" [{}-{}]", secs(range.start), secs(range.end)));
    }
    let chat = ctx.unique_in(dir, &format!("{name}.chat"), "json");
    plan.outputs.push(chat.clone());

    let mut args: Vec<String> = vec![
        "chatdownload".into(),
        "-u".into(),
        url.to_string(),
        "-o".into(),
        chat.display().to_string(),
        "--collision".into(),
        "Exit".into(),
        "--temp-path".into(),
        ctx.temp_dir.display().to_string(),
    ];
    // One section: just that stretch of chat. Several: all of it.
    if let [range] = ranges {
        args.extend([
            "-b".into(),
            format!("{}s", range.start.floor() as u64),
            "-e".into(),
            format!("{}s", range.end.ceil() as u64),
        ]);
    }
    Ok(Step {
        label: "Downloading chat".into(),
        tool: Tool::TwitchDownloader,
        args,
        progress: ProgressKind::TwitchChat,
        redact: Vec::new(),
    })
}

/// Whether the link's host is this site or one of its subdomains.
fn on_site(url: &str, site: &str) -> bool {
    let host = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host == site
        || host
            .strip_suffix(site)
            .is_some_and(|sub| sub.ends_with('.'))
}

fn is_twitch(url: &str) -> bool {
    on_site(url, "twitch.tv")
}

fn is_soundcloud(url: &str) -> bool {
    on_site(url, "soundcloud.com")
}

/// Videos saved by playlist downloads, one "site id" per line, so running a
/// channel again only gets what's new.
pub fn archive_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("download-archive.txt"))
        .map_err(|e| format!("Couldn't find the app's data folder: {e}"))
}

/// Options every yt-dlp run gets: ignore the user's own yt-dlp config and use
/// the bundled Deno and ffmpeg.
fn base_args(app: &AppHandle) -> Result<Vec<String>, String> {
    let deno = Tool::Deno.path(app)?;
    let ffmpeg = Tool::Ffmpeg.path(app)?;
    let mut args: Vec<String> = vec![
        "--ignore-config".into(),
        "--js-runtimes".into(),
        format!("deno:{}", deno.display()),
        "--ffmpeg-location".into(),
        ffmpeg.display().to_string(),
        // A connection that goes quiet is retried rather than hanging.
        "--socket-timeout".into(),
        "30".into(),
    ];
    args.extend(YTDLP_PROGRESS_ARGS.map(String::from));
    Ok(args)
}

struct SignInArgs {
    args: Vec<String>,
    /// Index within `args` of a value to hide from Show command and logs.
    secret_index: Option<usize>,
    temp: Option<PathBuf>,
}

/// Browser cookies, or a per-job copy of the user's cookies.txt. yt-dlp writes
/// refreshed cookies back to the file it's given, so it never gets the original.
fn sign_in_args(settings: &Settings, temp_copy: PathBuf) -> Result<SignInArgs, String> {
    match settings.sign_in {
        SignIn::None => Err(
            "Sign-in isn't set up. Choose your browser or a cookies.txt file in Settings.".into(),
        ),
        SignIn::Browser => Ok(SignInArgs {
            args: vec!["--cookies-from-browser".into(), settings.browser.clone()],
            secret_index: None,
            temp: None,
        }),
        SignIn::File => {
            let original = settings
                .cookies_file
                .as_deref()
                .ok_or("Choose a cookies.txt file in Settings first.")?;
            check_cookies_file(Path::new(original))?;
            std::fs::copy(original, &temp_copy)
                .map_err(|e| format!("Couldn't read the cookies file: {e}"))?;
            Ok(SignInArgs {
                args: vec!["--cookies".into(), temp_copy.display().to_string()],
                secret_index: Some(1),
                temp: Some(temp_copy),
            })
        }
    }
}

/// A Netscape-format cookies.txt: a header comment, or tab-separated 7-field lines.
pub fn check_cookies_file(path: &Path) -> Result<(), String> {
    // Real exports are kilobytes; anything this big is the wrong file.
    if std::fs::metadata(path).is_ok_and(|m| m.len() > 20 * 1024 * 1024) {
        return Err("That file is far too big to be a cookies.txt export. Choose the file your cookies extension saved.".into());
    }
    let text = std::fs::read_to_string(path).map_err(|_| {
        "Can't read the cookies file. It may have been moved or deleted; choose it again in Settings."
            .to_string()
    })?;
    let header = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let looks_right = header.starts_with("# Netscape HTTP Cookie File")
        || header.starts_with("# HTTP Cookie File")
        || text.lines().any(|l| l.split('\t').count() == 7);
    if looks_right {
        Ok(())
    } else {
        Err("That file isn't a cookies.txt export. Export one in Netscape format with a cookies.txt browser extension.".into())
    }
}

/// A literal path inside a yt-dlp output template, where % starts a field.
fn template_path(path: &Path) -> String {
    path.display().to_string().replace('%', "%%")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatView {
    pub id: String,
    /// e.g. "1080p60 · H.264 · mp4"
    pub label: String,
    pub height: Option<u64>,
    pub size_bytes: Option<u64>,
    /// No sound of its own; the best audio is added.
    pub video_only: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    /// YYYY-MM-DD
    pub upload_date: Option<String>,
    pub site: String,
    pub is_live: bool,
    pub chapters: usize,
    pub has_subtitles: bool,
    /// The best H.264 the site offers, e.g. "1080p60". None means a re-encode will be needed.
    pub best_h264: Option<String>,
    /// The best overall, e.g. "2160p60 VP9".
    pub best: Option<String>,
    pub formats: Vec<FormatView>,
    pub kind: LinkKind,
    pub id: Option<String>,
    /// A playlist or channel: what's in it.
    pub playlist: Option<PlaylistView>,
    /// Which chat replay can be saved with this video.
    pub chat: Option<ChatSource>,
    /// A video opened from a playlist: a link to the whole playlist.
    pub playlist_url: Option<String>,
    /// The site's preview picture, for the card.
    pub thumbnail: Option<String>,
    /// Sound with no picture: a track, or a list of them.
    pub audio_only: bool,
    /// The best sound on offer, e.g. "AAC · 160 kbps". Only on a sound-only link.
    pub best_audio: Option<String>,
    /// That audio's file type, e.g. "m4a".
    pub audio_ext: Option<String>,
    /// The file the artist uploaded is on offer, not just the site's stream.
    pub original_file: bool,
    /// All that's on offer is a short preview; the whole track needs a subscription.
    pub preview_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Video,
    Playlist,
    Channel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatSource {
    Youtube,
    Twitch,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistView {
    /// How many videos, when the site says.
    pub count: Option<u64>,
    /// The list is longer than the lookup read (`LOOKUP_ITEMS`).
    pub more: bool,
    /// The first few, as a preview.
    pub entries: Vec<EntryView>,
    /// A channel's home page: its tabs (Videos, Live, Shorts). Pick one to download.
    pub tabs: Vec<TabView>,
    /// A channel's uploads, newest first: date filters can stop early.
    pub newest_first: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub title: String,
    pub duration: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabView {
    pub label: String,
    pub url: String,
    pub count: Option<u64>,
    pub more: bool,
}

/// A lookup reads this many videos of a playlist; enough to preview and count.
const LOOKUP_ITEMS: usize = 100;
const PREVIEW_ITEMS: usize = 8;

/// A slow site gets this long to answer a lookup before it's stopped.
const LINK_INFO_TIMEOUT: Duration = Duration::from_secs(45);

/// Deletes a scratch file (a cookies copy) however the lookup ends.
struct TempFile(Option<PathBuf>);

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Fetches a link's title and formats without downloading (`yt-dlp -J`).
pub async fn link_info(
    app: &AppHandle,
    temp_dir: &Path,
    settings: &Settings,
    url: &str,
    sign_in: bool,
) -> Result<LinkInfo, String> {
    let JobInput::Url(url) = JobInput::parse(url)? else {
        return Err("That isn't a link.".into());
    };
    let mut args = base_args(app)?;
    // A video opened from a playlist is just the video. For a playlist or
    // channel, list the first videos without looking each one up.
    args.extend([
        "--no-playlist".into(),
        "-J".into(),
        "--flat-playlist".into(),
        "--playlist-items".into(),
        format!("1:{LOOKUP_ITEMS}"),
    ]);
    let mut _temp = TempFile(None);
    if sign_in {
        let s = sign_in_args(
            settings,
            temp_dir.join(format!("cookies-{}.txt", uuid::Uuid::new_v4())),
        )?;
        _temp = TempFile(s.temp);
        args.extend(s.args);
    }
    args.extend(["--".into(), url.clone()]);

    let mut child = tools::spawn(tools::async_command(&Tool::YtDlp.path(app)?).args(&args))
        .map_err(|e| format!("Couldn't start yt-dlp: {e}"))?;
    let read_all = |pipe: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>| {
        tauri::async_runtime::spawn(async move {
            let mut buf = Vec::new();
            if let Some(pipe) = pipe {
                // A lookup's answer is a few MB at most; never hold more than this.
                let _ = pipe.take(64 * 1024 * 1024).read_to_end(&mut buf).await;
            }
            buf
        })
    };
    let stdout = read_all(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = read_all(child.stderr.take().map(|p| Box::new(p) as _));

    let status = match tokio::time::timeout(LINK_INFO_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(format!("yt-dlp stopped unexpectedly: {e}")),
        Err(_) => {
            tools::kill_tree(&mut child).await;
            return Err("The site took too long to answer. Try again in a moment.".into());
        }
    };
    let stdout = stdout.await.unwrap_or_default();
    let stderr = stderr.await.unwrap_or_default();
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        let tail: Vec<String> = stderr.lines().map(String::from).collect();
        return Err(errors::explain(Tool::YtDlp, status.code(), &tail, sign_in));
    }
    let json: serde_json::Value = serde_json::from_slice(&stdout)
        .map_err(|e| format!("Couldn't read yt-dlp's answer: {e}"))?;
    parse_link_info(url, &json)
}

fn parse_link_info(url: String, json: &serde_json::Value) -> Result<LinkInfo, String> {
    let str_of = |key: &str| json.get(key).and_then(|v| v.as_str()).map(String::from);
    if str_of("_type").as_deref() == Some("playlist") {
        return Ok(parse_playlist(url, json));
    }

    let mut formats: Vec<FormatView> = Vec::new();
    let mut best_h264: Option<(u64, String)> = None;
    let mut best: Option<(u64, String)> = None;
    // Sites offer the same quality several ways (a direct file and a streaming
    // playlist). List direct files first so de-duplication keeps them.
    let mut raw: Vec<&serde_json::Value> = json
        .get("formats")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .collect();
    let audio_only = !raw.is_empty() && raw.iter().all(|f| is_audio_format(f));
    let sound = audio_only.then(|| best_sound(&raw)).flatten();
    raw.sort_by_key(|f| f.get("protocol").and_then(|v| v.as_str()) != Some("https"));
    for f in raw {
        let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("");
        if is_audio_format(f) {
            continue;
        }
        let Some(id) = f.get("format_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let height = f.get("height").and_then(|v| v.as_u64());
        let fps = f.get("fps").and_then(|v| v.as_f64()).filter(|v| *v > 30.5);
        let res = match (height, fps) {
            (Some(h), Some(fps)) => format!("{h}p{}", fps.round()),
            (Some(h), None) => format!("{h}p"),
            _ => f
                .get("format_note")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .to_string(),
        };
        let codec = vcodec_label(vcodec);
        let ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("");
        let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");

        let rank = height.unwrap_or(0) * 1000 + fps.unwrap_or(30.0) as u64;
        if codec == "H.264" && best_h264.as_ref().is_none_or(|(r, _)| rank > *r) {
            best_h264 = Some((rank, res.clone()));
        }
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, format!("{res} {codec}")));
        }
        let label = [res, codec.to_string(), ext.to_string()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        if formats.iter().any(|existing| existing.label == label) {
            continue;
        }
        formats.push(FormatView {
            id: id.to_string(),
            label,
            height,
            size_bytes: f
                .get("filesize")
                .or_else(|| f.get("filesize_approx"))
                .and_then(|v| v.as_u64()),
            video_only: acodec == "none",
        });
    }
    formats.sort_by_key(|f| std::cmp::Reverse(f.height.unwrap_or(0)));

    let non_empty = |key: &str| {
        json.get(key)
            .and_then(|v| v.as_object())
            .is_some_and(|m| m.keys().any(|k| k.starts_with("en") && k != "live_chat"))
    };
    let site = str_of("extractor_key").unwrap_or_default();
    let is_live = json
        .get("is_live")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_live_chat = json
        .get("subtitles")
        .and_then(|v| v.as_object())
        .is_some_and(|m| m.contains_key("live_chat"));
    let chat = if is_live {
        None // still going; its chat isn't a replay yet
    } else if has_live_chat {
        Some(ChatSource::Youtube)
    } else if matches!(site.as_str(), "TwitchVod" | "TwitchClips") {
        Some(ChatSource::Twitch)
    } else {
        None
    };
    Ok(LinkInfo {
        audio_only,
        best_audio: sound.as_ref().map(|s| s.label.clone()),
        audio_ext: sound.as_ref().map(|s| s.ext.clone()),
        original_file: sound.as_ref().is_some_and(|s| s.original),
        preview_only: sound.as_ref().is_some_and(|s| s.preview),
        playlist_url: youtube_list_url(&url),
        title: str_of("title").unwrap_or_else(|| "Untitled".into()),
        uploader: str_of("uploader").or_else(|| str_of("channel")),
        duration: json.get("duration").and_then(|v| v.as_f64()),
        upload_date: str_of("upload_date")
            .filter(|d| d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()))
            .map(|d| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..])),
        site,
        is_live,
        chapters: json
            .get("chapters")
            .and_then(|v| v.as_array())
            .map_or(0, Vec::len),
        has_subtitles: non_empty("subtitles") || non_empty("automatic_captions"),
        best_h264: best_h264.map(|(_, s)| s),
        best: best.map(|(_, s)| s),
        formats,
        kind: LinkKind::Video,
        id: str_of("id"),
        playlist: None,
        chat,
        thumbnail: str_of("thumbnail")
            .filter(|t| t.starts_with("https://") || t.starts_with("http://")),
        url,
    })
}

/// No picture, going by the codec or, when the site doesn't name one, the file type.
fn is_audio_format(format: &serde_json::Value) -> bool {
    match format.get("vcodec").and_then(|v| v.as_str()) {
        Some("none") => true,
        Some(_) => false,
        None => format
            .get("ext")
            .and_then(|v| v.as_str())
            .is_some_and(|ext| crate::inspect::AUDIO_EXTENSIONS.contains(&ext)),
    }
}

struct BestSound {
    label: String,
    ext: String,
    original: bool,
    preview: bool,
}

/// What the best audio is. yt-dlp lists formats worst first, the same order it picks by.
fn best_sound(formats: &[&serde_json::Value]) -> Option<BestSound> {
    let text = |f: &serde_json::Value, key: &str| {
        f.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let is_preview = |f: &serde_json::Value| {
        text(f, "format_id").contains("preview")
            || text(f, "format_note").eq_ignore_ascii_case("preview")
    };
    let sound: Vec<&&serde_json::Value> = formats
        .iter()
        .filter(|f| text(f, "acodec") != "none")
        .collect();
    let best = sound
        .iter()
        .rev()
        .find(|f| !is_preview(f))
        .or(sound.last())?;

    let ext = text(best, "ext");
    let original = text(best, "format_id") == "download";
    let codec = acodec_label(&text(best, "acodec"), &ext);
    let bitrate = best
        .get("abr")
        .or_else(|| best.get("tbr"))
        .and_then(|v| v.as_f64())
        .filter(|kbps| *kbps > 0.0)
        .map(|kbps| format!("{} kbps", kbps.round()));
    let label = if original {
        format!("Original {codec} file")
    } else {
        [Some(codec), bitrate]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    };
    Some(BestSound {
        label,
        ext,
        original,
        preview: is_preview(best),
    })
}

fn acodec_label(acodec: &str, ext: &str) -> String {
    let name = match acodec.split('.').next().unwrap_or("") {
        "mp4a" | "aac" => "AAC",
        "mp3" => "MP3",
        "opus" => "Opus",
        "vorbis" => "Vorbis",
        "flac" => "FLAC",
        "alac" => "ALAC",
        codec if codec.starts_with("pcm") => "WAV",
        _ => "",
    };
    if name.is_empty() {
        ext.to_ascii_uppercase()
    } else {
        name.to_string()
    }
}

/// Sites that are sound and nothing else, so a list of theirs is a list of tracks.
fn is_audio_site(site: &str) -> bool {
    ["Soundcloud", "Bandcamp", "Mixcloud", "Audiomack"]
        .iter()
        .any(|name| site.starts_with(name))
}

/// A SoundCloud profile's own parts. Its bare address mixes in whatever it reposted.
fn soundcloud_tabs(page: &str) -> Vec<TabView> {
    let page = page.trim_end_matches('/');
    [("Tracks", "tracks"), ("Albums", "albums"), ("Sets", "sets")]
        .into_iter()
        .map(|(label, path)| TabView {
            label: label.into(),
            url: format!("{page}/{path}"),
            count: None,
            more: false,
        })
        .collect()
}

/// A flat list can leave titles out; the link's last part still says what it is.
/// ".../world-on-fire-1" → "World on fire 1".
fn title_from_link(entry: &serde_json::Value) -> Option<String> {
    let link = entry.get("url")?.as_str()?;
    let slug = link
        .split(['?', '#'])
        .next()?
        .trim_end_matches('/')
        .rsplit('/')
        .next()?
        .replace(['-', '_'], " ");
    let mut letters = slug.trim().chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

fn parse_playlist(url: String, json: &serde_json::Value) -> LinkInfo {
    let str_of = |key: &str| json.get(key).and_then(|v| v.as_str()).map(String::from);
    let entries: Vec<&serde_json::Value> = json
        .get("entries")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .collect();
    let (count, more) = list_size(json, entries.len());

    // A channel's home page lists its tabs as playlists of their own.
    let mut tabs: Vec<TabView> = entries
        .iter()
        .filter(|e| e.get("_type").and_then(|v| v.as_str()) == Some("playlist"))
        .filter_map(|e| {
            let url = e.get("webpage_url")?.as_str()?.to_string();
            let title = e.get("title").and_then(|v| v.as_str()).unwrap_or("Videos");
            // "Blender - Videos" → "Videos"
            let label = title.rsplit(" - ").next().unwrap_or(title).to_string();
            let n = e
                .get("entries")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            let (count, more) = list_size(e, n);
            Some(TabView {
                label,
                url,
                count,
                more,
            })
        })
        .collect();

    let site = str_of("extractor_key").unwrap_or_default();
    let page = str_of("webpage_url").unwrap_or_else(|| url.clone());
    // Profile pages say "Name (All)"; a tab says "Name (Tracks)".
    if site == "SoundcloudUser" && str_of("title").is_some_and(|t| t.ends_with(" (All)")) {
        tabs = soundcloud_tabs(&page);
    }
    let channel = !tabs.is_empty()
        || site == "SoundcloudUser"
        || site.starts_with("TwitchVideos")
        || (site == "YoutubeTab"
            && ["/@", "/channel/", "/c/", "/user/"]
                .iter()
                .any(|p| page.contains(p)));

    // The biggest of the list's own pictures (YouTube lists the smallest first).
    let thumbnail = json
        .get("thumbnails")
        .and_then(|v| v.as_array())
        .and_then(|list| list.iter().rev().find_map(|t| t.get("url")?.as_str()))
        .filter(|t| t.starts_with("https://") || t.starts_with("http://"))
        .map(String::from);

    LinkInfo {
        audio_only: is_audio_site(&site),
        best_audio: None,
        audio_ext: None,
        original_file: false,
        preview_only: false,
        thumbnail,
        title: str_of("title")
            .map(|t| t.trim_end_matches(" (All)").to_string())
            .unwrap_or_else(|| "Untitled playlist".into()),
        uploader: str_of("uploader").or_else(|| str_of("channel")),
        duration: None,
        upload_date: None,
        site,
        is_live: false,
        chapters: 0,
        has_subtitles: false,
        best_h264: None,
        best: None,
        formats: Vec::new(),
        kind: if channel {
            LinkKind::Channel
        } else {
            LinkKind::Playlist
        },
        id: str_of("id"),
        playlist: Some(PlaylistView {
            count: if tabs.is_empty() { count } else { None },
            more: tabs.is_empty() && more,
            entries: entries
                .iter()
                .filter(|e| e.get("_type").and_then(|v| v.as_str()) != Some("playlist"))
                .take(PREVIEW_ITEMS)
                .map(|e| EntryView {
                    title: e
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| title_from_link(e))
                        .unwrap_or_else(|| "Untitled".into()),
                    duration: e.get("duration").and_then(|v| v.as_f64()),
                })
                .collect(),
            tabs,
            newest_first: channel,
        }),
        chat: None,
        playlist_url: None,
        url,
    }
}

/// How many videos a playlist has, and whether there are more than the lookup read.
fn list_size(json: &serde_json::Value, read: usize) -> (Option<u64>, bool) {
    // Playlists say how long they are; channel tabs don't.
    match json.get("playlist_count").and_then(|v| v.as_u64()) {
        Some(n) => (Some(n), false),
        None => (Some(read as u64), read >= LOOKUP_ITEMS),
    }
}

/// youtube.com/watch?v=…&list=PL… → the playlist's own link.
fn youtube_list_url(url: &str) -> Option<String> {
    let host = url
        .split_once("://")?
        .1
        .split('/')
        .next()?
        .to_ascii_lowercase();
    if !(host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com")) {
        return None;
    }
    let query = url.split_once('?')?.1.split('#').next()?;
    let list = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("list="))?;
    let valid = !list.is_empty()
        && list
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| format!("https://www.youtube.com/playlist?list={list}"))
}

fn vcodec_label(vcodec: &str) -> &'static str {
    match vcodec.split('.').next().unwrap_or("") {
        "avc1" | "avc3" | "h264" => "H.264",
        "hev1" | "hvc1" | "hevc" | "h265" => "HEVC",
        "vp09" | "vp9" => "VP9",
        "av01" | "av1" => "AV1",
        "vp8" => "VP8",
        "" => "",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_file_check() {
        let dir = std::env::temp_dir();
        let good = dir.join(format!("good-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(
            &good,
            "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tx\n",
        )
        .unwrap();
        assert!(check_cookies_file(&good).is_ok());
        let bad = dir.join(format!("bad-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&bad, "{\"cookies\": []}").unwrap();
        assert!(check_cookies_file(&bad).is_err());
        let _ = std::fs::remove_file(good);
        let _ = std::fs::remove_file(bad);
    }

    #[test]
    fn link_info_from_json() {
        let json = serde_json::json!({
            "title": "Big Buck Bunny", "uploader": "Blender", "duration": 596.0,
            "upload_date": "20141110", "extractor_key": "Youtube",
            "chapters": [{"title": "a"}, {"title": "b"}],
            "automatic_captions": {"en": [], "live_chat": []},
            "formats": [
                {"format_id": "140", "vcodec": "none", "acodec": "mp4a.40.2", "ext": "m4a"},
                {"format_id": "137", "vcodec": "avc1.640028", "acodec": "none", "height": 1080, "fps": 60.0, "ext": "mp4"},
                {"format_id": "315", "vcodec": "vp09.00.51.08", "acodec": "none", "height": 2160, "fps": 60.0, "ext": "webm"},
                {"format_id": "18", "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "height": 360, "fps": 30.0, "ext": "mp4"}
            ]
        });
        let info = parse_link_info("https://youtu.be/x".into(), &json).unwrap();
        assert_eq!(info.best_h264.as_deref(), Some("1080p60"));
        assert_eq!(info.best.as_deref(), Some("2160p60 VP9"));
        assert_eq!(info.upload_date.as_deref(), Some("2014-11-10"));
        assert_eq!(info.chapters, 2);
        assert!(info.has_subtitles);
        assert_eq!(info.formats.len(), 3);
        assert_eq!(info.formats[0].label, "2160p60 · VP9 · webm");
        assert!(!info.formats[2].video_only);

        assert_eq!(info.kind, LinkKind::Video);
        assert_eq!(info.chat, None);
    }

    #[test]
    fn channel_home_lists_tabs() {
        let tab = |name: &str, path: &str| {
            serde_json::json!({
                "_type": "playlist", "title": format!("Blender - {name}"),
                "webpage_url": format!("https://www.youtube.com/@BlenderOfficial/{path}"),
                "entries": [{"_type": "url", "title": "a", "duration": 60}]
            })
        };
        let json = serde_json::json!({
            "_type": "playlist", "title": "Blender", "uploader": "Blender",
            "extractor_key": "YoutubeTab", "playlist_count": 3,
            "webpage_url": "https://www.youtube.com/@BlenderOfficial",
            "entries": [tab("Videos", "videos"), tab("Live", "streams"), tab("Shorts", "shorts")]
        });
        let info =
            parse_link_info("https://www.youtube.com/@BlenderOfficial".into(), &json).unwrap();
        assert_eq!(info.kind, LinkKind::Channel);
        let list = info.playlist.unwrap();
        assert_eq!(
            list.tabs
                .iter()
                .map(|t| t.label.as_str())
                .collect::<Vec<_>>(),
            ["Videos", "Live", "Shorts"]
        );
        assert!(list.tabs[1].url.ends_with("/streams"));
        assert!(list.entries.is_empty());
        assert_eq!(list.count, None);
    }

    #[test]
    fn playlist_preview() {
        let entries: Vec<_> = (0..LOOKUP_ITEMS)
            .map(|i| serde_json::json!({"_type": "url", "title": format!("v{i}"), "duration": 10}))
            .collect();
        let tab = serde_json::json!({
            "_type": "playlist", "title": "Blender - Videos", "extractor_key": "YoutubeTab",
            "webpage_url": "https://www.youtube.com/@BlenderOfficial/videos", "entries": entries
        });
        let info = parse_link_info("u".into(), &tab).unwrap();
        assert_eq!(info.kind, LinkKind::Channel);
        let list = info.playlist.unwrap();
        assert!(list.newest_first && list.more);
        assert_eq!(list.entries.len(), PREVIEW_ITEMS);

        let playlist = serde_json::json!({
            "_type": "playlist", "title": "Fundamentals", "extractor_key": "YoutubeTab",
            "playlist_count": 43,
            "webpage_url": "https://www.youtube.com/playlist?list=PLx", "entries": []
        });
        let info = parse_link_info("u".into(), &playlist).unwrap();
        assert_eq!(info.kind, LinkKind::Playlist);
        let list = info.playlist.unwrap();
        assert_eq!(
            (list.count, list.more, list.newest_first),
            (Some(43), false, false)
        );
    }

    #[test]
    fn chat_and_playlist_links() {
        let vod = serde_json::json!({"title": "VOD", "extractor_key": "TwitchVod", "formats": []});
        assert_eq!(
            parse_link_info("https://www.twitch.tv/videos/1".into(), &vod)
                .unwrap()
                .chat,
            Some(ChatSource::Twitch)
        );
        let stream = serde_json::json!({
            "title": "Stream", "extractor_key": "Youtube", "formats": [],
            "subtitles": {"live_chat": [{"ext": "json"}]}
        });
        assert_eq!(
            parse_link_info("u".into(), &stream).unwrap().chat,
            Some(ChatSource::Youtube)
        );
        assert_eq!(
            youtube_list_url("https://www.youtube.com/watch?v=aqz&list=PLa1_-x&index=2").as_deref(),
            Some("https://www.youtube.com/playlist?list=PLa1_-x")
        );
        assert_eq!(
            youtube_list_url("https://www.youtube.com/watch?v=aqz"),
            None
        );
        assert_eq!(youtube_list_url("https://example.com/watch?list=PL1"), None);
        assert!(is_twitch("https://clips.twitch.tv/Abc"));
        assert!(is_twitch("https://www.twitch.tv/videos/1"));
        assert!(!is_twitch("https://nottwitch.tv/x"));
        assert_eq!(ytdlp_date("2026-10-01").unwrap(), "20261001");
        assert!(ytdlp_date("10/01/2026").is_err());
    }

    #[test]
    fn clock_reads_like_a_player() {
        assert_eq!(clock(19.4), "0:19");
        assert_eq!(clock(60.0), "1:00");
        assert_eq!(clock(3725.0), "1:02:05");
    }

    #[test]
    fn a_soundcloud_track_is_sound_only() {
        let json = serde_json::json!({
            "title": "Folk loop", "uploader": "Twisterium", "extractor_key": "Soundcloud",
            "formats": [
                {"format_id": "hls_aac_96k", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 96, "ext": "m4a"},
                {"format_id": "hls_aac_160k", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 160, "ext": "m4a"}
            ]
        });
        let info = parse_link_info("https://soundcloud.com/a/b".into(), &json).unwrap();
        assert!(info.audio_only && !info.preview_only && !info.original_file);
        assert_eq!(info.best_audio.as_deref(), Some("AAC · 160 kbps"));
        assert_eq!(info.audio_ext.as_deref(), Some("m4a"));
        assert!(info.formats.is_empty() && info.best_h264.is_none());
    }

    #[test]
    fn a_video_with_audio_formats_is_not_sound_only() {
        let json = serde_json::json!({
            "title": "v", "extractor_key": "Youtube",
            "formats": [
                {"format_id": "sb0", "vcodec": "none", "acodec": "none", "ext": "mhtml"},
                {"format_id": "140", "vcodec": "none", "acodec": "mp4a.40.2", "ext": "m4a"},
                {"format_id": "137", "vcodec": "avc1.640028", "acodec": "none", "height": 1080, "ext": "mp4"}
            ]
        });
        let info = parse_link_info("u".into(), &json).unwrap();
        assert!(!info.audio_only);
        assert_eq!(info.best_audio, None);
    }

    #[test]
    fn the_artists_own_file_and_previews_are_named() {
        let sound = |formats: serde_json::Value| {
            let json = serde_json::json!({"title": "t", "extractor_key": "Soundcloud", "formats": formats});
            parse_link_info("u".into(), &json).unwrap()
        };
        let info = sound(serde_json::json!([
            {"format_id": "hls_aac_160k", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 160, "ext": "m4a"},
            {"format_id": "download", "vcodec": "none", "ext": "wav"}
        ]));
        assert!(info.original_file);
        assert_eq!(info.best_audio.as_deref(), Some("Original WAV file"));
        assert_eq!(info.audio_ext.as_deref(), Some("wav"));

        let info = sound(serde_json::json!([
            {"format_id": "hls_aac_160k_preview", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 160, "ext": "m4a"}
        ]));
        assert!(info.preview_only);

        let info = sound(serde_json::json!([
            {"format_id": "hls_aac_96k", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 96, "ext": "m4a"},
            {"format_id": "hls_aac_160k_preview", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 160, "ext": "m4a"}
        ]));
        assert!(!info.preview_only);
        assert_eq!(info.best_audio.as_deref(), Some("AAC · 96 kbps"));
    }

    #[test]
    fn a_set_names_its_tracks_from_their_links() {
        let json = serde_json::json!({
            "_type": "playlist", "title": "The EP", "uploader": "The Band",
            "extractor_key": "SoundcloudSet", "playlist_count": 2,
            "webpage_url": "https://soundcloud.com/band/sets/the-ep",
            "entries": [
                {"_type": "url_transparent", "url": "https://soundcloud.com/band/world-on-fire-1"},
                {"_type": "url", "title": "Given", "url": "https://soundcloud.com/band/x"}
            ]
        });
        let info =
            parse_link_info("https://soundcloud.com/band/sets/the-ep".into(), &json).unwrap();
        assert!(info.audio_only);
        assert_eq!(info.kind, LinkKind::Playlist);
        let list = info.playlist.unwrap();
        assert_eq!(list.entries[0].title, "World on fire 1");
        assert_eq!(list.entries[1].title, "Given");
    }

    #[test]
    fn a_soundcloud_profile_offers_its_parts_and_a_tab_does_not() {
        let page = |title: &str, link: &str| {
            let json = serde_json::json!({
                "_type": "playlist", "title": title, "extractor_key": "SoundcloudUser",
                "webpage_url": link, "entries": [{"_type": "url", "title": "a", "url": "u"}]
            });
            parse_link_info(link.into(), &json).unwrap()
        };
        let info = page("Soft Cell (All)", "https://soundcloud.com/soft-cell/");
        assert_eq!(info.kind, LinkKind::Channel);
        assert_eq!(info.title, "Soft Cell");
        let list = info.playlist.unwrap();
        assert_eq!(list.tabs[0].url, "https://soundcloud.com/soft-cell/tracks");
        assert_eq!(list.tabs.len(), 3);

        let info = page(
            "Soft Cell (Tracks)",
            "https://soundcloud.com/soft-cell/tracks",
        );
        assert_eq!(info.kind, LinkKind::Channel);
        let list = info.playlist.unwrap();
        assert!(list.tabs.is_empty() && list.newest_first);
    }

    #[test]
    fn best_audio_keeps_what_premiere_opens_and_embeds_art_only_where_it_fits() {
        let with_ext = |ext: Option<&str>, playlist: bool| {
            let opts = DownloadOptions {
                quality: Quality::Audio,
                playlist,
                meta: Some(LinkMeta {
                    audio_ext: ext.map(String::from),
                    ..Default::default()
                }),
                ..Default::default()
            };
            quality_args(&opts).unwrap().join(" ")
        };
        let m4a = with_ext(Some("m4a"), false);
        assert!(m4a.contains("--embed-metadata --embed-thumbnail"));
        assert!(m4a.contains("opus>m4a"));
        assert!(!m4a.contains("320K"));
        assert!(!with_ext(Some("wav"), false).contains("--embed-thumbnail"));

        let list = with_ext(None, true);
        assert!(list.contains("[ext!=wav]") && list.contains("--embed-thumbnail"));
        assert!(list.contains("--ignore-errors"));
    }

    #[test]
    fn wav_is_24_bit_for_the_timeline_and_has_no_cover() {
        let opts = DownloadOptions {
            quality: Quality::Wav,
            ..Default::default()
        };
        let args = quality_args(&opts).unwrap().join(" ");
        assert!(args.contains("--audio-format wav"));
        assert!(args.contains("pcm_s24le"));
        assert!(!args.contains("--embed-thumbnail"));
    }

    #[test]
    fn a_conversion_never_shares_a_name_with_what_it_could_delete() {
        assert_eq!(conversion_suffix(Quality::Mp3), " (MP3)");
        assert_eq!(conversion_suffix(Quality::Wav), " (WAV)");
        // Nothing is converted away, so these keep the plain name.
        assert_eq!(conversion_suffix(Quality::Audio), "");
        assert_eq!(conversion_suffix(Quality::Premiere), "");
    }

    #[test]
    fn soundcloud_sets_become_albums() {
        let list = DownloadOptions {
            quality: Quality::Audio,
            playlist: true,
            ..Default::default()
        };
        let tags = audio_tag_args(&list, "https://soundcloud.com/a/sets/b").join(" ");
        assert!(tags.contains("track_number") && tags.contains("playlist_title"));
        assert!(audio_tag_args(&list, "https://soundcloud.com/a/tracks").is_empty());
        assert!(audio_tag_args(&list, "https://www.youtube.com/playlist?list=sets").is_empty());
    }

    #[test]
    fn soundcloud_is_told_apart_by_its_host() {
        assert!(is_soundcloud("https://soundcloud.com/a/b"));
        assert!(is_soundcloud("https://m.soundcloud.com/a/b"));
        assert!(is_soundcloud("https://on.soundcloud.com/abc"));
        assert!(!is_soundcloud("https://notsoundcloud.com/a"));
        assert!(!is_soundcloud("https://example.com/soundcloud.com"));
    }

    #[test]
    fn mp3_is_the_audio_alone() {
        let opts = DownloadOptions {
            quality: Quality::Mp3,
            ..Default::default()
        };
        let args = quality_args(&opts).unwrap().join(" ");
        assert!(args.contains("--extract-audio --audio-format mp3 --audio-quality 320K"));
        assert!(args.contains("--embed-metadata --embed-thumbnail"));
        assert!(!args.contains("--merge-output-format"));
    }
}
