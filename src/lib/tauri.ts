// Typed wrappers for the Rust commands. The UI talks to the backend only
// through these; it never runs tools itself (BRIEF §13.3).
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export { isTauri };

export type ToolId = "yt-dlp" | "deno" | "ffmpeg" | "ffprobe" | "exiftool" | "twitch-downloader";

export interface ToolInfo {
  id: ToolId;
  name: string;
  purpose: string;
  version: string | null;
  path: string;
  /** The file isn't there at all, as opposed to being there but failing. */
  missing: boolean;
  error: string | null;
}

export type Mode = "simple" | "pro";

export type SignIn = "none" | "browser" | "file";

export interface Settings {
  mode: Mode;
  downloadConcurrency: number;
  downloadDir: string;
  /** e.g. "{uploader}_{upload_date}_{title}" */
  namingTemplate: string;
  signIn: SignIn;
  browser: string;
  /** Path only; the toolkit never copies or logs the file. */
  cookiesFile: string | null;
  /** "auto", a GPU family ("nvenc", "amf", "qsv", "videotoolbox") or "software". */
  encoder: string;
  /** Share of the CPU all tools may use together, in percent: 100, 75 or 50. */
  cpuLimit: number;
  /** Speed limit for each download, in MB/s. 0 means no limit. */
  speedLimitMb: number;
  /** The first-run setup has been finished or skipped. */
  firstRunDone: boolean;
  notifyWhenDone: boolean;
  checkForUpdates: boolean;
}

// ---- tools (modules) and their settings forms ----

export interface Choice {
  value: string;
  label: string;
}

export type Field =
  | { kind: "select"; key: string; label: string; options: Choice[]; default: string; help: string | null }
  | {
      kind: "number";
      key: string;
      label: string;
      min: number;
      max: number;
      step: number;
      unit: string;
      default: number;
      help: string | null;
      showWhen: { key: string; value: string } | null;
      slider: boolean;
    }
  | { kind: "toggle"; key: string; label: string; default: boolean; help: string | null }
  | { kind: "lines"; key: string; label: string; placeholder: string; help: string | null }
  | { kind: "encode"; key: string }
  | { kind: "trimList"; key: string };

export interface ModuleInfo {
  id: string;
  name: string;
  description: string;
  category: "download" | "video" | "image";
  accepts: "video" | "image" | "link";
  combinesInputs: boolean;
  schema: Field[];
}

export type RateControl = { mode: "quality"; cq: number } | { mode: "bitrate"; targetMbps: number; maxMbps: number };

export interface EncodeSettings {
  codec: "h264" | "hevc";
  /** null: match the source. */
  fps: string | null;
  rate: RateControl;
  keyframeSecs: number | null;
  audioKbps: number;
}

export const PREMIERE_SAFE: EncodeSettings = {
  codec: "h264",
  fps: null,
  rate: { mode: "quality", cq: 19 },
  keyframeSecs: 1,
  audioKbps: 320,
};

export interface EncoderView {
  /** e.g. "h264_nvenc" */
  h264: string;
  /** e.g. "NVIDIA (NVENC)" */
  label: string;
  /** GPU families that passed the test, e.g. ["nvenc", "amf"]. */
  hardware: string[];
}

export const listModules = () => invoke<ModuleInfo[]>("list_modules");
export const encoderStatus = () => invoke<EncoderView>("encoder_status");
export const retestEncoders = () => invoke<EncoderView>("retest_encoders");

export const BROWSERS = ["firefox", "chrome", "edge", "brave", "opera", "vivaldi", "safari"] as const;

export interface FormatView {
  id: string;
  /** e.g. "1080p60 · H.264 · mp4" */
  label: string;
  height: number | null;
  sizeBytes: number | null;
  videoOnly: boolean;
}

export interface LinkInfo {
  url: string;
  title: string;
  /** The site's preview picture. */
  thumbnail: string | null;
  uploader: string | null;
  duration: number | null;
  /** YYYY-MM-DD */
  uploadDate: string | null;
  site: string;
  isLive: boolean;
  chapters: number;
  hasSubtitles: boolean;
  /** Best H.264 on offer, e.g. "1080p60"; null means converting will be needed. */
  bestH264: string | null;
  /** Best overall, e.g. "2160p60 VP9". */
  best: string | null;
  formats: FormatView[];
  kind: "video" | "playlist" | "channel";
  id: string | null;
  /** A playlist or channel: what's in it. */
  playlist: PlaylistView | null;
  /** Which chat replay can be saved with this video. */
  chat: "youtube" | "twitch" | null;
  /** A video opened from a playlist: a link to the whole playlist. */
  playlistUrl: string | null;
  /** Sound with no picture: a track, or a list of them. */
  audioOnly: boolean;
  /** The best sound on offer, e.g. "AAC · 160 kbps". Only on a sound-only link. */
  bestAudio: string | null;
  /** That audio's file type, e.g. "m4a". */
  audioExt: string | null;
  /** The file the artist uploaded is on offer, not just the site's stream. */
  originalFile: boolean;
  /** All that's on offer is a short preview. */
  previewOnly: boolean;
}

