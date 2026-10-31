# Crude's Edit Toolkit — Build Brief v2

> **Status:** decisions locked on 2026-10-24 (design review rounds 1 and 2). This replaces `crudes-edit-toolkit-brief.md`. The design previews live at https://claude.ai/artifact/LaxqjLj5HAsrV5tJ7HNcDA and are the visual reference.
>
> **For the coding agent:** Mustafa is newer to coding with AI. Explain trade-offs in plain language and confirm before anything hard to undo (installing system-wide software, creating repos, generating signing keys). Anything marked **VERIFY** must be checked against current docs or releases before you rely on it.

---

## 1. What we're building

A personal desktop app that puts **yt-dlp, ffmpeg/ffprobe, exiftool and TwitchDownloaderCLI** behind one drag-and-drop window, for everything that happens before editing in Adobe Premiere Pro: downloading, fixing and converting footage and images.

It's for Mustafa first, and for **a few non-technical friends** second. Friends will have different hardware, won't read docs, and need the app to tell them what's wrong with a file and what to do about it.

**Goals**
- **One place for pre-edit jobs:** download, remux, re-encode, CFR, clip, strip metadata/C2PA, convert images.
- **Premiere-safe by default:** every output drops straight onto a Premiere timeline (CFR, H.264, AAC 48 kHz, all audio tracks kept).
- **Fast:** hardware encoding wherever the machine has it, with a clean software fallback.
- **Friend-proof:** a Simple mode, plain-language errors, a first-run setup, and an app that updates itself.
- **Lightweight, local and secure:** no account, no server, no telemetry.
- **Built to grow:** every tool is a self-contained module so v2/v3 features plug in without rebuilding screens (Section 5).

**Out of scope:** auto-reframe/9:16 crop, chat-spike/highlight finder, silence detection, audio cleanup/denoise, Twitch muted-segment check, CLI mode.

---

## 2. Target platforms

| Platform | Arch | Notes |
|---|---|---|
| Windows 10/11 | x64 | Mustafa's RTX 5080 editing PC. Friends may have AMD, Intel or older NVIDIA GPUs. |
| macOS | Apple Silicon (arm64) | Primary Mac target. Intel Macs: best effort (software encode or VideoToolbox on Intel), not a release blocker. |

Linux is not required.

---

## 3. Tech stack

- **Tauri 2** (Rust backend + web UI).
- **Frontend: React + TypeScript + Vite, shadcn/ui on Tailwind v4.** (Changed from Svelte so we get the official shadcn, the biggest component ecosystem and the best AI coding help.)
- **Backend: Rust.** Owns the job queue, tool modules, presets, argument building, process spawning, progress parsing, and tool updates.
- **App self-update:** Tauri updater plugin. Update manifests and installers are served from the **Releases page of the project's one public GitHub repo** (decided later: one repo, not a separate releases repo). The updater signing keypair is generated once; the private key lives only in GitHub Actions secrets and Mustafa's password manager.
- **Bundled tools as Tauri sidecars**, one binary per target triple:

| Tool | Used for | Update strategy |
|---|---|---|
| yt-dlp (standalone binary) | Downloads, sections, playlists, live-chat JSON, thumbnails/subs/chapters/info | In-app update (Section 11.4). Breaks often when sites change. |
| Deno | JS runtime yt-dlp needs for full YouTube support. **VERIFY** current requirement and how yt-dlp finds it (PATH vs flag). | Updated alongside yt-dlp |
| ffmpeg + ffprobe | Remux, encode, CFR, proxies, clip, trim/merge, frame grab, WebP, inspect | Pinned; ships with app releases |
| exiftool | Lossless metadata/C2PA strip on images | Pinned |
| TwitchDownloaderCLI | Twitch VOD chat as JSON | Pinned |

**Updated tools never go inside the app bundle.** Bundled sidecars are read-only. An updated yt-dlp/Deno is downloaded to the app data folder, checksum-verified, and used if it's newer than the bundled one. (Writing into the bundle breaks the macOS signature and fails on Windows installs in Program Files.)

