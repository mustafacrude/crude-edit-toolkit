# Crude's Edit Toolkit

Everything that comes before editing in Premiere, in one small desktop app: download from a link, make any clip import cleanly, cut the parts you want, and clean up images. Drop a file or paste a link and it tells you what to do next.

> **Status: pre-release (0.1.0).** It runs and is tested on Windows. Installers are built by GitHub Actions and published as releases (see [Install](#install)). A Mac build is set up but not ready or tested yet; until then, build from source.

Only use it on content you own or have permission to use.

## What it does

- **Download** from YouTube, Twitch, SoundCloud and most other sites, as H.264 + AAC by default so Premiere opens it as-is, or just the sound. Music sites keep their original audio with tags and cover art, or give you an MP3 or a WAV. Cut just the parts you want while downloading, save thumbnails, subtitles and chapters, and use your sign-in for members-only or age-restricted videos.
- **Playlists and channels** with a date range or "newest N", a folder per playlist, and a memory of what it already saved, so running a channel again only gets what's new.
- **Chat replays** as JSON: Twitch VODs and clips (trimmed to the section you download) and YouTube live-stream chat.
- **Make footage Premiere-safe**: H.264 at a constant frame rate with AAC audio, every audio track kept. Plus remux, constant-frame-rate fix, a full re-encode panel, proxies (ProRes Proxy or DNxHR LB) and compress-to-a-size (Discord's 10 MB and friends).
- **Repair a cut-off recording** (Windows): rebuild an MP4 or MOV that was never finished, using a working video from the same camera, with [untrunc](https://github.com/anthwlock/untrunc), which is downloaded on request and kept up to date from Settings.
- **Cut and join**: clip a list of time ranges, trim and merge clips (lossless when they match), grab frames, extract audio (one WAV per track).
- **Images**: save pictures from a link, a Google Images result, a page or the clipboard, at full size; turn WebP, AVIF, HEIC, SVG and JPEG XL into PNGs Premiere imports; strip location, camera and Content Credentials (C2PA) without recompressing; and convert to WebP, with transparency and colour profiles handled properly.
- **Inspect on drop**: it reads each file and says in plain words what Premiere will struggle with, then recommends the fix.
- **Two modes**: *Simple* gives three guided screens (download a video, fix a video for editing, images) with sensible defaults, *Pro* gives every setting. A short first-run setup checks your download folder, graphics card and tools, and a failed job says what went wrong in plain words with a button that helps.

It never edits your originals, never overwrites a file, and runs nothing at startup. No account, no tracking.

Read the **[handbook](docs/handbook.md)** for how everything works, what each tool does, and recipes for common jobs. It is also inside the app: the book icon in the top bar.

## Built to be safe to leave running

Every tool runs at low priority inside one limited group: combined memory is capped, CPU can be capped from Settings, jobs refuse to start (or stop and clean up) when the drive is nearly full, hung tools are stopped, link lookups and file checks are throttled, playlist downloads pace themselves so sites don't block you, and if the app dies every tool dies with it. The [handbook](docs/handbook.md#9-safety-and-limits) lists every limit and, honestly, what isn't limited (the graphics card).

## Install

Download the installer from the **[Releases page](https://github.com/mustafacrude/crude-edit-toolkit/releases/latest)**.

**Windows.** Run `Crude’s Edit Toolkit_x.y.z_x64-setup.exe`. It installs for your user only (no admin password), adds Start menu and desktop shortcuts and an entry in **Settings → Apps → Installed apps**, and never starts with Windows. It also adds **Send to Crude's Toolkit** to the right-click menu of video, image and audio files and folders (on Windows 11, under *Show more options*).

The installer isn't signed with a paid certificate, so Windows SmartScreen may say "Windows protected your PC" the first time. Click **More info**, then **Run anyway**. Some antivirus programs also flag `yt-dlp.exe`, which comes with the app, because it downloads from the web; it's the real, unmodified program from its [official releases](https://github.com/yt-dlp/yt-dlp/releases).

**Uninstall.** Settings → Apps → Installed apps → Crude's Edit Toolkit → Uninstall. Tick *Delete the application data* to also remove settings and history. Your downloads and processed files are never touched.

**Mac (Apple Silicon).** Built by the same release, but not tested on a Mac yet, so treat it as a preview. Open the `.dmg`, drag the app to Applications, and open it. macOS will say it can't verify the app. Don't use right-click → Open (that no longer works). Instead open **System Settings → Privacy & Security**, scroll down to the message about the app, and click **Open Anyway**, then confirm.

**Updates.** The app looks for a newer version, and a newer yt-dlp (the part that breaks when sites change), when it opens, at most once a day. Nothing installs until you click **Update available**, or **Update and retry** on a failed download. You can turn the check off in Settings.

## Build and run from source (Windows)

You need [Node](https://nodejs.org), [Rust](https://rustup.rs) and the Visual Studio C++ Build Tools.

```
npm install
npm run tools        # downloads the pinned yt-dlp, Deno, ffmpeg, exiftool and TwitchDownloaderCLI, and checks their hashes
npm run tauri dev    # opens the app with hot reload
```

The tool versions live in [tools/manifest.json](tools/manifest.json). To bump one, change its `url` and `sha256` together and run `npm run tools` again. The downloaded binaries are never committed.

Checks:

```
cd src-tauri && cargo test && cargo clippy --all-targets
npx tsc --noEmit
```

Build the Windows installer yourself (needs the updater signing key; see [docs/releasing.md](docs/releasing.md)):

```
npm run tauri build -- --bundles nsis
```

## Project layout

| Path | What's there |
|---|---|
| `src/` | The React UI (shadcn/ui, Tailwind v4, dark only) |
| `src-tauri/src/modules/` | One file per tool (download, remux, clip, merge, compress, proxies, images…) |
| `src-tauri/src/jobs.rs` | The queue: lanes, progress, cancel, logs, partial results, limits |
| `src-tauri/src/guard.rs` | Resource limits: priority, memory and CPU caps, disk space, keep-awake |
| `src-tauri/src/tools.rs` | Finds, starts and version-checks the bundled tools |
| `src-tauri/src/fetch_image.rs` | Saving pictures from links and the clipboard, converted for Premiere |
| `src-tauri/src/tool_update.rs`, `updates.rs` | Updating yt-dlp (checksummed) and the app itself |
| `src-tauri/installer/hooks.nsh` | The Windows installer's extra steps (the right-click menu entry) |
| `.github/workflows/` | CI checks and the release build |
| `tools/manifest.json` | Pinned tool versions and hashes |
| `scripts/fetch-tools.mjs` | Downloads and verifies the tools |
| `docs/handbook.md` | How it all works, with use cases |
| `docs/modules.md` | How to add a tool |
| `BRIEF.md` | The design brief and the decisions behind it |

Adding a feature is a new file in `modules/` and one line in its list: the queue, history, Home suggestions and settings forms come for free. See [docs/modules.md](docs/modules.md).

## Bundled tools

yt-dlp, Deno, FFmpeg (a GPL build), ExifTool and TwitchDownloaderCLI come with the app under their own licences. untrunc (GPL-2.0-or-later) is not bundled: the app downloads its author's release only when you ask for Repair damaged video. See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

## Not built yet

A Mac test pass (the Mac build is set up and pinned but has never run on a real Mac; SVG pictures can't be converted there yet). Planned for later: Twitch chat rendered as a video overlay and per-project output folders.

## Licence

The app's own code is under the [MIT licence](LICENSE). The bundled programs keep their own licences ([THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)); FFmpeg is a GPL build, so its source is attached to every release. The full licence texts ship inside the app (Settings, About, Open-source licences).
