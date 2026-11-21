//! Turns tool output into progress. ffmpeg is run with `-progress pipe:1`, so
//! its stdout is a key=value stream; yt-dlp is run with [`YTDLP_TEMPLATE`] so
//! each progress tick is one easy-to-parse line.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Arguments that make yt-dlp print one parseable progress line per update.
pub const YTDLP_PROGRESS_ARGS: [&str; 5] = [
    "--newline",
    "--progress-template",
    YTDLP_TEMPLATE,
    "--color",
    "never",
];
/// The last two fields say which video of a playlist this is ("NA" for single videos).
pub const YTDLP_TEMPLATE: &str = "download:[progress] %(progress.downloaded_bytes)s/%(progress.total_bytes)s/%(progress.total_bytes_estimate)s/%(progress.speed)s/%(progress.eta)s/%(info.playlist_autonumber)s/%(info.n_entries)s";

/// Arguments that make ffmpeg report progress on stdout and keep stderr for the log.
pub const FFMPEG_PROGRESS_ARGS: [&str; 3] = ["-progress", "pipe:1", "-nostats"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProgressKind {
    /// ffmpeg with [`FFMPEG_PROGRESS_ARGS`]; the duration (seconds) turns time into a percentage.
    Ffmpeg { duration: Option<f64> },
    /// yt-dlp with [`YTDLP_PROGRESS_ARGS`]. `items`: how many videos a playlist
    /// download will take, when capped; otherwise yt-dlp's own count is used.
    /// `open_ended`: a date range, so the number of matching videos isn't known.
    YtDlp {
        items: Option<u32>,
        open_ended: bool,
    },
    /// TwitchDownloaderCLI's "[STATUS] - Downloading 25%" lines.
    TwitchChat,
    /// untrunc's "38.3%" redraws, each separated by a carriage return.
    Untrunc,
    /// No progress available; the job shows as indeterminate.
    None,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// 0.0..=1.0, when known.
    pub fraction: Option<f64>,
    pub eta_secs: Option<f64>,
    /// Human-readable, e.g. "3.2x" or "12.4 MB/s".
    pub speed: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Stdout,
    Stderr,
}

#[derive(Debug, PartialEq)]
pub enum Feed {
    Update(Progress),
    /// A progress line with nothing new to report.
    Consumed,
    /// Not progress; belongs in the job log.
    Log,
    /// A log line announcing a file the tool is about to write. The runner
    /// deletes it (and its .part) if the job fails, as long as it's new.
    Creating(PathBuf),
    /// yt-dlp found the file already in the folder and didn't download it again.
    Reused(PathBuf),
    /// The tool finished its way but says the result is only partly good. It becomes
    /// the job's warning if the job otherwise succeeds.
    Note(String),
}

pub enum Parser {
    Ffmpeg(FfmpegParser),
    YtDlp {
        items: Option<u32>,
        open_ended: bool,
    },
    TwitchChat,
    /// The last percentage reported, so a tool that redraws every tenth of a percent
    /// doesn't make the job report a thousand times.
    Untrunc(f64),
    None,
}

impl Parser {
    pub fn new(kind: &ProgressKind) -> Self {
        match kind {
            ProgressKind::Ffmpeg { duration } => Parser::Ffmpeg(FfmpegParser {
                duration: *duration,
                ..Default::default()
            }),
            ProgressKind::YtDlp { items, open_ended } => Parser::YtDlp {
                items: *items,
                open_ended: *open_ended,
            },
            ProgressKind::TwitchChat => Parser::TwitchChat,
            ProgressKind::Untrunc => Parser::Untrunc(-1.0),
            ProgressKind::None => Parser::None,
        }
    }

    pub fn feed(&mut self, source: Source, line: &str) -> Feed {
        match self {
            Parser::Ffmpeg(p) if source == Source::Stdout => p.feed(line),
            Parser::YtDlp { items, open_ended } => match parse_ytdlp(line, *items, *open_ended) {
                Some(p) => Feed::Update(p),
                None => ytdlp_reused(line).map_or_else(
                    || ytdlp_destination(line).map_or(Feed::Log, Feed::Creating),
                    Feed::Reused,
                ),
            },
            Parser::TwitchChat => parse_twitch_chat(line),
            Parser::Untrunc(last) => parse_untrunc(line, last),
            _ => Feed::Log,
        }
    }
}

/// untrunc prints "38.3%" while it reads, and "Error: unable to find correct codec ->
/// premature end (~38.32%)" when it can't go on matching the file to the example. It
/// still saves what it rebuilt and exits normally, so the second one is a warning.
fn parse_untrunc(line: &str, last: &mut f64) -> Feed {
    let line = line.trim();
    if let Some(pct) = line
        .strip_suffix('%')
        .and_then(|n| n.trim().parse::<f64>().ok())
    {
        let fraction = (pct / 100.0).clamp(0.0, 1.0);
        if (fraction - *last).abs() < 0.005 && fraction < 1.0 {
            return Feed::Consumed;
        }
        *last = fraction;
        return Feed::Update(Progress {
            fraction: Some(fraction),
            ..Default::default()
        });
    }
    if line.starts_with("Error:")
        && let Some((_, rest)) = line.split_once("premature end (~")
        && let Some(pct) = rest
            .split('%')
            .next()
            .and_then(|n| n.trim().parse::<f64>().ok())
    {
        return Feed::Note(format!(
            "Only the first {pct:.0}% of the recording could be rebuilt. The rest didn't match the working example. A second example from the same camera, or \"Skip over damaged sections\", may recover more."
        ));
    }
    Feed::Log
}

/// "[STATUS] - Downloading 25%" → 25%. Other status lines are just chatter.
fn parse_twitch_chat(line: &str) -> Feed {
    let Some(status) = line.trim().strip_prefix("[STATUS] - ") else {
        return Feed::Log;
    };
    match status
        .strip_prefix("Downloading ")
        .and_then(|p| p.trim_end_matches('%').trim().parse::<f64>().ok())
    {
        Some(pct) => Feed::Update(Progress {
            fraction: Some((pct / 100.0).clamp(0.0, 1.0)),
            ..Default::default()
        }),
        None => Feed::Consumed,
    }
}

/// "[download] D:\\Dl\\a.mp4 has already been downloaded" → the file.
fn ytdlp_reused(line: &str) -> Option<PathBuf> {
    let path = line
        .trim()
        .strip_prefix("[download] ")?
        .strip_suffix(" has already been downloaded")?;
    Some(PathBuf::from(path))
}

/// The paths yt-dlp announces before writing: downloads, merges and remuxes, and
/// the files saved beside a video (description, subtitles, thumbnail, info JSON),
/// so a failed job can take them away again.
fn ytdlp_destination(line: &str) -> Option<PathBuf> {
    let line = line.trim();
    // "Converting thumbnail "a.webp" to jpg" announces a.jpg.
    if let Some(rest) = line.strip_prefix("[ThumbnailsConvertor] Converting thumbnail \"") {
        let (from, format) = rest.split_once("\" to ")?;
        return Some(PathBuf::from(from).with_extension(format.trim()));
    }
    let path = if let Some(rest) = line
        .strip_prefix("[download] Destination: ")
        .or_else(|| line.strip_prefix("[ExtractAudio] Destination: "))
    {
        rest
    } else if let Some(rest) = ["description", "subtitles", "metadata as JSON"]
        .iter()
        .find_map(|what| line.strip_prefix(&format!("[info] Writing video {what} to: ")))
    {
        rest
    } else if line.starts_with("[info] Writing video thumbnail") {
        line.split_once(" to: ")?.1
    } else if let Some(rest) = line.strip_prefix("[Merger] Merging formats into ") {
        rest.trim_matches('"')
    } else if line.starts_with("[VideoRemuxer]") || line.starts_with("[VideoConvertor]") {
        line.split_once("; Destination: ")?.1
    } else {
        return None;
    };
    Some(PathBuf::from(path))
}

#[derive(Default)]
pub struct FfmpegParser {
    duration: Option<f64>,
    out_time: Option<f64>,
    speed: Option<f64>,
}

impl FfmpegParser {
    fn feed(&mut self, line: &str) -> Feed {
        let Some((key, value)) = line.split_once('=') else {
            return Feed::Log;
        };
        match key.trim() {
            "out_time_us" => {
                self.out_time = value.trim().parse::<f64>().ok().map(|us| us / 1_000_000.0)
            }
            "speed" => self.speed = value.trim().trim_end_matches('x').parse().ok(),
            "progress" => return Feed::Update(self.snapshot(value.trim() == "end")),
            _ => {}
        }
        Feed::Consumed
    }

    fn snapshot(&self, finished: bool) -> Progress {
        let fraction = match (self.out_time, self.duration) {
            _ if finished => Some(1.0),
            (Some(t), Some(d)) if d > 0.0 => Some((t / d).clamp(0.0, 1.0)),
            _ => None,
        };
        let eta_secs = match (self.out_time, self.duration, self.speed) {
            (Some(t), Some(d), Some(s)) if s > 0.0 && !finished => Some(((d - t) / s).max(0.0)),
            _ => None,
        };
        Progress {
            fraction,
            eta_secs,
            speed: self.speed.filter(|s| *s > 0.0).map(|s| format!("{s:.1}x")),
        }
    }
}

/// One progress line. In a playlist the bar covers the whole batch ("video 3 of
/// 10" at 40% is 24% overall), and the ETA is the current video's.
fn parse_ytdlp(line: &str, items: Option<u32>, open_ended: bool) -> Option<Progress> {
    let rest = line.trim().strip_prefix("[progress] ")?;
    let mut parts = rest.split('/');
    let num = |s: Option<&str>| s.and_then(|v| v.trim().parse::<f64>().ok());
    let downloaded = num(parts.next());
    let total = num(parts.next());
    let estimate = num(parts.next());
    let speed = num(parts.next());
    let eta = num(parts.next());
    let index = num(parts.next());
    let entries = num(parts.next());

    let mut fraction = match (downloaded, total.or(estimate)) {
        (Some(d), Some(t)) if t > 0.0 => Some((d / t).clamp(0.0, 1.0)),
        _ => None,
    };
    let mut speed = speed.map(format_rate);
    // A date range: how many videos match isn't known, so say which one this is
    // and leave the bar open rather than count the whole channel.
    if open_ended && let Some(i) = index {
        let of = format!("video {i}");
        return Some(Progress {
            fraction: None,
            eta_secs: eta,
            speed: Some(speed.map_or(of.clone(), |s| format!("{of} · {s}"))),
        });
    }
    let count = items.map(f64::from).or(entries).filter(|n| *n > 1.0);
    if let (Some(i), Some(n)) = (index, count) {
        let i = i.min(n);
        fraction = Some(((i - 1.0 + fraction.unwrap_or(0.0)) / n).clamp(0.0, 1.0));
        let of = format!("video {i} of {n}");
        speed = Some(speed.map_or(of.clone(), |s| format!("{of} · {s}")));
    }
    Some(Progress {
        fraction,
        eta_secs: eta,
        speed,
    })
}

fn format_rate(bytes_per_sec: f64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes_per_sec >= MB {
        format!("{:.1} MB/s", bytes_per_sec / MB)
    } else {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_progress() {
        let mut p = Parser::new(&ProgressKind::Ffmpeg {
            duration: Some(100.0),
        });
        assert_eq!(p.feed(Source::Stdout, "frame=120"), Feed::Consumed);
        p.feed(Source::Stdout, "out_time_us=25000000");
        p.feed(Source::Stdout, "speed=2.5x");
        let Feed::Update(u) = p.feed(Source::Stdout, "progress=continue") else {
            panic!("expected update")
        };
        assert_eq!(u.fraction, Some(0.25));
        assert_eq!(u.eta_secs, Some(30.0));
        assert_eq!(u.speed.as_deref(), Some("2.5x"));

        let Feed::Update(done) = p.feed(Source::Stdout, "progress=end") else {
            panic!("expected update")
        };
        assert_eq!(done.fraction, Some(1.0));
        // stderr is always log, even if it looks like key=value.
        assert_eq!(
            p.feed(Source::Stderr, "Stream #0:0 -> #0:0 (copy)"),
            Feed::Log
        );
    }

    #[test]
    fn ffmpeg_na_values() {
        let mut p = Parser::new(&ProgressKind::Ffmpeg { duration: None });
        p.feed(Source::Stdout, "out_time_us=N/A");
        p.feed(Source::Stdout, "speed=N/A");
        let Feed::Update(u) = p.feed(Source::Stdout, "progress=continue") else {
            panic!("expected update")
        };
        assert_eq!(u, Progress::default());
    }

    #[test]
    fn ytdlp_progress() {
        let mut p = Parser::new(&ProgressKind::YtDlp {
            items: None,
            open_ended: false,
        });
        let Feed::Update(u) = p.feed(
            Source::Stdout,
            "[progress] 50000000/NA/200000000/10485760/15/NA/NA",
        ) else {
            panic!("expected update")
        };
        assert_eq!(u.fraction, Some(0.25));
        assert_eq!(u.eta_secs, Some(15.0));
        assert_eq!(u.speed.as_deref(), Some("10.0 MB/s"));
        assert_eq!(
            p.feed(Source::Stdout, "[youtube] abc: Downloading webpage"),
            Feed::Log
        );
    }

    #[test]
    fn ytdlp_playlist_progress() {
        // Video 3 of a 10-item batch, half done: 25% overall.
        let mut p = Parser::new(&ProgressKind::YtDlp {
            items: Some(10),
            open_ended: false,
        });
        let Feed::Update(u) = p.feed(Source::Stdout, "[progress] 50/100/NA/2097152/4/3/487") else {
            panic!("expected update")
        };
        assert_eq!(u.fraction, Some(0.25));
        assert_eq!(u.speed.as_deref(), Some("video 3 of 10 · 2.0 MB/s"));
    }

    #[test]
    fn twitch_chat_progress() {
        let mut p = Parser::new(&ProgressKind::TwitchChat);
        let Feed::Update(u) = p.feed(Source::Stdout, "[STATUS] - Downloading 25%") else {
            panic!("expected update")
        };
        assert_eq!(u.fraction, Some(0.25));
        assert_eq!(
            p.feed(Source::Stdout, "[STATUS] - Writing Output File"),
            Feed::Consumed
        );
        assert_eq!(
            p.feed(Source::Stdout, "TwitchDownloaderCLI 1.56.5"),
            Feed::Log
        );
    }

    #[test]
    fn ytdlp_destinations() {
        let mut p = Parser::new(&ProgressKind::YtDlp {
            items: None,
            open_ended: false,
        });
        assert_eq!(
            p.feed(
                Source::Stdout,
                r"[download] Destination: D:\Dl\a b.f137.mp4"
            ),
            Feed::Creating(PathBuf::from(r"D:\Dl\a b.f137.mp4"))
        );
        assert_eq!(
            p.feed(
                Source::Stdout,
                r#"[Merger] Merging formats into "D:\Dl\a b.mp4""#
            ),
            Feed::Creating(PathBuf::from(r"D:\Dl\a b.mp4"))
        );
        assert_eq!(
            p.feed(
                Source::Stdout,
                r"[VideoRemuxer] Remuxing video from webm to mp4; Destination: D:\Dl\a.mp4"
            ),
            Feed::Creating(PathBuf::from(r"D:\Dl\a.mp4"))
        );
        assert_eq!(
            p.feed(Source::Stdout, r"[ExtractAudio] Destination: D:\Dl\a.mp3"),
            Feed::Creating(PathBuf::from(r"D:\Dl\a.mp3"))
        );
        assert_eq!(
            p.feed(
                Source::Stdout,
                r"[download] D:\Dl\a.mp4 has already been downloaded"
            ),
            Feed::Reused(PathBuf::from(r"D:\Dl\a.mp4"))
        );
    }

    #[test]
    fn ytdlp_side_files_and_reused_files() {
        let mut p = Parser::new(&ProgressKind::YtDlp {
            items: None,
            open_ended: false,
        });
        for (line, path) in [
            (
                r"[info] Writing video description to: D:\Dl\a.description",
                r"D:\Dl\a.description",
            ),
            (
                r"[info] Writing video subtitles to: D:\Dl\a.en.vtt",
                r"D:\Dl\a.en.vtt",
            ),
            (
                r"[info] Writing video metadata as JSON to: D:\Dl\a.info.json",
                r"D:\Dl\a.info.json",
            ),
            (
                r"[info] Writing video thumbnail 0 to: D:\Dl\a.webp",
                r"D:\Dl\a.webp",
            ),
            (
                r#"[ThumbnailsConvertor] Converting thumbnail "D:\Dl\a.webp" to jpg"#,
                r"D:\Dl\a.jpg",
            ),
        ] {
            assert_eq!(
                p.feed(Source::Stdout, line),
                Feed::Creating(PathBuf::from(path)),
                "{line}"
            );
        }
        // The list of finished files yt-dlp prints is a temp file of ours, not a side file.
        assert_eq!(
            p.feed(
                Source::Stdout,
                r"[info] Writing '%(filepath)s' to: C:\Temp\outputs-1.txt"
            ),
            Feed::Log
        );
        assert_eq!(
            p.feed(
                Source::Stdout,
                r"[download] D:\Dl\My Video [60-90].mp4 has already been downloaded"
            ),
            Feed::Reused(PathBuf::from(r"D:\Dl\My Video [60-90].mp4"))
        );
    }

    #[test]
    fn untrunc_percentages_are_thinned_and_a_cut_short_result_is_noted() {
        let mut p = Parser::new(&ProgressKind::Untrunc);
        let pct = |p: &mut Parser, line: &str| match p.feed(Source::Stdout, line) {
            Feed::Update(u) => Some(u.fraction.unwrap()),
            Feed::Consumed => None,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(pct(&mut p, "0%  "), Some(0.0));
        // Redraws every tenth of a percent are not each reported.
        assert!((pct(&mut p, "38.3%  ").unwrap() - 0.383).abs() < 1e-9);
        assert_eq!(pct(&mut p, "38.4%  "), None);
        assert_eq!(pct(&mut p, "39%  "), Some(0.39));
        assert_eq!(pct(&mut p, "100%"), Some(1.0));

        let Feed::Note(note) = p.feed(
            Source::Stdout,
            "Error: unable to find correct codec -> premature end (~38.32%)",
        ) else {
            panic!("expected a note")
        };
        assert!(note.starts_with("Only the first 38% of the recording"));
        assert_eq!(
            p.feed(Source::Stdout, "Info: saving a.mp4"),
            Feed::Log
        );
    }

    #[test]
    fn date_range_progress_has_no_total() {
        let mut p = Parser::new(&ProgressKind::YtDlp {
            items: None,
            open_ended: true,
        });
        let Feed::Update(u) = p.feed(Source::Stdout, "[progress] 500/1000/NA/2097152/3/2/35")
        else {
            panic!("expected an update")
        };
        assert_eq!(u.fraction, None);
        assert_eq!(u.speed.as_deref(), Some("video 2 · 2.0 MB/s"));
    }
}