export interface PlaylistView {
  /** How many videos, when known. With `more`, at least this many. */
  count: number | null;
  more: boolean;
  /** The first few, as a preview. */
  entries: { title: string; duration: number | null }[];
  /** A channel's home page: its tabs (Videos, Live, Shorts). Pick one to download. */
  tabs: { label: string; url: string; count: number | null; more: boolean }[];
  /** A channel's uploads, newest first. */
  newestFirst: boolean;
}

/** "43 videos", "100+ tracks" */
export const videoCount = (count: number | null, more: boolean, noun: "video" | "track" = "video") =>
  count == null
    ? noun === "track"
      ? "Tracks"
      : "Videos"
    : `${count}${more ? "+" : ""} ${noun}${count === 1 && !more ? "" : "s"}`;

/** What one thing on this link is called. */
export const linkNoun = (info: LinkInfo | null): "video" | "track" => (info?.audioOnly ? "track" : "video");

/** Download options that come from the link lookup: playlist handling and names. */
export const linkOptions = (info: LinkInfo): Partial<DownloadOptions> => ({
  title: info.title,
  playlist: info.kind !== "video",
  newestFirst: info.playlist?.newestFirst ?? false,
  audioOnly: info.audioOnly,
  meta: {
    title: info.title,
    uploader: info.uploader,
    uploadDate: info.uploadDate,
    id: info.id,
    site: info.site,
    duration: info.duration,
    audioExt: info.audioExt,
  },
});

export interface DownloadOptions {
  quality: "premiere" | "highest" | "format" | "mp3" | "audio" | "wav";
  formatId?: string;
  sections?: string[];
  exactCuts?: boolean;
  thumbnail?: boolean;
  thumbnailFormat?: "jpg" | "png" | "webp";
  subtitles?: boolean;
  chapters?: boolean;
  description?: boolean;
  infoJson?: boolean;
  signIn?: boolean;
  title?: string;
  /** The whole playlist or channel tab, into its own folder. */
  playlist?: boolean;
  newestFirst?: boolean;
  /** The lookup found sound but no picture. */
  audioOnly?: boolean;
  /** Only the first N videos (a channel's newest N). */
  maxItems?: number;
  /** YYYY-MM-DD, inclusive. */
  dateAfter?: string;
  dateBefore?: string;
  /** Download again videos an earlier playlist download saved. */
  redownload?: boolean;
  /** Also save the chat replay as JSON. */
  chat?: boolean;
  chatOnly?: boolean;
  meta?: {
    title: string;
    uploader: string | null;
    uploadDate: string | null;
    id: string | null;
    site: string;
    /** Seconds, so a section past the end is caught before anything runs. */
    duration: number | null;
    /** The file type of the best audio, on a sound-only link. */
    audioExt: string | null;
  };
  /** Module to run on the downloaded file afterwards. */
  then?: string;
  thenOptions?: Record<string, unknown>;
}

/** After a download: make it Premiere-safe, but only when it isn't already. */
export const PREMIERE_SAFE_IF_NEEDED = { then: "premiere-safe", thenOptions: { onlyIfNeeded: true } } as const;

export const fetchLinkInfo = (url: string, signIn: boolean) => invoke<LinkInfo>("fetch_link_info", { url, signIn });
export const downloadArchiveCount = () => invoke<number>("download_archive_count");
export const forgetDownloadArchive = () => invoke<void>("forget_download_archive");
export const pickFolder = () => invoke<string | null>("pick_folder");

/** An image saved from a link or the clipboard into the downloads folder's Images folder. */
export interface SavedImage {
  id: string;
  path: string;
  width: number | null;
  height: number | null;
  /** What it arrived as ("WebP") when it was converted to PNG. */
  convertedFrom: string | null;
  notes: string[];
  /** The image's own address; null for a pasted image. */
  source: string | null;
}

export const saveImageLink = (url: string) => invoke<SavedImage>("save_image_link", { url });
export const savePastedImage = async (image: Blob) =>
  invoke<SavedImage>("save_pasted_image", new Uint8Array(await image.arrayBuffer()));
export const savedImageAction = (id: string, action: "open" | "reveal" | "lens") =>
  invoke<void>("saved_image_action", { id, action });

/** A link to a picture itself, or a Google or Bing Images result, rather than a video page. */
export function isImageLink(link: string): boolean {
  try {
    const url = new URL(link);
    return (
      /\.(png|jpe?g|jfif|webp|avif|heic|heif|svg|gif|bmp|tiff?|jxl)$/i.test(url.pathname) ||
      url.searchParams.has("imgurl") ||
      url.searchParams.has("mediaurl") ||
      /^encrypted-tbn\d\.gstatic\.com$/.test(url.hostname)
    );
  } catch {
    return false;
  }
}
export const pickCookiesFile = () => invoke<string | null>("pick_cookies_file");

export type SettingsSection = "signin" | "downloads" | "encoding" | "performance" | "tools" | "updates";