**VERIFY ffmpeg builds:** Windows needs NVENC (`h264_nvenc`, `hevc_nvenc`, `av1_nvenc`), AMF (`h264_amf`), QSV (`h264_qsv`), libwebp, ProRes encoder. macOS arm64 needs VideoToolbox (`h264_videotoolbox`, `hevc_videotoolbox`, `prores_videotoolbox`) and libwebp. The NVENC build will be GPL; record each binary's license and source link in `THIRD_PARTY_LICENSES.md`.

**VERIFY exiftool packaging:** Windows standalone exe; macOS relies on system Perl — confirm still present on current macOS.

---

## 4. Architecture

```
React UI (shadcn/ui)
   │  Tauri commands only — no shell access from the UI
   ▼
Rust core
   • Tool module registry (Section 5)
   • Job queue: downloads in parallel (default 3), encodes one at a time
   • Presets (built-in + user-saved, JSON)
   • Argument builder (Vec<String>, never shell strings)
   • Progress parser (yt-dlp / ffmpeg → % / ETA / speed)
   • Inspector (ffprobe → typed MediaInfo, VFR + C2PA detection)
   • Encoder detection + GPU test (cached)
   • Tool manager (versions, updates, SHA-256 verify, rollback)
   • Naming templates + job history (SQLite)
   │
   ├── yt-dlp (+ Deno) ── ffmpeg/ffprobe ── exiftool ── TwitchDownloaderCLI
   ▼
Output folders on disk
```

---

## 5. Tool modules (the extensibility contract)

Every tool — Remux, Re-encode, CFR, Proxies, Clip, Trim & merge, Strip, Frame grab, Audio extract, Compress to size, WebP convert, Download, Chat — is one module implementing the same Rust trait, roughly:

- `id`, `name`, `category` (download / video / image)
- `options_schema` — the settings it takes (drives the settings form in the UI)
- `suggest(&MediaInfo) -> Option<Suggestion>` — whether it fixes something about a dropped file, with a plain-language reason and a priority
- `build_jobs(inputs, options) -> Vec<JobSpec>` — argument vectors per tool invocation
- `outputs(...)` — the file paths it will create

The UI renders tool tabs, the drop-first suggestions and Simple-mode flows **from the registry**, so a v2 module (e.g. whisper captions) appears without new screens. Keep the module trait stable and documented in `docs/modules.md`.

---

## 6. Downloading

- **Sources:** YouTube, Twitch VODs and clips, anything yt-dlp supports.
- **Smart format default:** prefer best **H.264 (avc1) + AAC (m4a)** via format sorting (e.g. `-S vcodec:h264,res,acodec:m4a`, **VERIFY**). If only VP9/AV1 exists, take the best and run Premiere-safe after. Manual format picker from `yt-dlp -J`.
- **Section download:** one or more ranges via `--download-sections`; **Fast** (keyframe cuts) or **Exact** (`--force-keyframes-at-cuts`).
- **Extras checkboxes:** thumbnail (→ JPG/PNG/WebP), subtitles + auto-captions (SRT), chapters, description, info JSON.
- **Chat as JSON:** YouTube live-chat replay via yt-dlp (**VERIFY** `--sub-langs live_chat`); Twitch VOD chat via `TwitchDownloaderCLI chatdownload`. Overlay render is v2.
- **Channel/playlist batch:** date range or max count; `--download-archive` so re-runs skip finished items.
- **Post-download step:** automatically run Premiere-safe (or any preset) when the download isn't already Premiere-safe.
- **Signing in for members-only / sub-only / age-restricted:** two options in Settings, chosen per job or as a default:
  1. **From browser** — `--cookies-from-browser <browser>`. **Firefox is the default**; Chrome/Edge on Windows are labelled "may not work" (app-bound cookie encryption, **VERIFY** current state).
  2. **From a cookies.txt file** — user picks a Netscape-format `cookies.txt`; passed as `--cookies <path>`. See rule 11.5 for handling.

---

## 7. Video tools

The app **suggests a remux** whenever probed codecs are already Premiere-friendly. All outputs keep **every audio track** by default (OBS multi-track recordings stay separate in Premiere).

