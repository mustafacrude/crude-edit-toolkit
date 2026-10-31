# Tool modules

Everything the toolkit can do is a module: a Rust type implementing `Module` in
`src-tauri/src/modules/`. The queue, history, Home suggestions and the tool
screens only see modules through that trait, so a new feature is a new file plus
one line in the `MODULES` list. Its settings form, its place on the Video
screen and its Run button come for free.

## The contract

| Method | What it does |
|---|---|
| `id()` | Stable id stored in job history (`"remux"`). Never rename one; old history points at it. |
| `name()` | Shown in the UI (`"Remux to MP4"`). |
| `description()` | One or two plain sentences: what it does and what you get. Shown above its form. |
| `category()` | `Download`, `Video` or `Image`: which screen lists it. |
| `lane()` | `Download` (runs in parallel, default 3) or `Process` (CPU/GPU heavy, one at a time). |
| `accepts()` | `Video` (default) or `Link`: which picker the UI offers. |
| `options_schema()` | The settings form, as a list of `Field`s (see below). Options arrive in `plan` keyed by each field's `key`; a missing key, or `null` options, means the defaults. |
| `suggest(info)` | Optional. Given an inspected file's `MediaInfo` (with `vfr` and `c2pa` filled in), return a `Suggestion` (a plain-language reason and a 0–100 priority) when this module fixes something about it. Home shows suggestions highest priority first; the top one at 50 or above is marked Recommended. |
| `plan(ctx, input, options)` | Turns one input into a `Plan`: a title, the tool `Step`s to run, and the output files it will create. Runs off the UI thread and may probe the file. Return `Err` with a plain sentence when the input can't be handled; the user sees it as a failed job. Set `Plan::skip` (with a reason) when there's nothing to do; no job is queued. |
| `combines_inputs()` / `plan_combined(ctx, inputs, options)` | For tools that turn several inputs into one job (Trim & merge). |

### Form fields (`modules::schema::Field`)

| Kind | Renders as | Option value |
|---|---|---|
| `Select` | Segmented buttons | the chosen `value` |
| `Number` | Number input with a unit, or a slider (`slider: true`); `show_when` hides it unless another field has a value (compared as text, so a toggle is `"true"`/`"false"`) | number |
| `Toggle` | Checkbox | boolean |
| `Lines` | Multi-line text | array of non-empty lines |
| `Encode` | The shared encode panel (codec, frame rate, quality or bitrate, keyframes, audio) | `EncodeSettings` |
| `TrimList` | The chosen files in order, each with start/end and reorder buttons | `[{start, end}]` in file order |

## Rules for `plan`

- **Arguments are a `Vec<String>`.** Never build a shell string. Inputs reaching a
  module are already validated: files are canonical absolute paths that exist,
  links are http(s).
- **Outputs come from `ctx.output_next_to(...)`** (or `ctx.output_in(...)` for
  another folder name, like `Proxies`), which returns a path that doesn't exist
  and isn't claimed by another queued job. Pass `-n` to ffmpeg as well, so it
  never overwrites.
- **exiftool** reads its arguments from a UTF-8 file (`exif::argfile`), because
  on Windows it can't take non-ASCII file names on its command line. Add the
  file to `Plan::temp_files`; Show command displays its contents.
- **Encoders:** `ctx.encoder(Codec::H264)` gives the encoder for this machine
  (tested GPU encoder, or software). Build video arguments with
  `encode::video_args`, never by hand, so every encoder family gets the right
  rate control and pixel format.
- **Progress:** add `FFMPEG_PROGRESS_ARGS` (or `YTDLP_PROGRESS_ARGS`) to the
  step's args and set the matching `ProgressKind`.
- **Scratch files** (`ctx.temp_path(...)`) go in `Plan::temp_files` and are
  deleted when the job ends, however it ends.
- **Secrets:** put the index of any sensitive argument (a cookies file path) in
  `Step::redact`. It's hidden from Show command, and its value is scrubbed from
  the log.
- **Start tools with `tools::spawn` (long runs) or `tools::run` (quick probes),
  never `.spawn()` or `.output()` yourself.** Those put the tool under the app's
  limits (guard.rs): low priority, the shared memory cap, optional CPU cap,
  killed if the app dies, and for `run`, a time limit and capped output. A
  direct call skips all of that.
- **Where a job writes:** outputs' folders are checked for free disk space
  automatically. Add any other folder to `Plan::writes_to` (the download folder,
  when file names aren't known yet).
- **Many files from one step** (a playlist): set `Plan::partial_ok`. Saving
  nothing is then not an error, and a failure or cancel that saved some files
  keeps them; see below.
- **Files in other folders:** `ctx.unique_in(dir, name, ext)` reserves a free
  path anywhere (Twitch chat goes beside the downloads).

## What the runner guarantees

- Outputs that don't exist when a job starts are deleted if it fails or is
  cancelled, as are files a tool announces it's creating (yt-dlp's
  "Destination:" lines) and their `.part` files. Originals are never touched.
- With `partial_ok`, a failed or cancelled job that saved files finishes as
  done with a warning (the job's `error`, shown in amber), keeping the finished
  files and anything named after them; only unfinished pieces are deleted.
- Cancel stops the whole process tree (yt-dlp's launcher, its worker, ffmpeg).
- Each job gets a log file (`<app data>/logs/<job id>.log`) with every command
  and everything the tools printed, minus progress lines.
- A non-zero exit becomes a plain-language error via `errors::explain`. To
  teach it a new failure, add a `(text to find, plain sentence, Hint)` row to
  `PATTERNS` in `errors.rs`, specific rows before general ones. The `Hint`
  (sign-in, folder, encoder, memory, tools) decides which fix button
  `errors::fix_for` puts on the job; the UI only shows what the job carries.
  Add a unit test with a real line of the tool's output.
- A finished job with `options.then` runs that module on its video outputs,
  with `options.thenOptions`.
- yt-dlp exit code 101 (stopped on purpose by `--break-match-filters`) counts
  as success.
