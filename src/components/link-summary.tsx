import { ListVideo } from "lucide-react";
import { clock, siteName } from "@/lib/format";
import { linkNoun, videoCount, type LinkInfo } from "@/lib/tauri";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

/**
 * Title, who/when, and what quality is on offer, as badges. Shared by Home and
 * Download. `onOpenLink` follows a channel tab or a video's playlist.
 */
export function LinkSummary({
  info,
  onOpenLink,
}: {
  info: LinkInfo;
  onOpenLink?: (url: string) => void;
}) {
  if (info.playlist)
    return <PlaylistSummary info={info} onOpenLink={onOpenLink} />;

  // "Twitch · … · Twitch" reads oddly when the uploader is the site.
  const meta = [
    ...new Set(
      [
        info.uploader,
        info.duration ? clock(info.duration) : null,
        info.uploadDate,
        siteName(info.site),
      ].filter(Boolean),
    ),
  ];

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-[15px] leading-snug font-semibold text-balance">
          {info.title}
        </span>
        <span className="text-xs text-muted-foreground">
          {meta.join(" · ")}
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {info.bestAudio && <Chip tone="accent">{info.bestAudio}</Chip>}
        {info.previewOnly && (
          <Chip
            tone="warn"
            title="The site only gives out a short preview of this track without a subscription."
          >
            Preview only
          </Chip>
        )}
        {info.site.startsWith("Soundcloud") && !info.originalFile && (
          <Chip title="A SoundCloud Go+ sign-in can reach 256 kbps. Set up sign-in in Settings.">
            Higher with Go+
          </Chip>
        )}
        {info.bestH264 ? (
          <Chip tone="accent">{info.bestH264} H.264, opens in Premiere</Chip>
        ) : (
          info.best && (
            <Chip
              tone="warn"
              title="Premiere can't edit this codec smoothly. It will need converting to H.264."
            >
              No H.264 · best is {info.best}
            </Chip>
          )
        )}
        {info.bestH264 &&
          info.best &&
          !info.best.startsWith(info.bestH264 + " ") && (
            <Chip title="Available with Highest quality, but it will need converting for Premiere.">
              Up to {info.best}
            </Chip>
          )}
        {info.chapters > 0 && (
          <Chip>
            {info.chapters} chapter{info.chapters === 1 ? "" : "s"}
          </Chip>
        )}
        {info.hasSubtitles && <Chip>English subtitles</Chip>}
        {info.chat && <Chip>Chat replay</Chip>}
        {info.isLive && <Chip tone="warn">Live right now</Chip>}
      </div>
      {info.playlistUrl && onOpenLink && (
        <Button
          variant="ghost"
          size="xs"
          className="self-start text-muted-foreground"
          onClick={() => onOpenLink(info.playlistUrl!)}
        >
          <ListVideo />
          Part of a playlist: get the whole playlist
        </Button>
      )}
    </div>
  );
}

/** A small filled label: accent for the headline fact, amber for a caution, grey for the rest. */
export function Chip({
  tone = "plain",
  title,
  children,
}: {
  tone?: "plain" | "accent" | "warn";
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      title={title}
      className={
        "rounded-md px-2 py-0.5 text-xs " +
        (tone === "accent"
          ? "bg-primary-soft font-medium text-primary-ink"
          : tone === "warn"
            ? "bg-status-warn/12 font-medium text-status-warn"
            : "bg-muted text-muted-foreground")
      }
    >
      {children}
    </span>
  );
}

function PlaylistSummary({
  info,
  onOpenLink,
}: {
  info: LinkInfo;
  onOpenLink?: (url: string) => void;
}) {
  const list = info.playlist!;
  const noun = linkNoun(info);
  const meta = [
    info.uploader,
    list.tabs.length ? null : videoCount(list.count, list.more, noun),
    siteName(info.site),
  ].filter(Boolean);
  const unseen = list.count != null ? list.count - list.entries.length : 0;

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="flex items-center gap-2 text-[14px] font-semibold text-balance">
          {info.title}
          <Badge variant="outline">
            {info.audioOnly
              ? info.kind === "channel"
                ? "Artist"
                : "Set"
              : info.kind === "channel"
                ? "Channel"
                : "Playlist"}
          </Badge>
        </span>
        <span className="text-xs text-muted-foreground">
          {meta.join(" · ")}
        </span>
      </div>

      {list.tabs.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <span className="text-xs text-muted-foreground">
            {info.audioOnly
              ? "Which part of this profile?"
              : "Which part of the channel?"}
          </span>
          <div className="flex flex-wrap gap-1.5">
            {list.tabs.map((tab) => (
              <Button
                key={tab.url}
                size="xs"
                variant="outline"
                disabled={!onOpenLink}
                onClick={() => onOpenLink?.(tab.url)}
              >
                {tab.label}
                {tab.count != null && (
                  <span className="text-muted-foreground tabular-nums">
                    {videoCount(tab.count, tab.more, noun)}
                  </span>
                )}
              </Button>
            ))}
          </div>
        </div>
      )}

      {list.entries.length > 0 && (
        <ol className="flex flex-col gap-0.5 rounded-md border bg-background/40 px-2.5 py-2 text-xs">
          {list.entries.map((entry, i) => (
            <li key={i} className="flex min-w-0 items-baseline gap-2">
              <span className="w-4 shrink-0 text-right text-muted-foreground tabular-nums">
                {i + 1}
              </span>
              <span className="min-w-0 flex-1 truncate" title={entry.title}>
                {entry.title}
              </span>
              {entry.duration != null && (
                <span className="shrink-0 font-mono text-muted-foreground tabular-nums">
                  {clock(entry.duration)}
                </span>
              )}
            </li>
          ))}
          {unseen > 0 && (
            <li className="pl-6 text-muted-foreground">
              and {unseen}
              {list.more ? "+" : ""} more
            </li>
          )}
        </ol>
      )}
    </div>
  );
}
