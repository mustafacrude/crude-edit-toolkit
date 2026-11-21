//! The bundled command-line tools: where they live and what version they are.
//!
//! Single-file tools ship as Tauri sidecars, which end up next to the app's
//! executable (Tauri strips the target-triple suffix). exiftool is a folder of
//! files, so it ships as a resource directory instead.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    YtDlp,
    Deno,
    Ffmpeg,
    Ffprobe,
    Exiftool,
    TwitchDownloader,
    /// Not bundled: downloaded on request (see untrunc_update.rs), Windows only.
    Untrunc,
}

impl Tool {
    pub const ALL: [Tool; 6] = [
        Tool::YtDlp,
        Tool::Deno,
        Tool::Ffmpeg,
        Tool::Ffprobe,
        Tool::Exiftool,
        Tool::TwitchDownloader,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Deno => "Deno",
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
            Tool::Exiftool => "exiftool",
            Tool::TwitchDownloader => "TwitchDownloaderCLI",
            Tool::Untrunc => "untrunc",
        }
    }

    pub fn purpose(self) -> &'static str {
        match self {
            Tool::YtDlp => "Downloads videos, subtitles and chat",
            Tool::Deno => "Lets yt-dlp handle YouTube's player code",
            Tool::Ffmpeg => "Converts, remuxes and encodes video",
            Tool::Ffprobe => "Inspects video files",
            Tool::Exiftool => "Removes metadata and C2PA from images",
            Tool::TwitchDownloader => "Downloads Twitch VOD chat",
            Tool::Untrunc => "Repairs cut-off MP4 and MOV recordings",
        }
    }

    /// Sidecar file name without the platform suffix, or None for resource-dir tools.
    fn sidecar(self) -> Option<&'static str> {
        match self {
            Tool::YtDlp => Some("yt-dlp"),
            Tool::Deno => Some("deno"),
            Tool::Ffmpeg => Some("ffmpeg"),
            Tool::Ffprobe => Some("ffprobe"),
            Tool::TwitchDownloader => Some("TwitchDownloaderCLI"),
            Tool::Exiftool | Tool::Untrunc => None,
        }
    }

    fn version_args(self) -> &'static [&'static str] {
        match self {
            Tool::YtDlp | Tool::Deno | Tool::TwitchDownloader => &["--version"],
            Tool::Ffmpeg | Tool::Ffprobe => &["-hide_banner", "-version"],
            Tool::Exiftool => &["-ver"],
            Tool::Untrunc => &["-V"],
        }
    }

    /// Pulls the bare version number out of the tool's version output.
    fn parse_version(self, stdout: &str) -> Option<String> {
        let first = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
        if self == Tool::Untrunc {
            // "version 'v1-87a5d92-dirty' using ffmpeg '3.3.4' ...": the build is its commit.
            return crate::untrunc_update::commit_of(first);
        }
        let version = match self {
            Tool::YtDlp | Tool::Exiftool => first,
            // "deno 2.9.8 (stable, ...)" / "TwitchDownloaderCLI 1.56.5+f8335ca..."
            Tool::Deno | Tool::TwitchDownloader => first.split_whitespace().nth(1)?,
            Tool::Untrunc => unreachable!("returned above"),
            // "ffmpeg version n9.0.2-35-g9a9b70bb8c-20261024 Copyright ..."
            Tool::Ffmpeg | Tool::Ffprobe => {
                let raw = first.split_whitespace().nth(2)?;
                raw.trim_start_matches('n').split('-').next()?
            }
        };
        Some(version.split('+').next().unwrap_or(version).to_string())
    }

    pub fn path(self, app: &AppHandle) -> Result<PathBuf, String> {
        // A newer yt-dlp downloaded from inside the app (see tool_update.rs).
        if self == Tool::YtDlp
            && let Some(newer) = crate::tool_update::override_path(app)
        {
            return Ok(newer);
        }
        if self == Tool::Untrunc {
            return Ok(crate::untrunc_update::installed_path(app)
                .unwrap_or_else(|| crate::untrunc_update::expected_path(app)));
        }
        let exe_suffix = std::env::consts::EXE_SUFFIX;
        match self.sidecar() {
            Some(name) => {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let dir = exe.parent().ok_or("App folder not found")?;
                Ok(dir.join(format!("{name}{exe_suffix}")))
            }
            None => {
                let res = app.path().resource_dir().map_err(|e| e.to_string())?;
                Ok(res.join("exiftool").join(format!("exiftool{exe_suffix}")))
            }
        }
    }
}

/// Windows process flags: no console window, below-normal priority so the
/// computer stays responsive while a tool works flat out.
#[cfg(windows)]
const CREATION_FLAGS: u32 = 0x0800_0000 /* CREATE_NO_WINDOW */ | 0x0000_4000 /* BELOW_NORMAL */;

/// How much a quick tool run (a probe, a version check) may print. More is cut off.
const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;

/// Builds a Command for a quick tool run; see [`run`].
pub fn command(path: &Path) -> Command {
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATION_FLAGS);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        // SAFETY: nice() is async-signal-safe and touches nothing else.
        unsafe {
            cmd.pre_exec(|| {
                libc::nice(10);
                Ok(())
            });
        }
    }
    cmd
}

