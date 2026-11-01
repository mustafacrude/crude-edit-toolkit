# Crude's Edit Toolkit: the handbook

How everything works, what each tool does, and recipes for the jobs it was built for. (Developers: the design decisions are in BRIEF.md and adding a tool is covered in docs/modules.md, both in the project folder.)

Only use it on content you own or have permission to use.

**Contents**

1. [What it is](#1-what-it-is)
2. [Finding your way around](#2-finding-your-way-around)
3. [How a job works](#3-how-a-job-works)
4. [Downloading](#4-downloading)
5. [Fixing video for Premiere](#5-fixing-video-for-premiere)
6. [Cutting, joining and extracting](#6-cutting-joining-and-extracting)
7. [Images](#7-images)
8. [Settings](#8-settings)
9. [Safety and limits](#9-safety-and-limits)
10. [Where things live](#10-where-things-live)
11. [When something goes wrong](#11-when-something-goes-wrong)
12. [Recipes](#12-recipes)
13. [What's not built yet](#13-whats-not-built-yet)
14. [Installing and uninstalling](#14-installing-and-uninstalling)

---

## 1. What it is

A desktop app that does the chores that come before editing in Premiere: getting footage from a link, making files Premiere opens without trouble, cutting the parts you want, and cleaning images. It wraps well-known command-line tools and gives them a window:

| Tool | What it's used for |
|---|---|
| yt-dlp (with Deno) | Downloading from YouTube, Twitch, SoundCloud and the 1,800-odd other sites yt-dlp supports |
| ffmpeg and ffprobe | Reading files, converting, cutting, joining, extracting |
| exiftool | Reading and removing image metadata |
| TwitchDownloaderCLI | Twitch chat replays |

They come with the app, at pinned versions with checked hashes. Nothing is installed elsewhere, nothing runs at startup, there is no account, and nothing is sent anywhere except the requests the download tools make to the sites you ask for.

It never edits your originals. Everything it makes is a new file, and it never overwrites an existing one.

## 2. Finding your way around

The top bar has the app name, the screens in the middle (**Home, Download, Video, Images, History**), the queue button (on screens other than Home), the tool status light, the **Simple / Pro** switch, this **Handbook** (the book icon) and Settings.

### Home: drop first

Drop video, images or a whole folder anywhere in the app, paste a link, or press Ctrl+V. Each thing becomes a card on Home:

- **A file** is inspected in a second or two and gets a preview: a frame from a video, or the picture itself. The card lists what Premiere will struggle with (a codec it can't read, variable frame rate, audio it doesn't support, hidden metadata or C2PA), then the best fix in a **Recommended** box with one button. Other tools that fit the file sit underneath as small buttons.
- **A link to a picture** (an image address, or a Google or Bing Images result) gets a **Save image** button instead; so does a picture you copied in a browser and pasted. See [Save from the web](#save-from-the-web).
- **A link** is looked up, then shows the site's preview picture, its title, length, the best H.264 quality on offer and whether it has chapters, subtitles or a chat replay. **Download** gets it with the defaults; **More options** opens the Download screen.
- **A playlist or channel** shows how many videos it has and the first few. A playlist has a **Download all** button; a channel asks which part first (Videos, Live, Shorts), then how many.
- A folder expands into the media files inside it, and each gets its own card.

### Send to Toolkit (Windows)

The installer adds **Send to Crude's Toolkit** to the right-click menu of video, image and audio files and of folders (on Windows 11 it's under *Show more options*). It opens the app, or brings it forward if it's already open, with those files as cards on Home. It's removed when you uninstall.

### Simple and Pro

**Simple** is for people who just want a result. Its Home has a big link box (a video link opens the download, a picture link is saved straight away), three big choices, and the last few finished files with an **Open** button. Each choice is a short guided screen (a bar at the top shows the step) that uses sensible defaults, picks with big option cards, and shows its own progress and a preview of the result:

- **Download a video.** Paste a link (or press Ctrl+V anywhere on the screen) and press **Download**. The video comes out ready for Premiere; pick **Just the audio (MP3)** to get an MP3 instead. A playlist or channel asks how many videos (newest 5, newest 20 or everything), a channel first asks which part (Videos, Live or Shorts), and a stream with a chat replay offers to save the chat too. When it's done: **Open it** or **Show in folder**.
- **Fix a video for Premiere.** Drop a video (or several, or a folder) or choose them. Each one is checked and explained in plain words: either "We'll fix it: ..." with the exact reason, or "This one is fine for Premiere as it is." One button (**Fix it**) does all of them. The fixed copy is saved next to the original. Drop an image by mistake and it points you to the image screen.
- **Images.** Paste a picture's link or a copied picture to save it, ready for Premiere. Or drop images or a folder, then choose **Make Premiere-ready** (WebP, AVIF, HEIC, SVG and JPEG XL become PNG), **Remove hidden info** (location, camera and AI labels; the picture itself isn't touched) or **Make smaller (WebP)**.

Drops and pastes on these screens go into the screen you're on, not back to Home. **Pro** shows every setting and the full Download, Video and Images screens. The switch is in the top bar and remembered.

### First-run setup

The first time the app opens (and any time from **Settings → About → Show the setup again**) three short steps appear:

1. **Where downloads go.** The folder every download lands in. Change it or keep the default.
2. **Your graphics card.** The toolkit tests each graphics-card encoder with a real encode and tells you which it will use, or that video will convert on the CPU (slower, but it works).
3. **Check the tools.** Each bundled program gets a tick, or a warning if one is missing. You can still carry on with a warning.

**Skip for now** closes it at any step; nothing is lost, and every choice can be changed in Settings.

### The queue

On Home, the panel on the right shows the jobs from the last hour in three groups: **running** and waiting (with a progress bar, time left and speed), **needs attention** (failed, with the reason and a Retry or fix button) and **done** (one line each, with open and show-in-folder buttons). On the other Pro screens it folds into a **Queue · 2 running** button in the top bar, which opens it over the right edge; Esc closes it. Click a job to open its details: where it came from, where it saved, the exact commands that ran (copy them if you want to run them yourself) and the log. **History** keeps the last 500 jobs.

## 3. How a job works

1. You choose a tool and its settings. The tool **plans** the job: it reads the file, decides the exact commands and picks output names. If it can't (a damaged file, a missing audio track) you get a failed job with the reason in plain words.
2. The job joins the **queue**. There are two lanes: downloads run in parallel (3 by default), and anything heavy on the CPU or graphics card runs **one at a time**, so a converting job never fights another for the machine.
3. While it runs you see progress. **Cancel** stops the tool and everything it started, and deletes the half-written files (never anything that existed before).
4. When it ends it is **Done** (open the file, or show it in its folder), **Failed** (with the reason, and **Retry**), or **Cancelled**.

### Where files go

- **Downloads** go to one folder: `Videos\Crude Toolkit` by default (change it in Settings). A playlist or channel gets a folder of its own inside it, and pictures saved from the web go in its `Images` folder.
- **Everything else** goes in a `Toolkit` folder next to the original, so the result is always beside its source. Proxies go in `Proxies`.
- **Names never collide.** If `clip_h264.mp4` exists, the next one is `clip_h264 (2).mp4`.
- Download names follow your naming template (default `{uploader}_{upload_date}_{title}`).

### Jobs that follow jobs

A download can hand its file straight to another tool when it finishes. By default it makes the video Premiere-safe **only if it needs it**: an H.264 download with a constant frame rate is left alone, and a VP9 or AV1 one is converted. Only the video is passed on, never a chat file.

### A file that's already there

Downloads never overwrite. If the file a download would make is already in the folder (the same video and the same section saved earlier, say), it is used as it is and the job finishes amber with "… was already in the folder, so it was used as it is and nothing new was downloaded. Rename or delete it to download again." To get a different version (a Fast cut, then an Exact cut), rename or delete the first.

### Retry and Run again

**Retry** re-plans the job from scratch, so it gets a fresh output name and picks up any file or setting changes.

## 4. Downloading

Paste a link on Home or on the **Download** screen and press **Check link**. The choices below are cards, filled in with the link's real resolution and codec once it's checked. The bar along the bottom always shows where the files will go, what happens after, and the **Download** button.

### Quality

| Choice | What you get |
|---|---|
| **Best for Premiere** (default) | The best H.264 video with AAC audio, in an MP4. Opens in Premiere as-is. |
| **Highest** | The sharpest version whatever the codec (often VP9 or AV1). Convert it afterwards. |
| **Pick a format** | A specific one from the list, with sizes. |
| **Audio only** (MP3) | Just the sound, as a 320 kbps MP3 with the title and uploader in its tags. Works with sections, playlists and channels; no video is kept. |

If a site has no H.264 version, you'll see a warning and get the best available; the "after download" step converts it.

### Only part of a video

Type a time range (`12:40-15:05`, `1:02:10-1:03:45`) and press Enter; it becomes a chip you can remove with its ×. Add as many as you like. Each range becomes its own file, named with its start and end. **Fast cut** is instant and lossless but starts at the nearest keyframe, a second or two early; **Exact cut** is frame-accurate and re-encodes near the cut. Only the sections are downloaded, not the whole video. A section that starts after the video ends is refused with a plain message ("A section starts at 1:00, but this video is only 0:19 long").

### Also save

Thumbnail (JPG, PNG or WebP), English subtitles (SRT; the original and the English track, not YouTube's dozens of auto-translations), chapters (embedded and as a JSON file), the description, and the full info JSON. If a download fails or is cancelled, these are removed along with the video.

### Members-only and age-restricted videos

Turn on sign-in in Settings (your browser's cookies, Firefox works best, or a `cookies.txt` file), then tick **Use my sign-in** on a download. The toolkit only remembers *where* a cookies file is. For each job it works from a temporary copy, hides it from commands and logs, and deletes the copy afterwards.

### Playlists and channels

Paste a playlist, a channel, or one of a channel's tabs.

- **A channel's home page** offers its tabs (Videos, Live, Shorts); pick one.
- **Which videos:** "only the newest N" (or first N for a playlist), and/or an upload date range. On a channel's list the download stops at the first video older than your "from" date instead of checking the whole channel.
- **Each video is saved in a folder** named after the playlist or channel.
- **It remembers what it saved.** Running the same channel again only gets the new videos, even if you moved or deleted the files. Tick "Download videos I already have again" to override, or clear the memory in Settings → Downloads → Playlist memory.
- **Live right now** streams are skipped; they'd record until they end.
- **One failure doesn't ruin the rest.** If some videos can't be downloaded (private, members-only, removed) the others are kept and the job finishes amber with the reason. Cancelling partway likewise keeps the finished videos.
- **It paces itself.** Long lists pause a few seconds between videos and back off when retrying, so YouTube and Twitch are less likely to block you.
- A video link that has `&list=` in it is treated as that one video, with a link to get the whole playlist.

### Chat replays

| Site | How |
|---|---|
| **Twitch** VODs and clips | Every message with its time, as JSON. With **one** section chosen, only that stretch of chat is saved, and the file's name carries the same `[start-end]` as the video. |
| **YouTube** streams that were live | The live chat as JSON (`.live_chat.json`), always the whole stream. |

Choose **With the video** or **Chat only** (seconds, no video download). For YouTube playlists, **Save chat replays** applies to the videos that were live streams. Chat from Twitch can't be downloaded for a whole channel at once.

### SoundCloud and other music sites

A link with sound but no picture gets its own choices instead of the video ones. Paste a track, a set or an artist's page.

- **Best quality** keeps the site's own audio as it arrives, with no conversion, so nothing is lost. The card says exactly what that is, such as AAC at 160 kbps. The title, uploader, date, genre and link go in the tags and the cover art is embedded. If the artist allows downloads, this is the file they uploaded, which can be lossless.
- **MP3** is for anything that insists on an MP3. It's made at 320 kbps, but a conversion can't add detail the source never had: from a 96 kbps stream you get a much bigger file that sounds the same or slightly worse. A track that's already an MP3 is kept as it is.
- **WAV** is 48 kHz 24-bit, ready for the timeline. WAV can't hold cover art.

An MP3 or WAV gets (MP3) or (WAV) on the end of its name, so it can sit next to the other versions of the same track.

SoundCloud gives most public tracks at 96 to 160 kbps. A SoundCloud Go+ sign-in (Settings, then Sign-in) can reach 256 kbps, and it's the only way to get the whole of a Go+ track, which otherwise may come as a short preview.

A **set** goes into its own folder, with the set's name as the album and the running order as track numbers. An **artist's page** asks which part you want: Tracks, Albums or Sets, because the bare address mixes in what they reposted.

Titles are left as the uploader wrote them. "Artist - Track" is common there, but so is a track's name with a dash in it, so the app doesn't guess: the artist tag is the account that posted it.

### What to expect from each site

| Site | Best to expect | Good to know |
|---|---|---|
| YouTube | H.264 up to 1080p; higher only in VP9 or AV1 | **Highest** gets those and converts them after. Age-restricted videos need sign-in. |
| Twitch | The stream's own H.264 | Chat replays save as JSON. Subscriber-only videos need sign-in. |
| SoundCloud | AAC up to 160 kbps | Go+ sign-in can reach 256 kbps. The artist's own file when they allow downloads. |
| Instagram, TikTok, X | Whatever the site serves | They often refuse without sign-in. Links change or expire quickly. |

Anything else yt-dlp supports works the same way: **Check link** shows what's really on offer before anything downloads, and each job's details in History show the exact command it ran.

## 5. Fixing video for Premiere

These are on the **Video** screen. Drop files on Home and the right one is suggested.

### Make Premiere-safe

Converts any video to an **H.264 MP4 at a constant frame rate with AAC 48 kHz audio**, keeping every audio track. This is the "drop it on the timeline and it just works" button. Defaults: quality CQ 19 (very high), the frame rate of the source matched, a keyframe every second (smooth scrubbing), audio 320 kbps. It uses your graphics card's encoder when one works, else the CPU.

### Remux to MP4

For MKV and other files whose video is already fine. Re-wraps in seconds without touching the picture. Audio Premiere can't read (Opus, for example) becomes AAC; every track is kept.

### Constant frame rate

For phone and screen recordings with **variable** frame rate, which drift out of sync in Premiere. Re-encodes at a steady rate.

### Re-encode

Everything is yours to set: H.264 or HEVC, frame rate, **quality or bitrate**, keyframe interval, audio. The same panel is used by Make Premiere-safe, with its defaults filled in.

### Make proxies

A small, fast copy of 4K or heavy footage in a `Proxies` folder next to the original: **ProRes Proxy** (default) or **DNxHR LB**, at half or quarter size. In Premiere: right-click the clip → Proxy → Attach Proxies. You edit smoothly and export at full quality.

### Compress to size

Shrinks a clip to fit an upload limit: 10 MB (Discord free), 50 MB, 500 MB or your own. It lowers the resolution when it has to, so the result stays watchable, and uses two careful passes to land just under the limit. Always on the CPU, because it hits the target far more reliably.

### Strip metadata (video)

A copy with metadata, chapters and Content Credentials (C2PA) removed. Nothing is re-encoded, so it takes seconds.

## 6. Cutting, joining and extracting

### Clip parts

Paste time ranges, one per line, with an optional label to name the file: `1:02:10-1:03:45 the clutch`. You get one file per range. **Fast** copies without re-encoding (starts at the nearest keyframe); **Exact** is frame-accurate and re-encodes as Premiere-safe H.264.

### Trim & merge

Choose several clips, put them in order (use the arrows), and optionally set a start and end on each. Clips with matching codecs join **losslessly in seconds**; mixed ones are re-encoded to match the first.

### Frame grab

Full-resolution PNG or WebP stills at the timestamps you list.

### Extract audio

The audio on its own: 24-bit **WAV** for editing, or MP3 / M4A for listening. Multi-track recordings (a game plus voice chat) give one file per track, or only the first if you choose.

## 7. Images

On the **Images** screen, or drop images on Home. It reads PNG, JPG, WebP, AVIF, HEIC (iPhone photos), SVG, JPEG XL, TIFF and BMP. Choose files, or **Choose a folder** to do every image directly inside it (not its subfolders). A folder dropped on Home shows its first 100 files and says how many were left out; Choose a folder on the Images screen takes up to 2,000.

### Save from the web

For when you're sourcing pictures for an edit and they come in small or in formats Premiere won't take. Paste into the box (or anywhere on the screen):

- **An image's address** (right-click a picture, *Copy image address*).
- **A Google or Bing Images result.** In Google Images, click the result so the large preview opens, then right-click the preview and choose *Copy image address*: that's the original, full size. Bing result links, and older Google links with `imgurl=` in them, work as they are. A plain Google *search page* link has no picture in it; the app says how to copy the right one.
- **A web page.** Its main picture (the one shown when the page is shared) is saved.
- **A copied picture** (right-click, *Copy image*, then Ctrl+V). Handy when a site won't hand the file over.

Pictures go in the `Images` folder inside your downloads folder. PNG, JPG, GIF, TIFF, BMP and PSD are kept exactly as they came; WebP, AVIF, HEIC, SVG and JPEG XL are saved as PNG (lossless, transparency kept; an SVG is drawn 4K wide or tall so logos stay sharp).

Each result shows its size. Under 1000 pixels on the long side it warns that it will look soft full screen; a Google preview is called out too. **Find a bigger version** opens Google Lens in your browser with the picture's address, to find other copies. It only sends the address, and only when you press it. Saving from the web doesn't use the queue, so it doesn't show in History.

### Make Premiere-ready (PNG)

Premiere can't import WebP, AVIF, HEIC, SVG or JPEG XL. This saves each as a PNG next to the original (in `Toolkit`): lossless, transparency kept, phone photos turned upright, and the colour profile kept so wide-colour iPhone photos don't look dull. Home recommends it for those formats.

### Strip metadata (images)

Removes location, camera details, editing history and **Content Credentials (C2PA)**, including the AI-generated label some tools add, **without recompressing**. The picture is pixel-identical. The rotation is kept, so a phone photo stays upright. Home flags images that carry a location or C2PA and recommends this.

### Convert to WebP

Usually a third smaller than JPEG at the same quality. Quality 85 looks the same for almost every image; **Lossless** is pixel-perfect (best for screenshots). Keeps transparency. Removes metadata and C2PA unless you untick it; the colour profile is kept by default so wide-colour phone photos don't look dull.

## 8. Settings

| Section | Setting | What it does |
|---|---|---|
| Downloads | Folder | Where downloads go. |
| | File names | Template using `{title}`, `{uploader}`, `{upload_date}`, `{id}`, `{site}`. Shows a live example. |
| | Playlist memory | How many downloads playlist, channel and set runs remember; **Forget them** clears it. |
| Sign-in | Off / From my browser / From a cookies.txt file | See [section 4](#members-only-and-age-restricted-videos). |
| Encoding | Use | Automatic (the fastest encoder that passes a real test), a specific graphics card family, or CPU only. **Test again** re-runs the test. |
| Performance | Downloads at once | 1–6, default 3. Converting always runs one at a time. |
| | Speed limit per download | No limit, or 2 / 5 / 10 / 25 MB/s for each download. |
| | Most of the CPU to use | All, 75% or 50% (Windows). |
| Tools | Check again | Shows each bundled tool's version and whether it runs. |
| About | Show the setup again | Opens the first-run setup. |

### Updates and notifications

Settings → **Updates and notifications**.

- **The app.** When it opens (at most once a day) the app looks for a newer version. If there is one, **Update available** appears in the top bar. Opening it shows the version and what's new; **Install and restart** downloads it, checks its signature and restarts into it. It won't install while jobs are running, because restarting would stop them. Your settings and history carry over.
- **yt-dlp.** Sites change constantly and an old yt-dlp is what breaks first, so it can be updated between app releases. The new copy comes from yt-dlp's own GitHub release, is checked against that release's published checksum, and is run once before it's used. It's kept in the app's data folder; the copy that came with the app is never touched, and **Use the built-in one** goes back to it. A new app version brings its own yt-dlp and drops any downloaded copy. The other tools (ffmpeg, Deno, ExifTool, TwitchDownloaderCLI) update with the app.
- **Nothing installs by itself.** Looking for updates only looks. You can turn the launch check off here, and **Check for updates** works any time.
- **A notification** appears when the whole queue finishes while the app isn't in front ("Done: clip.mp4", "All 3 jobs are done", "2 done, 1 couldn't be finished"). Untick the option to turn it off.

## 9. Safety and limits

The toolkit is built so a long or mistaken job can't take your computer down. What is limited, and what isn't:

**Limited**

- **CPU priority.** Every tool runs at below-normal priority, so Premiere, your browser and Windows stay responsive while it works.
- **Memory.** All tools together may use at most 70% of your RAM (and never less than 2 GB). A job that hits the cap fails instead of making Windows swap itself to a crawl, usually with an "out of memory" message.
- **CPU share (optional).** Settings → Performance can hard-cap the tools at 75% or 50% of the CPU, even while a job is running.
- **Cleanup if the app dies.** All tools are tied to the app. If it crashes or is killed, they stop with it, instead of carrying on invisibly. Quitting the app stops them too.
- **Crash dialogs.** A crashing tool ends its own job; it doesn't open a Windows error window that would leave the job hanging.
- **Disk space.** A job won't start with under 1 GB free where it saves, and stops (cleaning up) if the drive falls below 500 MB while it runs, before Windows itself runs short.
- **Hangs.** A tool that prints nothing for 30 minutes is stopped. Quick checks (reading a file, testing an encoder) are stopped after 30–60 seconds; a damaged file or a network drive that stopped answering can't freeze the app.
- **Work in flight.** One conversion at a time; 3 downloads at once (changeable); only 3 link lookups and 3 file inspections at a time, so pasting fifty links or dropping a huge folder queues politely.
- **The queue and files.** At most 2500 jobs waiting or running; history keeps 500 jobs; a job's log keeps its latest 8 MB (and one older part); over-long tool output lines are cut; a cookies file over 20 MB or a link over 4,096 characters is refused; at most 100 sections per download.
- **Sites.** Playlist downloads pace their requests and back off on retries, retries and connections time out, and an optional speed limit keeps your connection usable. A "too many requests" reply says to wait a few minutes.
- **One copy.** Opening the app twice brings the first window forward instead of starting a second copy that would fight over the same files.
- **Sleep.** On Windows the computer won't go to sleep by itself while jobs run (closing a laptop lid still does).
- **Recovery.** If the history file is ever damaged, it is set aside as `history.sqlite.broken` and a fresh one is started so the app still opens. A panic is written to `logs\crash.log`.

**Not limited, honestly**

- **The graphics card.** Nothing caps GPU use. The protection is that only one conversion runs at a time. Hardware encoders (NVENC, AMF, Quick Sync) use a dedicated part of the card and little CPU, and the app tests an encoder with a real five-frame encode before trusting it. A full system crash from a driver fault is not something an app can promise to prevent.
- **The CPU limit and the memory cap** are Windows-only for now. The Mac code runs tools at lower priority and keeps the computer awake, but it has not been tested on a Mac, so treat all of that as planned.

**What it never does**

- Overwrite or delete a file it didn't create.
- Build a command as one string for a shell: every command is a list of arguments, so a file called `clip; del *.*` is just a file name.
- Run anything at startup, or in the background after you close it.

## 10. Where things live

Windows: `%APPDATA%\net.crudes.edittoolkit\`. Mac: `~/Library/Application Support/net.crudes.edittoolkit/`.

| What | Where |
|---|---|
| Settings | `settings.json` (only the path of a cookies file is stored, never its contents) |
| Job history | `history.sqlite` |
| Job logs | `logs\<job id>.log` (and `.1.log` for older output of a long job) |
| Crash log | `logs\crash.log` |
| Playlist memory | `download-archive.txt` |
| Encoder test results | `encoders.json` |
| Scratch files | `tmp\` (emptied every time the app starts) |
| A newer yt-dlp, if you updated it | `tools\yt-dlp\<version>\` and `tools\active.json` |
| When updates were last checked | `update-check.txt` |

Pressing **Clear finished** in History removes finished jobs and their logs. The files they made stay.

## 11. When something goes wrong

Every failed job shows a one-sentence reason, and the full tool output is in its log (click the job, then **Log**). Where a button would help, it appears right next to the reason:

| Button | Appears when | What it does |
|---|---|---|
| **Try again** / **Retry** | Any failure | Runs the job again from scratch |
| **Retry with my sign-in** | A video needs you signed in, and sign-in is already set up in Settings | Runs the download again with your saved sign-in turned on |
| **Set up sign-in** | A video needs you signed in and it isn't set up, or the site refused the sign-in you have | Opens Settings at Sign-in |
| **Choose another folder** | A download's drive is full or not writable | Opens Settings at Downloads |
| **Open encoding settings** | The graphics card couldn't start its encoder | Opens Settings at Encoding, where **CPU only** always works |
| **Open performance settings** | The job ran out of memory | Opens Settings at Performance |
| **Check the tools** | A bundled tool is missing | Opens Settings at Tools |
| **Update and retry** | A download failed in a way a newer yt-dlp usually fixes (YouTube changed something, the site refused, it can't read the page) | Downloads the newest yt-dlp, checks it, then runs the job again. If yt-dlp is already the newest, it just retries |

| You see | Usually means | Try |
|---|---|---|
| The drive is full | Not enough room to write the file | Free some space and retry |
| Only N MB is free on D:\ | The safety check stopped or refused the job | Free some space and retry |
| A section starts at 1:00, but this video is only 0:19 long | A time range is past the end of the video | Fix the range |
| … was already in the folder, so it was used as it is | A download would have made a file that's already there | Rename or delete the old file to download again |
| The job ran out of memory | The memory limit was reached | Close other apps; run fewer jobs at once |
| Your graphics card couldn't start the encoder | It's busy with another app, or its driver is old | Retry, update the driver, or choose CPU only |
| This file looks damaged / unfinished | A recording or download was cut off | Re-download or re-record |
| Couldn't open the file | It was moved or deleted, or its drive isn't connected | Check the file is still there |
| Another app has this file open | A player or Premiere has it | Close it and retry |
| Age-restricted / members-only / subscribers only | The site wants you signed in | Use the sign-in buttons above |
| A live stream or premiere that hasn't started | It isn't available to download yet | Try again once it has started |
| Isn't available where you are | The uploader blocked your country | Nothing the toolkit can do |
| The site says that page doesn't exist | A wrong or expired link | Copy the link again |
| YouTube blocked this download | YouTube is rate-limiting or changed something | Press **Update and retry**, or wait a few minutes |
| The site is limiting how fast you can download | HTTP 429 | Wait a few minutes |
| Showed no sign of life for 30 minutes | A connection dropped | Retry |
| The tools light is red | A bundled tool is missing or won't start | Settings → Tools → Check again; reinstall the app |

If a chrome-based browser's cookies don't work for sign-in, use Firefox or a `cookies.txt` file; Chrome and Edge lock their cookies on Windows.

## 12. Recipes

**Download a stream VOD and clip the best moments.** Paste the link, put the moments in *Only part of it* (`1:02:10-1:03:45`), choose **Exact cut**, and **Download**. Each moment is its own file, already Premiere-safe if it needed to be.

**Get the Twitch chat for a clip.** Paste the VOD link, set the one range, choose **With the video**. You get the `.mp4` and a `.chat.json` with the same name and range, covering only those minutes.

**Archive every new video from a channel each week.** Paste the channel, choose Videos, leave "newest" empty and set "Uploaded from" a week back, then Download. Do the same each week: the app remembers what it already saved.

**Grab only the last 5 shorts.** Paste the channel, choose Shorts, set "Only the newest" to 5.

**A phone recording drifts out of sync in Premiere.** Drop it on Home. It's flagged as variable frame rate and **Constant frame rate** is recommended. Run it, import the result.

**A 4K MKV won't scrub smoothly.** Drop it on Home, run **Make Premiere-safe**, then **Make proxies** if the edit is heavy. Attach the proxies in Premiere.

**Send a clip in Discord.** Drop the clip, choose **Compress to size → 10 MB**. The result lands just under the limit.

**Turn a livestream recording into separate tracks.** Drop the recording, **Extract audio → WAV → Every track**. You get the game, the voice chat and the mic as separate files, ready for the timeline.

**Stitch five clips together fast.** **Trim & merge**, add them in order, trim the ends. If they match it takes seconds and loses nothing.

**Post an AI image without the hidden labels.** Drop the image. If it carries Content Credentials the card says so; run **Strip metadata**. The picture is identical, the C2PA and camera/location data are gone.

**Grab a logo or picture for an edit.** In Google Images, click the result, then right-click the large preview and choose *Copy image address* (or *Copy image* if the site won't hand it over). Paste it on Home or Images → Save from the web. A WebP or SVG arrives as a PNG Premiere imports; if it says the picture is small, try **Find a bigger version**.

**Use iPhone photos in Premiere.** Drop the HEIC files (or their folder) on Images, choose **Make Premiere-ready**. You get full-size, upright PNGs beside them.

**Prepare a whole folder of screenshots for a site.** Images → **Choose a folder** → **Convert to WebP** (Lossless for UI screenshots). Originals are untouched; the WebPs go in a `Toolkit` folder inside the folder.

**Let a render run while you edit.** Settings → Performance → set the CPU to 50%. Premiere stays smooth; the conversion takes longer.

## 13. What's not built yet

Planned, not in the app today:

- A **tested Mac build**: it's set up and built with each release (installer, Finder's Open With, VideoToolbox, an Apple Silicon ffmpeg) but hasn't been run on a Mac yet. One known gap: the Mac ffmpeg can't convert SVG pictures to PNG.
- Twitch chat rendered as a video overlay, and per-project output folders (version 2).

## 14. Installing and uninstalling

**Windows.** Run the installer from the Releases page. It installs for your user only (no admin password) into `%LOCALAPPDATA%\Crude's Edit Toolkit`, adds a Start menu entry and an entry under **Settings → Apps → Installed apps**, and never starts with Windows. The first time, SmartScreen may say "Windows protected your PC" (the installer isn't signed with a paid certificate): **More info → Run anyway**.

To uninstall, use Settings → Apps → Installed apps → Crude's Edit Toolkit → Uninstall. That removes the program, the bundled tools, the Start menu entry and the right-click menu entry. Ticking **Delete the application data** also removes settings, history and logs (the folder in section 10). Your downloads and processed files are never touched.

**Mac (Apple Silicon, preview).** Drag the app to Applications and open it; macOS will refuse the first time, so go to **System Settings → Privacy & Security** and click **Open Anyway** (right-click → Open no longer works). To remove it, drag the app to the Trash; the settings folder in section 10 stays unless you delete it.
