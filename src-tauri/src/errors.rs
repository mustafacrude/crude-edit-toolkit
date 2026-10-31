//! Turns a failed tool run into one sentence a non-technical person can act on,
//! and, where there's something to click, a fix button. The raw output always
//! stays in the job log.

use serde::{Deserialize, Serialize};

use crate::tools::Tool;

/// Which Settings card a fix opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsSection {
    SignIn,
    Downloads,
    Encoding,
    Performance,
    Tools,
}

/// A button shown next to an error. Decided here, never guessed by the UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Fix {
    /// Open Settings at the card that fixes it.
    #[serde(rename_all = "camelCase")]
    OpenSettings { section: SettingsSection },
    /// Run the job again with the saved sign-in switched on.
    RetryWithSignIn,
    /// Update yt-dlp, then run the job again.
    UpdateAndRetry,
}

/// What kind of fix a message calls for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hint {
    None,
    SignIn,
    Folder,
    Encoder,
    Memory,
    Tools,
    /// An old yt-dlp is the likely cause.
    Update,
}

/// (text to find in the tool's output, what to tell the person, which fix helps).
/// Lines are scanned newest first and the first needle in this table that
/// matches wins, so specific needles go before general ones.
const PATTERNS: &[(&str, &str, Hint)] = &[
    // A graphics card that can't start its encoder. These come before the
    // memory ones: NVENC says "out of memory" when it has run out of sessions.
    (
        "OpenEncodeSessionEx failed",
        "Your graphics card couldn't start the encoder. It may be busy with another app, or its driver may need updating. Retry, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "Cannot load nvcuda.dll",
        "Your graphics card couldn't start the encoder. It may be busy with another app, or its driver may need updating. Retry, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "No capable devices found",
        "Your graphics card couldn't start the encoder. It may be busy with another app, or its driver may need updating. Retry, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "Driver does not support the required nvenc API version",
        "Your graphics card's driver is too old for this encoder. Update the driver, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "amfrt64.dll failed to open",
        "Your graphics card couldn't start the encoder. Its driver may need updating. Retry, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "Error initializing an MFX session",
        "Your graphics card couldn't start the encoder. Its driver may need updating. Retry, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "Error while opening encoder",
        "The video encoder couldn't start. If it's your graphics card, it may be busy or need a driver update: retry, or choose CPU only in Settings.",
        Hint::Encoder,
    ),
    (
        "No space left on device",
        "The drive is full. Free up some space and retry.",
        Hint::Folder,
    ),
    (
        "There is not enough space on the disk",
        "The drive is full. Free up some space and retry.",
        Hint::Folder,
    ),
    // The tools share a memory cap (guard.rs); hitting it ends the job, not the computer.
    (
        "Cannot allocate memory",
        "The job ran out of memory. Close other apps, or run fewer jobs at once, and retry.",
        Hint::Memory,
    ),
    (
        "Out of memory",
        "The job ran out of memory. Close other apps, or run fewer jobs at once, and retry.",
        Hint::Memory,
    ),
    (
        "MemoryError",
        "The job ran out of memory. Close other apps, or run fewer jobs at once, and retry.",
        Hint::Memory,
    ),
    (
        "Not enough memory",
        "The job ran out of memory. Close other apps, or run fewer jobs at once, and retry.",
        Hint::Memory,
    ),
    (
        "HTTP Error 429",
        "The site is limiting how fast you can download. Wait a few minutes, then retry.",
        Hint::None,
    ),
    (
        "Permission denied",
        "The toolkit isn't allowed to write there. Pick another folder or check the file isn't open elsewhere.",
        Hint::Folder,
    ),
    (
        "being used by another process",
        "Another app has this file open. Close it (Premiere, a player) and retry.",
        Hint::None,
    ),
    // yt-dlp's wrapper for any ffmpeg failure while cutting a section or merging.
    (
        "ffmpeg exited with code",
        "The video couldn't be cut or converted. If you set a section, check that it fits inside the video.",
        Hint::None,
    ),
    (
        "Invalid data found when processing input",
        "This file looks damaged or isn't a format the toolkit can read.",
        Hint::None,
    ),
    (
        "moov atom not found",
        "This file looks unfinished, as if a recording or download was cut off.",
        Hint::None,
    ),
    (
        "No such file or directory",
        "Couldn't open the file. It may have been moved or deleted, or its drive isn't connected.",
        Hint::None,
    ),
    (
        "Sign in to confirm your age",
        "This video is age-restricted. Turn on sign-in (in Settings) and try again.",
        Hint::SignIn,
    ),
    (
        "Sign in to confirm you",
        "YouTube blocked this download. Updating the downloader usually fixes it.",
        Hint::Update,
    ),
    (
        "members-only",
        "This video is members-only. Turn on sign-in (in Settings) with an account that has access.",
        Hint::SignIn,
    ),
    (
        "Join this channel",
        "This video is members-only. Turn on sign-in (in Settings) with an account that has access.",
        Hint::SignIn,
    ),
    (
        "subscriber-only",
        "This VOD is for subscribers only. Turn on sign-in (in Settings) with an account that's subscribed.",
        Hint::SignIn,
    ),
    // yt-dlp's general "log in first" ending, used by many sites (Vimeo, Patreon…).
    (
        "to provide account credentials",
        "This site wants you signed in to download this. Turn on sign-in (in Settings) and try again.",
        Hint::SignIn,
    ),
    ("Private video", "This video is private.", Hint::None),
    (
        "This live event will begin in",
        "This is a live stream that hasn't started yet. Try again once it has started.",
        Hint::None,
    ),
    (
        "Premieres in",
        "This is a premiere that hasn't started yet. Try again once it has started.",
        Hint::None,
    ),
    (
        "not made this video available in your country",
        "This video isn't available where you are.",
        Hint::None,
    ),
    (
        "This video is unavailable",
        "This video isn't available. It may have been removed, made private, or the link is wrong.",
        Hint::None,
    ),
    (
        "Video unavailable",
        "This video isn't available. It may have been removed, made private, or blocked where you are.",
        Hint::None,
    ),
    (
        "HTTP Error 404",
        "The site says that page doesn't exist. Check the link.",
        Hint::None,
    ),
    (
        "Unsupported URL",
        "The toolkit doesn't know how to download from this site.",
        Hint::None,
    ),
    (
        "is not a valid URL",
        "That doesn't look like a complete link. Copy it again from the address bar.",
        Hint::None,
    ),
    (
        "Requested format is not available",
        "That format isn't available any more. Pick another one.",
        Hint::None,
    ),
    (
        "Failed to decrypt with DPAPI",
        "Couldn't read your browser's sign-in. Chrome and Edge block this; use Firefox or a cookies.txt file instead.",
        Hint::SignIn,
    ),
    (
        "Could not copy Chrome cookie database",
        "Couldn't read your browser's sign-in while it's open. Close the browser and retry, or use Firefox or a cookies.txt file.",
        Hint::SignIn,
    ),
    (
        "cookies database",
        "Couldn't find that browser's sign-in on this computer. Pick the browser you actually use in Settings.",
        Hint::SignIn,
    ),
    (
        "Unable to download webpage",
        "Couldn't reach the site. Check your internet connection and retry.",
        Hint::None,
    ),
    (
        "Temporary failure in name resolution",
        "Couldn't reach the site. Check your internet connection and retry.",
        Hint::None,
    ),
    (
        "getaddrinfo failed",
        "Couldn't reach the site. Check your internet connection and retry.",
        Hint::None,
    ),
    (
        "HTTP Error 403",
        "The site refused the download. Updating the downloader usually fixes it.",
        Hint::Update,
    ),
    (
        "Unable to extract",
        "The site changed and the downloader can't read it yet. Updating the downloader usually fixes it.",
        Hint::Update,
    ),
    (
        "nsig extraction failed",
        "YouTube changed how its videos are served. Updating the downloader usually fixes it.",
        Hint::Update,
    ),
];