| Tool | What it does | Quality loss |
|---|---|---|
| Remux | `-c copy` into MP4/MOV. Opus/other audio → AAC 48 kHz, video untouched. | None (video) |
| Re-encode | Codec change using the encode settings panel (Section 8). | Minor |
| CFR convert | VFR → constant frame rate using the encode settings panel. Detect VFR via ffprobe (`r_frame_rate` vs `avg_frame_rate`, frame timestamp sampling). | Minor |
| Proxies | **ProRes Proxy, half resolution, default**; DNxHR LB optional. Same frame rate, duration and audio channel count for Attach Proxies (**VERIFY** current Premiere requirements). | n/a |
| Clip from timestamps | Paste ranges (`1:02:10-1:03:45`, one per line, optional label) → one file per range. Fast (stream copy) or Exact (re-encode). | None / minor |
| Trim & merge | Trim local files and join them; lossless concat when codecs/params match, otherwise re-encode via encode settings with a visible notice. | None / minor |
| Frame grab | Still at a timestamp → PNG or WebP. | n/a |
| Audio extract | WAV (for editing) or MP3/AAC (reference). Choose track(s). | None (WAV) |
| Compress to target size | Two-pass or bitrate-calculated encode to hit a size (presets: Discord 10 MB, 50 MB, 500 MB, custom). **VERIFY** current Discord limits. | Varies |
| Metadata / C2PA strip | Remux with `-map_metadata -1`, no re-encode. **VERIFY** with real C2PA-signed MP4s, confirm with `c2patool`. | None |
| Inspect | Container, codec, profile, resolution, fps, VFR flag, colour, bit depth, audio tracks, metadata, C2PA present/absent. | n/a |

---

## 8. Encode settings panel

One shared panel used by Re-encode, CFR, Premiere-safe and Compress to size. Any configuration can be saved as a preset.

- **Frame rate:** Match source (default; uses nominal rate, e.g. 60), 59.94, 30, 29.97, Custom.
- **Quality control:**
  - **By quality (default):** Visually lossless (CQ 16) · **High (CQ 19, default)** · Balanced (CQ 23) · Small (CQ 27). Map to the equivalent setting per encoder (NVENC `-cq`, VideoToolbox `-q:v`, x264 `-crf`, AMF/QSV equivalents — **VERIFY** mappings and tune by eye with Mustafa).
  - **By bitrate:** target Mbps + max Mbps, with a suggestion derived from the source bitrate (~1.5× source for a same-codec-family re-encode, tune).
- **Encoder:** auto-picked from detection (NVENC → AMF → QSV → VideoToolbox → libx264), overridable.
- **Keyframe every 1 second** (default on) for smoother Premiere scrubbing.
- **Audio:** keep all tracks (default), AAC 320 kbps 48 kHz.
- Shows **estimated size, time and output path** before running.

---

## 9. Image tools

- **Convert to WebP:** quality slider (default 85) + lossless option.
- **Strip C2PA and metadata without recompressing** via exiftool (**VERIFY** exact flags for JUMBF/C2PA removal).
- **Convert + strip in one pass.**
- Originals are never overwritten.

---

## 10. Presets

| Preset | Video | Audio | Container |
|---|---|---|---|
| **Premiere-safe** (default) | **H.264**, auto encoder, High (CQ 19), CFR match source, 1 s keyframes | All tracks, AAC 320k 48 kHz | MP4 |
| Premiere-safe HEVC | HEVC, 10-bit when source is HDR/10-bit, CFR | Same | MP4 |
| Remux to MP4 | Copy | Copy; non-AAC → AAC 48 kHz | MP4 |
| Proxy: ProRes (default proxy) | ProRes Proxy, half res | PCM 48 kHz | MOV |
| Proxy: DNxHR | DNxHR LB, half res | PCM 48 kHz | MOV |
| Strip metadata | Copy | Copy | Same as source |
| Discord 10 MB | H.264, target size | AAC 128k | MP4 |

Presets are JSON, user-duplicable and renamable, and resolve to the right encoder per machine.

---

## 11. App-wide features