export type Fix =
  | { kind: "openSettings"; section: SettingsSection }
  | { kind: "retryWithSignIn" }
  | { kind: "updateAndRetry" };

export type JobStatus = "queued" | "running" | "done" | "failed" | "cancelled" | "interrupted";

export interface Job {
  id: string;
  module: string;
  title: string;
  lane: "download" | "process";
  status: JobStatus;
  inputs: string[];
  /** What it was started with; Retry runs it again with the same options. */
  options: unknown;
  steps: { label: string; command: string }[];
  currentStep: number;
  /** Overall 0..1, or null while unknown. */
  progress: number | null;
  etaSecs: number | null;
  speed: string | null;
  outputs: string[];
  /** Why it failed, or a warning when it finished with a problem. */
  error: string | null;
  /** A button that would help with `error`. */
  fix: Fix | null;
  /** The job that started this one (a download's Premiere-safe follow-up). */
  parent: string | null;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
}

function requireApp() {
  if (!isTauri()) throw new Error("Open this inside the app. A plain browser can't run the tools.");
}

export async function toolVersions(): Promise<ToolInfo[]> {
  requireApp();
  return invoke("tool_versions");
}

export const licensesText = () => invoke<string>("licenses_text");
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });

export const listJobs = () => invoke<Job[]>("list_jobs");
export const startJobs = (module: string, inputs: string[], options?: unknown) =>
  invoke<string[]>("start_jobs", { module, inputs, options });
export const cancelJob = (id: string) => invoke<void>("cancel_job", { id });
export const retryJob = (id: string) => invoke<string[]>("retry_job", { id });
export const clearFinished = () => invoke<void>("clear_finished");
export const jobLog = (id: string) => invoke<string>("job_log", { id });
export const openJobOutput = (id: string) => invoke<void>("open_job_output", { id });
export const revealJobOutput = (id: string) => invoke<void>("reveal_job_output", { id });
/** "video" or "image" limits the picker; otherwise videos, images and audio. */
export const pickFiles = (filter?: "video" | "image") => invoke<string[]>("pick_files", { filter });

export type Severity = "ok" | "info" | "warn";

export interface Inspection {
  path: string;
  name: string;
  kind: "video" | "audio" | "image";
  duration: number | null;
  sizeBytes: number | null;
  /** e.g. "VP9 · 2560×1440 · 60 fps" */
  headline: string;
  /** What's wrong in a sentence or two, or that nothing is. */
  summary: string;
  findings: { severity: Severity; label: string; detail: string | null }[];
  details: { label: string; value: string }[];
  c2pa: "present" | "absent" | "unknown";
  suggestions: { module: string; name: string; reason: string; recommended: boolean }[];
}

export const inspectFile = (path: string) => invoke<Inspection>("inspect_file", { path });
/** A small JPEG data URL of a file on disk or a link's thumbnail address, or null. */
export const thumbnailOf = (source: { path?: string; url?: string }) => invoke<string | null>("thumbnail", source);
/** Folders become the files inside them; `kind` keeps only images or videos. */
export interface Expanded {
  files: string[];
  /** Files past the limit that were left out. */
  skipped: number;
}
export const expandPaths = (paths: string[], kind?: "image" | "video") =>
  invoke<Expanded>("expand_paths", { paths, kind });

/** "Showing the first 100 of 300 files." when a folder had more than the limit. */
export const skippedNotice = (found: number, skipped: number) =>
  skipped > 0 ? `Showing the first ${found} of ${found + skipped} files. The rest were left out.` : null;

/** What a look for updates found. Nothing is installed by looking. */
export interface UpdatesFound {
  app: { version: string; notes: string | null } | null;
  ytDlp: { tool: string; current: string | null; latest: string; available: boolean; downloaded: boolean } | null;
}
export const checkForUpdates = () => invoke<UpdatesFound>("check_for_updates");
/** Returns the new version, or null if the one in use is already the newest. */
export const updateYtDlp = () => invoke<string | null>("update_yt_dlp");
export const goBackToBundledYtDlp = () => invoke<void>("use_bundled_yt_dlp");
export const installAppUpdate = () => invoke<void>("install_app_update");
export function onUpdatesFound(handler: (found: UpdatesFound) => void): Promise<UnlistenFn> {
  return listen<UpdatesFound>("updates-found", (e) => handler(e.payload));
}

/** Files handed to the app from outside: Explorer's "Send to Crude's Toolkit". */
export const takeOpenedFiles = () => invoke<string[]>("take_opened_files");
/** Fires when files were added to the waiting list; collect them with takeOpenedFiles. */
export function onFilesOpened(handler: () => void): Promise<UnlistenFn> {
  return listen("files-opened", () => handler());
}

export function onJobUpdated(handler: (job: Job) => void): Promise<UnlistenFn> {
  return listen<Job>("job-updated", (e) => handler(e.payload));
}

export function onJobsRemoved(handler: (ids: string[]) => void): Promise<UnlistenFn> {
  return listen<string[]>("jobs-removed", (e) => handler(e.payload));
}