/// Runs a quick tool to the end and collects what it printed, under the app's
/// limits. A run longer than `limit` (a damaged file, a network drive that
/// stopped answering) is killed and reported as timed out.
pub fn run(cmd: &mut Command, limit: Duration) -> std::io::Result<Output> {
    use std::io::Read;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    crate::guard::adopt_std(&child);
    // Read both pipes on their own threads so a chatty tool can't block on a full pipe.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(pipe) = pipe {
                let _ = pipe.take(OUTPUT_LIMIT).read_to_end(&mut buf);
            }
            buf
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as _));

    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            kill_tree_blocking(&mut child);
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("took longer than {} seconds", limit.as_secs()),
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// The async version for long-running tools: no console window, low priority,
/// output piped, and (on macOS) its own process group so [`kill_tree`] can
/// stop all of it. Start it with [`spawn`].
pub fn async_command(path: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATION_FLAGS);
    #[cfg(unix)]
    {
        cmd.process_group(0);
        // SAFETY: nice() is async-signal-safe and touches nothing else.
        unsafe {
            cmd.pre_exec(|| {
                libc::nice(10);
                Ok(())
            });
        }
    }
    cmd
}

/// Starts a long-running tool under the app's limits (see guard.rs).
pub fn spawn(cmd: &mut tokio::process::Command) -> std::io::Result<tokio::process::Child> {
    let child = cmd.spawn()?;
    crate::guard::adopt(&child);
    Ok(child)
}

fn kill_tree_blocking(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Stops a tool and everything it started. yt-dlp.exe is a launcher that runs the
/// real yt-dlp as a child, which runs ffmpeg and Deno in turn; killing only the
/// launcher leaves those running with the download's files still open.
pub async fn kill_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        #[cfg(windows)]
        {
            let mut taskkill = tokio::process::Command::new("taskkill");
            taskkill
                .args(["/T", "/F", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000);
            let _ = taskkill.status().await;
        }
        #[cfg(unix)]
        {
            // The tool runs in its own process group; -pid kills the whole group.
            let _ = tokio::process::Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .status()
                .await;
        }
    }
    let _ = child.kill().await;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: Tool,
    pub name: &'static str,
    pub purpose: &'static str,
    pub version: Option<String>,
    pub path: String,
    /// The file isn't there at all, as opposed to being there but failing.
    pub missing: bool,
    pub error: Option<String>,
}

pub fn inspect(app: &AppHandle, tool: Tool) -> ToolInfo {
    let mut info = ToolInfo {
        id: tool,
        name: tool.name(),
        purpose: tool.purpose(),
        version: None,
        path: String::new(),
        missing: false,
        error: None,
    };

    let path = match tool.path(app) {
        Ok(p) => p,
        Err(e) => {
            info.error = Some(e);
            return info;
        }
    };
    info.path = path.display().to_string();

    if !path.exists() {
        info.missing = true;
        info.error = Some(if tool == Tool::Untrunc {
            "Not downloaded yet. Video tools > Repair damaged video has the button.".into()
        } else {
            "Missing from the app folder. Reinstall the app to restore it.".into()
        });
        return info;
    }

    match run(
        command(&path).args(tool.version_args()),
        Duration::from_secs(30),
    ) {
        Ok(out) => {
            // TwitchDownloaderCLI prints its version to stderr; the rest use stdout.
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            info.version = tool
                .parse_version(&stdout)
                .or_else(|| tool.parse_version(&stderr));
            if info.version.is_none() {
                info.error = Some("Ran, but didn't report a version.".into());
            }
        }
        Err(e) => info.error = Some(format!("Couldn't start it: {e}")),
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(
            Tool::YtDlp.parse_version("2026.10.21\n").as_deref(),
            Some("2026.10.21")
        );
        assert_eq!(
            Tool::Deno
                .parse_version("deno 2.9.8 (stable, release, x86_64-pc-windows-msvc)\nv8 14")
                .as_deref(),
            Some("2.9.8")
        );
        assert_eq!(
            Tool::Ffmpeg
                .parse_version(
                    "ffmpeg version n9.0.2-35-g9a9b70bb8c-20261024 Copyright (c) 2000-2026"
                )
                .as_deref(),
            Some("9.0.2")
        );
        assert_eq!(
            Tool::TwitchDownloader
                .parse_version(
                    "TwitchDownloaderCLI 1.56.5+f8335cabef7436c362b13703359d440a26065bc4\n"
                )
                .as_deref(),
            Some("1.56.5")
        );
        assert_eq!(
            Tool::Exiftool.parse_version("13.59\r\n").as_deref(),
            Some("13.59")
        );
        assert_eq!(
            Tool::Untrunc
                .parse_version("version 'v1-87a5d92-dirty' using ffmpeg '3.3.4' Lavc57.89.100\r\n")
                .as_deref(),
            Some("87a5d92")
        );
        assert_eq!(Tool::YtDlp.parse_version("\n\n"), None);
    }

    #[cfg(windows)]
    #[test]
    fn quick_runs_time_out() {
        let started = Instant::now();
        let err = run(
            Command::new("ping").args(["-n", "30", "127.0.0.1"]),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10));

        let out = run(
            Command::new("cmd").args(["/C", "echo hello"]),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
    }
}