1. **Drop-first Home.** Drop a file/folder or paste a link anywhere (Ctrl/Cmd+V). The app inspects it, explains what it found in plain words, and ranks module suggestions with the recommended one first. All tools remain reachable.
2. **Simple mode / Pro mode.** Simple (default on first run): three big choices — Download a video, Fix a video for editing, Clean up images — each a guided flow using defaults. Pro: full navigation and settings. Switch lives in the top bar.
3. **First-run setup:** output folder, GPU detection result, tool check. Three short steps.
4. **Tool check + updater:** on first run and from Settings › Tools. yt-dlp/Deno updates verified against published SHA-256 sums (yt-dlp `SHA2-256SUMS`), previous version kept for rollback. Errors like "Sign in to confirm you're not a bot" offer **Update and retry**.
5. **App self-update** via the Tauri updater (Section 3), prompted, never silent.
6. **Queue + history:** downloads run in parallel (default 3), encodes one at a time. Progress, ETA, speed, open file, show in folder, **retry failed jobs**, re-run, view log. History persists (SQLite).
7. **Desktop notification** when the queue finishes (and on failures).
8. **Show command:** every job reveals the exact command, copyable.
9. **Naming template:** default `{uploader}_{upload_date}_{title}`, editable. Sanitize Windows-illegal characters and long paths.
10. **Output locations:** downloads go to one folder (default `Videos\Crude Toolkit` / `~/Movies/Crude Toolkit`); processed files go to a `Toolkit` folder **next to the original**. Never overwrite originals.
11. **Right-click "Send to Toolkit"** in Explorer and Finder, opening the app with files queued on Home.
12. **Plain-language errors:** a mapping layer from known stderr patterns to a sentence + a fix button; raw stderr stays in the log.

---

## 12. UI design (locked)

Visual reference: the round 2 preview, restyled in October 2026 ("Ultraviolet", see the redesign canvas).

- **Framework:** shadcn/ui components, Tailwind v4, Geist + Geist Mono (self-hosted, no font CDN at runtime).
- **Theme:** dark only. Cool graphite greys in three depths (page `#0e0f13`, panel `#15171c`, raised `#1b1e25`), opaque borders.
- **Accent: Ultraviolet `#6a58f5`** with **white text** (4.8:1); text drawn in the accent uses the lighter `#b1a8ff`, and tints use it at 16%. Used only for the one main action on a card, the selected state, focus rings, progress bars and the logo mark. Mapped to `--primary`, `--primary-ink`, `--primary-soft`. (Replaced deep pink `#ff1477`, which was too loud and sat too close to the error colour.)
- **Status colours** are separate from the accent: green ok `#4ade80`, amber warning `#f5b841`, red error `#ff6b5e`.
- **Logo mark:** crop corners, white on an Ultraviolet tile; the same mark is the app icon.
- **Density:** compact.
- **Navigation: StreamXP-style top bar** — app name + mode badge on the left, nav dead centre (Home · Download · Video · Images · History), controls on the right (tool status, Simple/Pro switch, Settings). In Simple mode the bar shows only the name and the switch.
- **Queue:** docked on the right on Home, grouped into running, needs attention and done (one line each). Other Pro screens fold it into a "Queue · 2 running" button in the top bar that opens it as a drawer. History is its own tab.
- **Cards:** a thumbnail (a frame of the video, the picture, or the link's own preview) beside the content; problems as short bullets; one tinted **Recommended** box holding the card's only accent button; other tools as small "Or:" buttons.
- **Borrowed from StreamXP:** 1px borders and minimal shadow, 6–8px radii on badges/inputs/buttons (no pill shapes), sentence-case headings, monospace only for paths/commands/timestamps/ids, tabular numbers for live progress, no colour transitions, motion limited to progress and status dots, respects reduced motion. Every empty state says what to do next; every failure names the reason.

---

## 13. Security, privacy and engineering rules

Requirements, not suggestions.

1. **No shell strings.** Spawn every tool with an argument vector. Never `sh -c` / `cmd /c`.
2. **Validate inputs.** URLs must be http(s). Timestamps parsed and validated. Paths canonicalized. Use `--` where supported so user values can't be read as flags.
3. **Tauri capabilities:** expose only the app's own commands. No shell plugin from the frontend, no remote content in the webview, strict CSP.
4. **Verified updates** for tools (SHA-256) and the app (Tauri updater signature). Keep previous tool binary for rollback.
5. **Cookies:**
   - Browser cookies via `--cookies-from-browser`, chosen per job.
   - **cookies.txt import:** the app stores only the **path** the user picked, never a copy, and never reads the contents into logs, history or Show command output (show `--cookies <redacted path>`). Validate the Netscape header before use. Because yt-dlp writes refreshed cookies back to the file it's given, pass a **per-job temporary copy** (in the app's temp folder, deleted after the job) so the user's original isn't modified. Settings shows a one-line warning that a cookies file is as sensitive as a password.
