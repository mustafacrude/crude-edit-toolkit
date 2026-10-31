import {
  Check,
  ChevronRight,
  Download as DownloadIcon,
  LoaderCircle,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { bytes } from "@/lib/format";
import { useJobs } from "@/lib/jobs";
import { PAGE_FRAME } from "@/lib/page";
import { useSettings } from "@/lib/settings";
import {
  fetchLinkInfo,
  linkNoun,
  linkOptions,
  PREMIERE_SAFE_IF_NEEDED,
  videoCount,
  type DownloadOptions,
  type LinkInfo,
} from "@/lib/tauri";
import { cn, radioArrows } from "@/lib/utils";
import { LinkSummary } from "@/components/link-summary";
import { Thumb } from "@/components/thumb";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

const isLink = (s: string) => /^https?:\/\/\S+$/i.test(s.trim());

type Fetch =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; url: string; info: LinkInfo }
  | { state: "error"; error: string };

type ChatChoice = "none" | "with" | "only";

const SOUND_QUALITIES: DownloadOptions["quality"][] = ["audio", "mp3", "wav"];

export function Download() {
  const { downloadUrl, navigate } = useAppNav();
  const { settings } = useSettings();
  const { start } = useJobs();
  const [url, setUrl] = useState(downloadUrl);
  const [fetched, setFetched] = useState<Fetch>({ state: "idle" });
  const [signIn, setSignIn] = useState(false);
  const [quality, setQuality] =
    useState<DownloadOptions["quality"]>("premiere");
  const [formatId, setFormatId] = useState("");
  const [then, setThen] = useState("premiere-safe");
  const [sections, setSections] = useState("");
  // A range being typed, before it becomes a chip.
  const [rangeDraft, setRangeDraft] = useState("");
  const [exactCuts, setExactCuts] = useState(false);
  const [extras, setExtras] = useState({
    thumbnail: false,
    subtitles: false,
    chapters: false,
    description: false,
    infoJson: false,
  });
  const [thumbnailFormat, setThumbnailFormat] = useState<
    "jpg" | "png" | "webp"
  >("jpg");
  const [maxItems, setMaxItems] = useState("");
  const [dateAfter, setDateAfter] = useState("");
  const [dateBefore, setDateBefore] = useState("");
  const [redownload, setRedownload] = useState(false);
  const [chatChoice, setChatChoice] = useState<ChatChoice>("none");
  const [queued, setQueued] = useState(false);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lastChecked = useRef("");

  const signInReady = settings.signIn !== "none";
  // Details only count for the link they were looked up for.
  const info =
    fetched.state === "ready" && fetched.url === url.trim()
      ? fetched.info
      : null;
  const list = info?.playlist ?? null;
  const pickTab = list != null && list.tabs.length > 0;
  const soundOnly = info?.audioOnly ?? false;
  const noun = linkNoun(info);
  // The cards differ between sound and video links, so a choice from one can't carry over.
  const chosen: DownloadOptions["quality"] = soundOnly
    ? SOUND_QUALITIES.includes(quality)
      ? quality
      : "audio"
    : quality === "audio" || quality === "wav"
      ? "premiere"
      : quality;
  const justSound = SOUND_QUALITIES.includes(chosen);
  // YouTube keeps chat replays per video, so a YouTube playlist can bring them along.
  const chatAvailable = info
    ? list
      ? info.site === "YoutubeTab"
      : info.chat != null
    : false;
  const chat: ChatChoice = chatAvailable ? chatChoice : "none";

  const check = async (link = url, withSignIn = signIn) => {
    const trimmed = link.trim();
    if (!isLink(trimmed)) return;
    lastChecked.current = `${trimmed}|${withSignIn}`;
    setFetched({ state: "loading" });
    setQueued(false);
    try {
      const result = await fetchLinkInfo(trimmed, withSignIn);
      if (lastChecked.current === `${trimmed}|${withSignIn}`)
        setFetched({ state: "ready", url: trimmed, info: result });
    } catch (e) {
      if (lastChecked.current === `${trimmed}|${withSignIn}`)
        setFetched({ state: "error", error: String(e) });
    }
  };

  // A link handed over from Home is checked straight away.
  useEffect(() => {
    if (downloadUrl) {
      setUrl(downloadUrl);
      void check(downloadUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [downloadUrl]);

  const openLink = (link: string) => {
    setUrl(link);
    void check(link);
  };

  const download = async () => {
    setError(null);
    const max = maxItems.trim() ? Number(maxItems) : undefined;
    const options: DownloadOptions = {
      quality: chosen,
      formatId: chosen === "format" ? formatId : undefined,
      // A range still being typed counts too.
      sections: list
        ? []
        : [...sections.split("\n"), rangeDraft]
            .map((l) => l.trim())
            .filter(Boolean),
      exactCuts: exactCuts && !justSound,
      ...extras,
      ...(soundOnly ? { subtitles: false, chapters: false } : {}),
      thumbnailFormat,
      signIn,
      ...(info ? linkOptions(info) : {}),
      ...(list
        ? {
            maxItems: max,
            dateAfter: dateAfter || undefined,
            dateBefore: dateBefore || undefined,
            redownload,
          }
        : {}),
      chat: chat !== "none",
      chatOnly: chat === "only",
      ...(chat === "only" || justSound
        ? {}
        : then === "premiere-safe"
          ? PREMIERE_SAFE_IF_NEEDED
          : then
            ? { then }
            : {}),
    };
    if (pickTab) {
      setError(
        soundOnly
          ? "Pick which part of this profile to download first."
          : "Pick which part of the channel to download first.",
      );
      return;
    }
    if (chosen === "format" && !formatId && chat !== "only") {
      setError("Pick a format first.");
      return;
    }
    if (max !== undefined && !(Number.isInteger(max) && max > 0)) {
      setError(`The number of ${noun}s has to be a whole number, 1 or more.`);
      return;
    }
    if (dateAfter && dateBefore && dateAfter > dateBefore) {
      setError('The "from" date is after the "to" date.');
      return;
    }
    await start("download", [url.trim()], options);
    setQueued(true);
  };

  // What each quality choice gets, with this link's real numbers once it's checked.
  const qualities: {
    value: DownloadOptions["quality"];
    name: string;
    spec: string;
    note: string;
    disabled?: boolean;
  }[] = soundOnly
    ? [
        {
          value: "audio",
          name: "Best quality",
          spec: info?.bestAudio ?? "The site's own audio",
          note: info?.originalFile
            ? "The file the artist uploaded, with tags and cover art."
            : "Not converted, so nothing is lost. Tags and cover art included.",
        },
        {
          value: "mp3",
          name: "MP3",
          spec: "MP3 · 320 kbps",
          note: info?.bestAudio?.startsWith("MP3")
            ? "Already an MP3, so it's kept as it is."
            : "For anything that needs an MP3. Bigger, but no better than the source.",
        },
        {
          value: "wav",
          name: "WAV",
          spec: "WAV · 48 kHz · 24-bit",
          note: "Uncompressed, ready for the timeline. WAV can't hold cover art.",
        },
      ]
    : [
        {
          value: "premiere",
          name: "Best for Premiere",
          spec: info?.bestH264
            ? `${info.bestH264} · H.264 · AAC`
            : "H.264 · AAC · MP4",
          note:
            info && !list && !info.bestH264
              ? "No H.264 here, so the best version, converted after."
              : "Opens in Premiere as it is.",
        },
        {
          value: "highest",
          name: "Highest",
          spec: info?.best ?? "Best available",
          note: "Sharpest, any codec. VP9 or AV1 gets converted after.",
        },
        {
          value: "mp3",
          name: "Audio only",
          spec: "MP3 · 320 kbps",
          note: "Just the sound, with the title and cover in its tags.",
        },
        {
          value: "format",
          name: "Pick a format",
          spec:
            info && !list
              ? `${info.formats.length} available`
              : "After checking a video",
          note: "An exact resolution and codec.",
          disabled: !info || list != null,
        },
      ];
  const ranges = sections.split("\n").filter((l) => l.trim());
  const addRange = () => {
    const next = rangeDraft.trim();
    if (!next) return;
    setSections([...ranges, next].join("\n"));
    setRangeDraft("");
  };
  const removeRange = (i: number) =>
    setSections(ranges.filter((_, j) => j !== i).join("\n"));
  const afterLine =
    chat === "only"
      ? "Chat only, no video"
      : chosen === "audio"
        ? "Kept as the site sends it"
        : justSound
          ? "One audio file, nothing to convert after"
          : then === "premiere-safe"
            ? "Then made Premiere-safe if needed"
            : then === "remux"
              ? "Then re-wrapped as MP4"
              : "Kept exactly as it arrives";
  // What the folded-away options currently say, so closing them never hides a choice.
  const moreSummary = [
    extras.thumbnail && (soundOnly ? "Cover art file" : "Thumbnail"),
    !soundOnly && extras.subtitles && "Subtitles",
    !soundOnly && extras.chapters && "Chapters",
    extras.description && "Description",
    extras.infoJson && "Details file",
    !justSound && chat !== "only"
      ? then === "premiere-safe"
        ? "Premiere-safe after"
        : then === "remux"
          ? "Re-wrapped as MP4"
          : "No conversion after"
      : null,
    signIn && "Signed in",
  ]
    .filter(Boolean)
    .join(", ");
  const slash = settings.downloadDir.includes("\\") ? "\\" : "/";
  const savesTo = `${settings.downloadDir}${list && !pickTab && info ? `${slash}${info.title}` : ""}`;

  return (
    <div className={cn(PAGE_FRAME, "gap-4")}>
      <h1 className="text-xl font-semibold tracking-[-0.015em]">Download</h1>

      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void check();
        }}
      >
        <Input
          id="download-url"
          value={url}
          aria-label="Link"
          placeholder="Paste a YouTube, Twitch, SoundCloud or other link"
          className={cn(
            "h-10 rounded-[10px] bg-card",
            url && "font-mono text-[13px]",
          )}
          onChange={(e) => {
            setUrl(e.target.value);
            if (
              isLink(e.target.value) &&
              e.nativeEvent instanceof InputEvent &&
              e.nativeEvent.inputType === "insertFromPaste"
            ) {
              void check(e.target.value);
            }
          }}
        />
        <Button
          type="submit"
          variant="outline"
          className="h-10 rounded-[10px]"
          disabled={!isLink(url) || fetched.state === "loading"}
        >
          {fetched.state === "loading" ? (
            <LoaderCircle className="animate-spin motion-reduce:animate-none" />
          ) : null}
          Check link
        </Button>
      </form>

      {fetched.state === "error" && (
        <div className="flex flex-col gap-2 rounded-[12px] border border-status-error/30 bg-status-error/[0.06] p-3">
          <p className="text-[13px] text-status-error">{fetched.error}</p>
          {signInReady && !signIn && (
            <Button
              variant="outline"
              size="sm"
              className="self-start"
              onClick={() => {
                setSignIn(true);
                void check(url, true);
              }}
            >
              Try again with my sign-in
            </Button>
          )}
        </div>
      )}

      {info && (
        <div className="flex flex-wrap gap-4 rounded-[14px] border bg-card p-3.5">
          <Thumb
            url={info.thumbnail}
            kind={soundOnly ? "audio" : "video"}
            duration={info.duration}
            className={soundOnly ? "size-28" : "w-40"}
          />
          <div className="min-w-0 flex-[1_1_280px]">
            <LinkSummary info={info} onOpenLink={openLink} />
          </div>
        </div>
      )}

      <section aria-labelledby="dl-quality" className="flex flex-col gap-2.5">
        <h2
          id="dl-quality"
          className="text-[13px] font-semibold text-muted-foreground"
        >
          What to get
        </h2>
        <div
          role="radiogroup"
          aria-labelledby="dl-quality"
          onKeyDown={radioArrows}
          className="grid grid-cols-[repeat(auto-fit,minmax(190px,1fr))] gap-2.5"
        >
          {qualities.map((q) => {
            const on = chosen === q.value;
            return (
              <button
                key={q.value}
                type="button"
                role="radio"
                aria-checked={on}
                disabled={q.disabled || chat === "only"}
                onClick={() => setQuality(q.value)}
                className={cn(
                  "flex flex-col gap-1.5 rounded-[12px] border-[1.5px] p-3.5 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-45",
                  on
                    ? "border-primary bg-primary-soft"
                    : "border-border bg-card hover:border-input",
                )}
              >
                <span className="flex items-center justify-between gap-2">
                  <span className="text-[13px] font-semibold">{q.name}</span>
                  <span
                    className={cn(
                      "size-3.5 shrink-0 rounded-full",
                      on
                        ? "border-[1.5px] border-primary bg-primary shadow-[inset_0_0_0_3px_var(--primary-soft)]"
                        : "border-[1.5px] border-input",
                    )}
                    aria-hidden
                  />
                </span>
                <span className="font-mono text-xs">{q.spec}</span>
                <span className="text-xs leading-relaxed text-muted-foreground">
                  {q.note}
                </span>
              </button>
            );
          })}
        </div>
        {chosen === "format" && info && (
          <NativeSelect
            id="download-format"
            value={formatId}
            onChange={(e) => setFormatId(e.target.value)}
            className="w-full max-w-md"
            aria-label="Format"
          >
            <NativeSelectOption value="">Choose a format…</NativeSelectOption>
            {info.formats.map((f) => (
              <NativeSelectOption key={f.id} value={f.id}>
                {f.label}
                {f.sizeBytes ? ` · ${bytes(f.sizeBytes)}` : ""}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        )}
      </section>

      {pickTab ? null : list ? (
        <Panel title={`Which ${noun}s`}>
          <div className="flex flex-wrap items-center gap-2 text-[13px]">
            <span>
              {list.newestFirst ? "Only the newest" : "Only the first"}
            </span>
            <Input
              id="download-max"
              value={maxItems}
              onChange={(e) =>
                setMaxItems(e.target.value.replace(/[^0-9]/g, ""))
              }
              placeholder="all"
              inputMode="numeric"
              className="w-20 tabular-nums"
            />
            <span className="text-muted-foreground">
              of {videoCount(list.count, list.more, noun).toLowerCase()}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-[13px]">
            <span>Uploaded from</span>
            <Input
              type="date"
              aria-label="Uploaded on or after"
              value={dateAfter}
              onChange={(e) => setDateAfter(e.target.value)}
              className="w-40"
            />
            <span>to</span>
            <Input
              type="date"
              aria-label="Uploaded on or before"
              value={dateBefore}
              onChange={(e) => setDateBefore(e.target.value)}
              className="w-40"
            />
          </div>
          <Extra
            id="x-redownload"
            label={`Download ${noun}s I already have again`}
            checked={redownload}
            onChange={setRedownload}
          />
          <p className="text-xs text-faint">
            Leave these empty for everything. Anything saved before is skipped,
            so running it again only gets what's new.
          </p>
        </Panel>
      ) : (
        <Panel
          title="Only part of it"
          hint="optional"
          action={
            justSound ? undefined : (
              <ToggleGroup
                type="single"
                variant="outline"
                size="sm"
                spacing={0}
                value={exactCuts ? "exact" : "fast"}
                onValueChange={(v) => v && setExactCuts(v === "exact")}
                disabled={ranges.length === 0}
                aria-label="Cut style"
              >
                <ToggleGroupItem value="fast">Fast cut</ToggleGroupItem>
                <ToggleGroupItem value="exact">Exact cut</ToggleGroupItem>
              </ToggleGroup>
            )
          }
        >
          <div className="flex flex-wrap items-center gap-2">
            {ranges.map((range, i) => (
              <span
                key={`${range}-${i}`}
                className="flex h-8 items-center gap-1.5 rounded-lg bg-muted pr-1 pl-2.5 font-mono text-xs"
              >
                {range.replace(/\s*-\s*/, " – ")}
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Remove ${range}`}
                  className="text-faint"
                  onClick={() => removeRange(i)}
                >
                  <X />
                </Button>
              </span>
            ))}
            <Input
              id="download-sections"
              value={rangeDraft}
              aria-label="Add a time range"
              placeholder={
                ranges.length
                  ? "Add another range"
                  : "e.g. 12:40-15:05, then Enter"
              }
              className="h-8 w-64 font-mono text-xs"
              onChange={(e) => setRangeDraft(e.target.value)}
              onBlur={addRange}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addRange();
                }
              }}
            />
          </div>
          <p className="text-xs text-faint">
            {ranges.length === 0 || justSound
              ? `Leave empty for the whole ${noun}. Each range becomes its own file.`
              : exactCuts
                ? "Frame-accurate. Re-encodes a few seconds around each cut."
                : "Cuts at the nearest keyframe, up to two seconds early. No quality loss."}
          </p>
        </Panel>
      )}

      {chatAvailable && info && (
        <Panel title="Chat replay">
          {list ? (
            <Extra
              id="x-chat"
              label="Save chat replays as JSON"
              checked={chat !== "none"}
              onChange={(v) => setChatChoice(v ? "with" : "none")}
            />
          ) : (
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              spacing={0}
              value={chat}
              onValueChange={(v) => v && setChatChoice(v as ChatChoice)}
              aria-label="Chat replay"
              className="self-start"
            >
              <ToggleGroupItem value="none">Don't save</ToggleGroupItem>
              <ToggleGroupItem value="with">With the video</ToggleGroupItem>
              <ToggleGroupItem value="only">Chat only</ToggleGroupItem>
            </ToggleGroup>
          )}
          <p className="text-xs text-faint">
            {list
              ? "For the videos that were live streams, as JSON next to each video."
              : info.chat === "twitch"
                ? "Every message with its time, as JSON. With one section above, only that stretch is saved."
                : "The whole stream's chat as JSON, with each message's time. Sections don't trim it."}
          </p>
        </Panel>
      )}

      <section aria-label="More options" className="flex flex-col gap-3">
        <button
          type="button"
          aria-expanded={more}
          onClick={() => setMore(!more)}
          className="flex items-center gap-1 self-start rounded-md py-1 pr-2 text-[13px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          <ChevronRight
            className={cn(
              "size-3.5 transition-transform motion-reduce:transition-none",
              more && "rotate-90",
            )}
            aria-hidden
          />
          More options
          {moreSummary && (
            <span className="text-xs text-faint"> · {moreSummary}</span>
          )}
        </button>
        {more && (
          <>
            <section
              aria-labelledby="dl-extras"
              className="flex flex-wrap items-center gap-2"
            >
              <h2
                id="dl-extras"
                className="mr-1.5 text-[13px] font-semibold text-muted-foreground"
              >
                Also save
              </h2>
              <Extra
                id="x-thumbnail"
                label={soundOnly ? "Cover art file" : "Thumbnail"}
                checked={extras.thumbnail}
                onChange={(v) => setExtras({ ...extras, thumbnail: v })}
              />
              {extras.thumbnail && (
                <NativeSelect
                  size="sm"
                  value={thumbnailFormat}
                  onChange={(e) => setThumbnailFormat(e.target.value as "jpg")}
                  aria-label="Thumbnail format"
                >
                  <NativeSelectOption value="jpg">JPG</NativeSelectOption>
                  <NativeSelectOption value="png">PNG</NativeSelectOption>
                  <NativeSelectOption value="webp">WebP</NativeSelectOption>
                </NativeSelect>
              )}
              {!soundOnly && (
                <>
                  <Extra
                    id="x-subtitles"
                    label="Subtitles"
                    checked={extras.subtitles}
                    disabled={info && !list ? !info.hasSubtitles : false}
                    onChange={(v) => setExtras({ ...extras, subtitles: v })}
                  />
                  <Extra
                    id="x-chapters"
                    label="Chapters"
                    checked={extras.chapters}
                    disabled={info && !list ? info.chapters === 0 : false}
                    onChange={(v) => setExtras({ ...extras, chapters: v })}
                  />
                </>
              )}
              <Extra
                id="x-description"
                label="Description"
                checked={extras.description}
                onChange={(v) => setExtras({ ...extras, description: v })}
              />
              <Extra
                id="x-info"
                label="Details file (.json)"
                checked={extras.infoJson}
                onChange={(v) => setExtras({ ...extras, infoJson: v })}
              />
            </section>

            <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-3">
              {!soundOnly && (
                <Panel title="After download">
                  <NativeSelect
                    id="download-then"
                    value={justSound ? "" : then}
                    disabled={chat === "only" || justSound}
                    onChange={(e) => setThen(e.target.value)}
                    className="w-full"
                  >
                    <NativeSelectOption value="premiere-safe">
                      Make Premiere-safe if needed
                    </NativeSelectOption>
                    <NativeSelectOption value="remux">
                      Re-wrap as MP4 (no re-encode)
                    </NativeSelectOption>
                    <NativeSelectOption value="">Nothing</NativeSelectOption>
                  </NativeSelect>
                  <p className="text-xs text-faint">
                    {justSound
                      ? "Nothing to do after an audio file."
                      : then === "premiere-safe"
                        ? "Converts only when it's needed (VP9, AV1, variable frame rate)."
                        : then === "remux"
                          ? "Re-wraps as MP4 without re-encoding."
                          : "Keeps the file exactly as it arrives."}
                  </p>
                </Panel>
              )}
              <Panel title="Sign-in">
                <Extra
                  id="x-signin"
                  label={
                    settings.signIn === "browser"
                      ? `Use my ${settings.browser[0].toUpperCase()}${settings.browser.slice(1)} sign-in`
                      : settings.signIn === "file"
                        ? "Use my cookies.txt"
                        : "Use my sign-in"
                  }
                  checked={signIn}
                  disabled={!signInReady}
                  onChange={setSignIn}
                />
                <p className="text-xs text-faint">
                  {signInReady ? (
                    soundOnly ? (
                      "For private tracks, and Go+ quality on SoundCloud."
                    ) : (
                      "For members-only and age-restricted videos."
                    )
                  ) : (
                    <>
                      {soundOnly
                        ? "For private tracks and Go+ quality."
                        : "For members-only videos."}{" "}
                      <button
                        type="button"
                        className="underline underline-offset-2 hover:text-foreground"
                        onClick={() => navigate("settings")}
                      >
                        Set it up in Settings
                      </button>
                      .
                    </>
                  )}
                </p>
              </Panel>
            </div>
          </>
        )}
      </section>

      <div className="sticky -bottom-5 z-10 -mx-1 mt-1 flex flex-wrap items-center gap-3 border-t bg-background px-1 pt-3.5 pb-5">
        <div className="flex min-w-0 flex-[1_1_260px] flex-col gap-0.5">
          <span className="text-xs text-faint">Saves to</span>
          <span
            className="truncate font-mono text-xs selectable"
            title={savesTo}
          >
            {savesTo}
          </span>
        </div>
        <span className="text-xs text-muted-foreground">
          {isLink(url) ? afterLine : "Paste a link above to start."}
        </span>
        {queued && (
          <Button variant="ghost" size="sm" onClick={() => setQueued(false)}>
            Download again
          </Button>
        )}
        <Button
          size="lg"
          className="h-10 rounded-[10px] px-5 text-[14px]"
          onClick={download}
          disabled={!isLink(url) || queued}
        >
          {queued ? <Check /> : <DownloadIcon />}
          {queued
            ? "Added to queue"
            : chat === "only"
              ? "Download chat"
              : list
                ? "Download videos"
                : ranges.length > 1
                  ? `Download ${ranges.length} ${chosen === "mp3" ? "MP3s" : justSound ? "audio files" : "clips"}`
                  : "Download"}
        </Button>
      </div>
      {error && <p className="-mt-2 text-[13px] text-status-error">{error}</p>}
    </div>
  );
}

/** A titled block of related settings. */
function Panel({
  title,
  hint,
  action,
  children,
}: {
  title: string;
  hint?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2.5 rounded-[14px] border bg-card p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-[13px] font-semibold">
          {title}
          {hint && (
            <span className="ml-1.5 font-normal text-faint">{hint}</span>
          )}
        </h2>
        {action}
      </div>
      {children}
    </section>
  );
}

/** A tick option as a small outlined pill. */
function Extra({
  id,
  label,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div
      className={cn(
        "flex h-8 items-center gap-2 self-start rounded-lg border px-2.5",
        checked && !disabled && "border-input bg-raised",
        disabled && "opacity-50",
      )}
    >
      <Checkbox
        id={id}
        checked={checked && !disabled}
        disabled={disabled}
        onCheckedChange={(v) => onChange(v === true)}
      />
      <Label htmlFor={id} className="text-[13px] font-normal">
        {label}
      </Label>
    </div>
  );
}