const SIGN_IN_REJECTED: &str = "The site didn't accept your sign-in. Sign in again in your browser (or export a fresh cookies.txt) and retry.";

/// `signed_in`: the run used cookies, so a bot check means the sign-in was rejected
/// rather than that yt-dlp is out of date.
pub fn explain(tool: Tool, exit_code: Option<i32>, log_tail: &[String], signed_in: bool) -> String {
    if signed_in
        && log_tail.iter().any(|l| {
            l.contains("Sign in to confirm you're not a bot")
                || l.contains("Sign in to confirm you\u{2019}re not a bot")
                || l.contains("cookies are no longer valid")
        })
    {
        return SIGN_IN_REJECTED.into();
    }
    for line in log_tail.iter().rev() {
        for (needle, message, _) in PATTERNS {
            if line.contains(needle) {
                return (*message).to_string();
            }
        }
    }
    // exiftool says "Error: <what> - <file path>"; the path is noise.
    if let Some(line) = log_tail.iter().rev().find(|l| l.starts_with("Error: ")) {
        let msg = line.trim_start_matches("Error: ");
        let msg = msg.rsplit_once(" - ").map_or(msg, |(what, _)| what);
        return format!("{} couldn't do it: {msg}", tool.name());
    }
    // No known pattern: quote the tool's own last error line, minus its prefixes
    // ("ERROR: [youtube] abc123: ").
    if let Some(line) = log_tail.iter().rev().find(|l| l.starts_with("ERROR:")) {
        let mut msg = line.trim_start_matches("ERROR:").trim();
        if msg.starts_with('[')
            && let Some((_, rest)) = msg.split_once("]")
        {
            msg = rest.trim();
            if let Some((id, rest)) = msg.split_once(": ")
                && !id.contains(' ')
            {
                msg = rest;
            }
        }
        return format!("{} couldn't do it: {msg}", tool.name());
    }
    match exit_code {
        Some(code) => format!(
            "{} stopped with an error (code {code}). The log has the details.",
            tool.name()
        ),
        None => format!(
            "{} stopped unexpectedly. The log has the details.",
            tool.name()
        ),
    }
}