6. **Local only:** no telemetry, analytics or crash upload.
7. **Never destroy originals.**
8. **GPU test at startup:** tiny test encode per detected hardware encoder, cached; visible notice on software fallback.
9. **Signing:** macOS ad-hoc signed, not notarized for now. README includes a screenshot guide for **System Settings › Privacy & Security › Open Anyway** (the right-click → Open shortcut no longer works on current macOS). Windows unsigned; README explains the SmartScreen "More info › Run anyway" step and that antivirus may flag yt-dlp.exe.
10. **Licenses:** `THIRD_PARTY_LICENSES.md` for every bundled binary, with source links for GPL builds.
11. **Content rights:** one line in README/About that it's for content the user has rights or permission to use.

---

## 14. Build and distribution

- **Private GitHub repo** for code.
- **GitHub Actions** builds Windows x64 and macOS arm64 installers on tag push.
- **The public repo's Releases page** receives the installers and the Tauri updater `latest.json`.
- Updater private key and any tokens are Actions secrets only.

---

## 15. Milestones

Show Mustafa something runnable at the end of each.

1. **Skeleton:** Tauri 2 + React + shadcn app runs on Windows (Mac via CI). Top bar, Simple/Pro switch, empty screens, the first (deep-pink) theme. Sidecars wired; Settings › Tools shows each version.
2. **Job runner + modules:** module trait and registry, queue (parallel downloads / serial encodes), safe arg builder, progress parsing, log capture, Show command, SQLite history, retry.
3. **Inspect + drop-first Home:** ffprobe inspector, VFR/C2PA detection, suggestions ranked on Home.
4. **Downloading:** single URL, smart H.264 default, extras, sections, post-step, cookies (browser + cookies.txt).
5. **Video tools:** encode settings panel, remux, re-encode, CFR, strip, clip, trim & merge, frame grab, audio extract, compress to size, proxies.
6. **Image tools:** WebP, exiftool strip, combined pass.
7. **Batch + chat:** playlists/channels with archive file; YouTube and Twitch chat JSON.
8. **Simple mode flows + first-run setup + plain-language errors.**
9. **Ship:** CI builds, releases repo, app updater, tool updater with checksums, Send to Toolkit, notifications, README with Open Anyway / SmartScreen guides.

**Acceptance checks** (both platforms):
- [ ] A VP9 YouTube video becomes an H.264/AAC 48 kHz CFR MP4 that imports into Premiere cleanly.
- [ ] A YouTube video with an H.264 option downloads with no re-encode.
- [ ] A 2-minute section of a long Twitch VOD works in Fast and Exact modes.
- [ ] MKV → MP4 remux finishes in seconds with an identical video stream.
- [ ] A 3-track OBS MKV keeps all 3 audio tracks through remux, CFR and Premiere-safe.
- [ ] C2PA-signed MP4 and JPEG/PNG show no C2PA after stripping (Inspect / `c2patool`).
- [ ] A filename containing `; & | $()` quotes and spaces processes safely.
- [ ] A playlist re-run skips archived items.
- [ ] A members-only video downloads with a cookies.txt, and the original cookies.txt is unchanged afterwards and never appears in logs.
- [ ] NVENC is used on the RTX 5080, VideoToolbox on the Mac, and a machine with no supported GPU falls back to libx264 with a notice.
- [ ] Compress to 10 MB produces a file at or under 10 MB.
- [ ] The app updates itself from the public repo's releases.

---

## 16. Roadmap

| Version | Features |
|---|---|
| v2 | Per-project output folders · local captions with whisper.cpp (SRT, optional burn-in) · Twitch chat overlay render (`chatrender`) · watch folder auto-processing |
| v3 | Multi-POV audio sync: align multiple angles by cross-correlating shared audio, then trim/pad or export offsets for a Premiere multicam |
