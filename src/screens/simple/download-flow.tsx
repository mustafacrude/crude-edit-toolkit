import {
  Download as DownloadIcon,
  Film,
  LoaderCircle,
  Music,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { useIntakeSink } from "@/lib/intake";
import { useJobs } from "@/lib/jobs";
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
import { cn } from "@/lib/utils";
import { LinkSummary } from "@/components/link-summary";
import { Thumb } from "@/components/thumb";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { FlowFrame, FlowProgress, OptionCards } from "@/screens/simple/flow";

const isLink = (s: string) => /^https?:\/\/\S+$/i.test(s.trim());

type Lookup =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; url: string; info: LinkInfo }
  | { state: "error"; error: string };

/** What to save from a sound-only link. */
type Sound = "audio" | "mp3" | "wav";

/** How many videos to take from a playlist or channel; "all" means everything. */
const COUNTS = ["5", "20", "all"] as const;
type Count = (typeof COUNTS)[number];

/** Simple mode: paste a link, say how much, press the button. */
export function DownloadFlow() {
  const { downloadUrl, openSettings } = useAppNav();
  const { settings } = useSettings();
  const { start } = useJobs();
  const [url, setUrl] = useState(downloadUrl);
  const [lookup, setLookup] = useState<Lookup>({ state: "idle" });
  const [count, setCount] = useState<Count>("5");
  const [chat, setChat] = useState(false);
  const [audio, setAudio] = useState(false);
  const [sound, setSound] = useState<Sound>("audio");
  const [signIn, setSignIn] = useState(false);
  const [ids, setIds] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef("");

  const check = async (link: string, withSignIn = signIn) => {
    const trimmed = link.trim();
    if (!isLink(trimmed)) return;
    latest.current = trimmed;
    setLookup({ state: "loading" });
    setIds(null);
    setError(null);
    try {
      const info = await fetchLinkInfo(trimmed, withSignIn);
      if (latest.current === trimmed)
        setLookup({ state: "ready", url: trimmed, info });
    } catch (e) {
      if (latest.current === trimmed)
        setLookup({ state: "error", error: String(e) });
    }
  };

  // A link from Home's "More options", or pasted anywhere on this screen.
  useEffect(() => {
    if (downloadUrl) {
      setUrl(downloadUrl);
      void check(downloadUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [downloadUrl]);
  useIntakeSink(({ urls }) => {
    const link = urls[0];
    if (link) {
      setUrl(link);
      void check(link);
    }
  });

  const info =
    lookup.state === "ready" && lookup.url === url.trim() ? lookup.info : null;
  const list = info?.playlist ?? null;
  const pickTab = list != null && list.tabs.length > 0;
  const soundOnly = info?.audioOnly ?? false;
  const noun = linkNoun(info);
  const chatChoice = info
    ? list
      ? info.site === "YoutubeTab"
      : info.chat != null
    : false;
  const mentionsSignIn =
    lookup.state === "error" && /sign.?in/i.test(lookup.error);

  const download = async () => {
    if (!info) return;
    setError(null);
    const options: DownloadOptions = {
      quality: soundOnly ? sound : audio ? "mp3" : "premiere",
      signIn,
      ...linkOptions(info),
      ...(list && count !== "all" ? { maxItems: Number(count) } : {}),
      chat: chatChoice && chat,
      ...(audio || soundOnly ? {} : PREMIERE_SAFE_IF_NEEDED),
    };
    const started = await start(
      "download",
      [lookup.state === "ready" ? lookup.url : url.trim()],
      options,
    );
    if (started.length === 0)
      setError("Couldn't start the download. Try again.");
    else setIds(started);
  };

  const again = () => {
    setIds(null);
    setUrl("");
    setLookup({ state: "idle" });
    setChat(false);
    setAudio(false);
    setSound("audio");
  };

  if (ids) {
    return (
      <FlowFrame
        title="Your download"
        intro={
          soundOnly
            ? "Your audio is saved with its details in the tags."
            : audio
              ? "Your MP3 is saved with the title in its tags."
              : "Your video comes out ready for Premiere."
        }
        step={3}
        steps={3}
      >
        <FlowProgress
          ids={ids}
          onChange={setIds}
          onAgain={again}
          againLabel="Download another"
        />
      </FlowFrame>
    );
  }

  return (
    <FlowFrame
      title="Download a video or track"
      intro="Paste a link from YouTube, Twitch, SoundCloud or 1,800 other sites."
      step={info ? 2 : 1}
      steps={3}
    >
      <form
        className="flex gap-2 rounded-[14px] border border-input bg-card p-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          void check(url);
        }}
      >
        <Input
          autoFocus
          value={url}
          aria-label="Link"
          placeholder="Paste a YouTube, Twitch, SoundCloud or other link"
          className={cn(
            "h-11 border-0 bg-transparent shadow-none focus-visible:ring-0 dark:bg-transparent",
            url ? "font-mono text-[13px]" : "text-[15px]",
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
          // Once checked, Download below is the one accent button.
          variant={info ? "outline" : "default"}
          className="h-11 rounded-[10px] px-5"
          disabled={!isLink(url) || lookup.state === "loading"}
        >
          {lookup.state === "loading" && (
            <LoaderCircle className="animate-spin motion-reduce:animate-none" />
          )}
          Check link
        </Button>
      </form>

      {lookup.state === "error" && (
        <div className="flex flex-col gap-2 rounded-[14px] border border-status-error/30 bg-status-error/[0.06] p-4">
          <p className="text-[13px] break-words text-status-error">
            {lookup.error}
          </p>
          <div className="flex flex-wrap gap-2">
            {settings.signIn !== "none" && !signIn && (
              <Button
                size="sm"
                onClick={() => {
                  setSignIn(true);
                  void check(url, true);
                }}
              >
                Try again with my sign-in
              </Button>
            )}
            {settings.signIn === "none" && mentionsSignIn && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => openSettings("signin")}
              >
                Set up sign-in
              </Button>
            )}
          </div>
        </div>
      )}

      {info && (
        <div className="flex flex-col gap-5 rounded-2xl border bg-card p-5">
          <div className="flex flex-wrap gap-4">
            <Thumb
              url={info.thumbnail}
              kind={soundOnly ? "audio" : "video"}
              duration={info.duration}
              className={soundOnly ? "size-28" : "w-44"}
            />
            <div className="min-w-0 flex-[1_1_280px]">
              <LinkSummary
                info={info}
                onOpenLink={(link) => {
                  setUrl(link);
                  void check(link);
                }}
              />
            </div>
          </div>

          {pickTab && (
            <p className="text-[13px] text-muted-foreground">
              {soundOnly
                ? "Pick which part of this profile you want above."
                : "Pick which part of the channel you want above."}
            </p>
          )}

          {list && !pickTab && (
            <OptionCards<Count>
              label={`How many ${noun}s?`}
              value={count}
              onChange={setCount}
              options={[
                {
                  value: "5",
                  title: info.kind === "channel" ? "Newest 5" : "First 5",
                  body: "A quick grab.",
                },
                {
                  value: "20",
                  title: info.kind === "channel" ? "Newest 20" : "First 20",
                  body: "A good batch.",
                },
                {
                  value: "all",
                  title: "Everything",
                  body: `All ${videoCount(list.count, list.more, noun).toLowerCase()}. A long list takes a while.`,
                },
              ]}
            />
          )}

          {soundOnly ? (
            <OptionCards<Sound>
              label="What do you want?"
              value={sound}
              onChange={setSound}
              options={[
                {
                  value: "audio",
                  title: "Best quality",
                  icon: Music,
                  body: info.bestAudio
                    ? `${info.bestAudio}, not converted. Tags and cover art included.`
                    : "The site's own audio, not converted. Tags and cover art included.",
                },
                {
                  value: "mp3",
                  title: "MP3",
                  icon: Music,
                  body: "For anything that needs an MP3. Bigger, but no better.",
                },
                {
                  value: "wav",
                  title: "WAV",
                  icon: Music,
                  body: "Uncompressed, ready for the timeline.",
                },
              ]}
            />
          ) : (
            <OptionCards<"video" | "mp3">
              label="What do you want?"
              value={audio ? "mp3" : "video"}
              onChange={(v) => setAudio(v === "mp3")}
              options={[
                {
                  value: "video",
                  title: "Video",
                  icon: Film,
                  body: info.bestH264
                    ? `${info.bestH264} H.264, opens in Premiere as it is.`
                    : "The best version, converted for Premiere.",
                },
                {
                  value: "mp3",
                  title: "Just the audio",
                  icon: Music,
                  body: "A 320 kbps MP3 with the title and cover in its tags.",
                },
              ]}
            />
          )}

          {chatChoice && (
            <label className="flex h-9 items-center gap-2 self-start rounded-lg border px-3 text-[13px]">
              <input
                type="checkbox"
                checked={chat}
                onChange={(e) => setChat(e.target.checked)}
                className="size-4 accent-primary"
              />
              Also save the chat replay
            </label>
          )}

          <div className="flex flex-wrap items-center gap-3 border-t pt-4">
            <div className="flex min-w-0 flex-[1_1_240px] flex-col gap-0.5">
              <span className="text-xs text-faint">
                {list ? "They go in a folder named after it, in" : "Saves to"}
              </span>
              <span
                className="truncate font-mono text-xs selectable"
                title={settings.downloadDir}
              >
                {settings.downloadDir}
              </span>
            </div>
            <Button
              size="lg"
              className="h-11 rounded-[10px] px-6 text-[14px]"
              onClick={download}
              disabled={pickTab}
            >
              <DownloadIcon />
              {list ? "Download them" : "Download"}
            </Button>
          </div>
          {error && <p className="text-[13px] text-status-error">{error}</p>}
        </div>
      )}
    </FlowFrame>
  );
}