/// What a fix needs to know about the job that failed.
pub struct FixContext<'a> {
    pub module: &'a str,
    /// The job already ran with the saved sign-in.
    pub signed_in: bool,
    /// Settings has a browser or cookies.txt sign-in set up.
    pub sign_in_ready: bool,
}

/// Messages that don't come from [`PATTERNS`] (planning, safety checks, sign-in
/// setup) and the fix each one needs.
const OTHER_MESSAGES: &[(&str, Hint)] = &[
    ("Sign-in isn't set up", Hint::SignIn),
    ("Choose a cookies.txt file in Settings first", Hint::SignIn),
    ("Can't read the cookies file", Hint::SignIn),
    ("That file isn't a cookies.txt export", Hint::SignIn),
    ("That file is far too big to be a cookies.txt", Hint::SignIn),
    (SIGN_IN_REJECTED, Hint::SignIn),
    ("Couldn't create the download folder", Hint::Folder),
    (" is free on ", Hint::Folder),
    ("is missing from the app folder", Hint::Tools),
];

fn hint_of(message: &str) -> Hint {
    if let Some((_, _, hint)) = PATTERNS.iter().find(|(_, m, _)| *m == message) {
        return *hint;
    }
    OTHER_MESSAGES
        .iter()
        .find(|(text, _)| message.contains(text))
        .map_or(Hint::None, |(_, hint)| *hint)
}

/// The button to show next to an error message, if one would help. Only fixes
/// that work today: Settings cards, and a retry that turns on the saved sign-in.
pub fn fix_for(message: &str, ctx: &FixContext) -> Option<Fix> {
    let download = ctx.module == "download";
    let open = |section| Some(Fix::OpenSettings { section });
    match hint_of(message) {
        Hint::None => None,
        Hint::SignIn if download && !ctx.signed_in && ctx.sign_in_ready => {
            Some(Fix::RetryWithSignIn)
        }
        Hint::SignIn => open(SettingsSection::SignIn),
        // The folder is a download setting; a conversion's drive isn't one.
        Hint::Folder if download => open(SettingsSection::Downloads),
        Hint::Folder => None,
        Hint::Encoder => open(SettingsSection::Encoding),
        Hint::Memory => open(SettingsSection::Performance),
        Hint::Tools => open(SettingsSection::Tools),
        // Only a download uses yt-dlp.
        Hint::Update if download => Some(Fix::UpdateAndRetry),
        Hint::Update => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail(line: &str) -> Vec<String> {
        vec!["frame=10".to_string(), line.to_string()]
    }

    #[test]
    fn finds_known_errors() {
        let tail = vec!["ERROR: [vimeo] 76979871: The web client only works when logged-in. Use --cookies, --cookies-from-browser, --username and --password, --netrc-cmd, or --netrc (vimeo) to provide account credentials.".to_string()];
        assert!(
            explain(Tool::YtDlp, Some(1), &tail, false)
                .starts_with("This site wants you signed in")
        );
        let tail = vec![
            "frame=10".to_string(),
            "av_interleaved_write_frame(): No space left on device".to_string(),
        ];
        assert!(explain(Tool::Ffmpeg, Some(1), &tail, false).starts_with("The drive is full"));
        assert_eq!(
            explain(Tool::Ffmpeg, Some(1), &[], false),
            "ffmpeg stopped with an error (code 1). The log has the details."
        );
        let tail = vec!["ERROR: [twitch:vod] v123456: Something new went wrong".to_string()];
        assert_eq!(
            explain(Tool::YtDlp, Some(1), &tail, false),
            "yt-dlp couldn't do it: Something new went wrong"
        );

        let exif =
            vec!["Error: Not a valid PNG (looks more like a JPEG) - C:\\x\\a.png".to_string()];
        assert_eq!(
            explain(Tool::Exiftool, Some(1), &exif, false),
            "exiftool couldn't do it: Not a valid PNG (looks more like a JPEG)"
        );

        // A bot check means "update yt-dlp", unless cookies were used.
        let bot =
            vec!["ERROR: [youtube] x: Sign in to confirm you\u{2019}re not a bot.".to_string()];
        assert!(explain(Tool::YtDlp, Some(1), &bot, false).starts_with("YouTube blocked"));
        assert!(
            explain(Tool::YtDlp, Some(1), &bot, true)
                .starts_with("The site didn't accept your sign-in")
        );
    }

    #[test]
    fn graphics_card_errors_are_not_memory_errors() {
        for line in [
            "[h264_nvenc @ 000001] OpenEncodeSessionEx failed: out of memory (10): ",
            "[h264_nvenc @ 000001] OpenEncodeSessionEx failed: unsupported device (2): ",
            "[h264_nvenc @ 000001] Cannot load nvcuda.dll",
            "[h264_nvenc @ 000001] No capable devices found",
            "[h264_amf @ 000001] DLL amfrt64.dll failed to open",
            "[h264_qsv @ 000001] Error initializing an MFX session: -9.",
            "Error while opening encoder for output stream #0:0 - maybe incorrect parameters such as bit_rate, rate, width or height",
        ] {
            let message = explain(Tool::Ffmpeg, Some(1), &tail(line), false);
            assert!(
                message.contains("graphics card") || message.contains("encoder"),
                "{line} -> {message}"
            );
            assert!(!message.contains("out of memory"), "{line} -> {message}");
            let ctx = FixContext {
                module: "premiere-safe",
                signed_in: false,
                sign_in_ready: false,
            };
            assert_eq!(
                fix_for(&message, &ctx),
                Some(Fix::OpenSettings {
                    section: SettingsSection::Encoding
                }),
                "{line}"
            );
        }
        // A real out-of-memory still points at Performance.
        let message = explain(
            Tool::Ffmpeg,
            Some(1),
            &tail("Cannot allocate memory"),
            false,
        );
        let ctx = FixContext {
            module: "compress",
            signed_in: false,
            sign_in_ready: false,
        };
        assert_eq!(
            fix_for(&message, &ctx),
            Some(Fix::OpenSettings {
                section: SettingsSection::Performance
            })
        );
    }

    #[test]
    fn site_errors_read_plainly() {
        let cases = [
            (
                "ERROR: [youtube] abc: This live event will begin in 14 minutes.",
                "live stream that hasn't started",
            ),
            (
                "ERROR: [youtube] abc: Premieres in 3 hours",
                "premiere that hasn't started",
            ),
            (
                "ERROR: [youtube] abc: The uploader has not made this video available in your country.",
                "isn't available where you are",
            ),
            (
                "ERROR: Unable to download JSON metadata: HTTP Error 404: Not Found",
                "that page doesn't exist",
            ),
            (
                "D:\\clips\\a.mkv: No such file or directory",
                "moved or deleted",
            ),
        ];
        for (line, expect) in cases {
            let message = explain(Tool::YtDlp, Some(1), &tail(line), false);
            assert!(message.contains(expect), "{line} -> {message}");
        }
    }

    #[test]
    fn an_old_downloader_gets_update_and_retry() {
        let ctx = |module| FixContext {
            module,
            signed_in: false,
            sign_in_ready: false,
        };
        for line in [
            "ERROR: [youtube] x: Sign in to confirm you're not a bot.",
            "ERROR: unable to download video data: HTTP Error 403: Forbidden",
            "ERROR: [generic] Unable to extract uploader id; please report this issue",
            "ERROR: [youtube] x: nsig extraction failed: Some formats may be missing",
        ] {
            let message = explain(Tool::YtDlp, Some(1), &tail(line), false);
            assert!(message.contains("Updating the downloader"), "{message}");
            assert_eq!(
                fix_for(&message, &ctx("download")),
                Some(Fix::UpdateAndRetry)
            );
            // Only a download uses yt-dlp.
            assert_eq!(fix_for(&message, &ctx("webp")), None);
        }
    }

    #[test]
    fn sign_in_fix_depends_on_setup() {
        let members = explain(
            Tool::YtDlp,
            Some(1),
            &tail("ERROR: [youtube] x: Join this channel to get access to members-only content"),
            false,
        );
        let ready = |signed_in, sign_in_ready| FixContext {
            module: "download",
            signed_in,
            sign_in_ready,
        };
        // Set up and not used yet: one click retries with it.
        assert_eq!(
            fix_for(&members, &ready(false, true)),
            Some(Fix::RetryWithSignIn)
        );
        // Not set up: go set it up.
        let setup = Some(Fix::OpenSettings {
            section: SettingsSection::SignIn,
        });
        assert_eq!(fix_for(&members, &ready(false, false)), setup);
        // Already used and still refused: the sign-in itself needs attention.
        assert_eq!(fix_for(&members, &ready(true, true)), setup);
        assert_eq!(fix_for(SIGN_IN_REJECTED, &ready(true, true)), setup);
        assert_eq!(
            fix_for(
                "Sign-in isn't set up. Choose your browser or a cookies.txt file in Settings.",
                &ready(false, false)
            ),
            setup
        );
    }

    #[test]
    fn folder_fixes_are_for_downloads_only() {
        let full = "The drive is full. Free up some space and retry.";
        let make = |module| FixContext {
            module,
            signed_in: false,
            sign_in_ready: false,
        };
        assert_eq!(
            fix_for(full, &make("download")),
            Some(Fix::OpenSettings {
                section: SettingsSection::Downloads
            })
        );
        assert_eq!(fix_for(full, &make("remux")), None);
        let low = "Only 400 MB is free on D:\\, where this job saves files. Free up some space, then retry.";
        assert!(fix_for(low, &make("download")).is_some());
        assert_eq!(fix_for(low, &make("proxy")), None);
        assert_eq!(fix_for("This video is private.", &make("download")), None);
        assert_eq!(
            fix_for(
                "ffmpeg is missing from the app folder. Reinstall the app to restore it.",
                &make("remux")
            ),
            Some(Fix::OpenSettings {
                section: SettingsSection::Tools
            })
        );
    }

    #[test]
    fn fix_serialises_for_the_ui() {
        let json = serde_json::to_value(Fix::OpenSettings {
            section: SettingsSection::SignIn,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "openSettings", "section": "signin"})
        );
        assert_eq!(
            serde_json::to_value(Fix::RetryWithSignIn).unwrap(),
            serde_json::json!({"kind": "retryWithSignIn"})
        );
    }

    #[test]
    fn a_failed_cut_reads_plainly() {
        let message = explain(
            Tool::YtDlp,
            Some(1),
            &tail("ERROR: ffmpeg exited with code 4294967274"),
            false,
        );
        assert!(
            message.contains("check that it fits inside the video"),
            "{message}"
        );
    }
}
