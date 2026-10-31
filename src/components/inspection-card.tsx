import {
  Check,
  ChevronDown,
  Download as DownloadIcon,
  Image as ImageIcon,
  ImageDown,
  Link2,
  LoaderCircle,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { bytes, clock, fileName, siteName } from "@/lib/format";
import type { IntakeItem } from "@/lib/intake";
import { useJobs } from "@/lib/jobs";
import {
  fetchLinkInfo,
  isImageLink,
  linkNoun,
  linkOptions,
  saveImageLink,
  savePastedImage,
  PREMIERE_SAFE_IF_NEEDED,
  videoCount,
  type DownloadOptions,
  type Inspection,
  type LinkInfo,
  type SavedImage,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { SavedImageRow } from "@/components/image-saver";
import { Chip, LinkSummary } from "@/components/link-summary";
import { Thumb } from "@/components/thumb";
import { Button } from "@/components/ui/button";

export function IntakeCard({
  item,
  onRemove,
}: {
  item: IntakeItem;
  onRemove: () => void;
}) {
  if (item.type === "link") {
    return isImageLink(item.url) ? (
      <ImageCard
        label={item.url}
        save={() => saveImageLink(item.url)}
        onRemove={onRemove}
      />
    ) : (
      <LinkCard url={item.url} onRemove={onRemove} />
    );
  }
  if (item.type === "image") {
    return (
      <ImageCard
        label="Pasted image"
        save={() => savePastedImage(item.image)}
        onRemove={onRemove}
      />
    );
  }
  if (item.status === "loading") {
    return (
      <Shell onRemove={onRemove}>
        <Title name={fileName(item.path)} />
        <p className="flex items-center gap-2 text-[13px] text-muted-foreground">
          <LoaderCircle
            className="size-3.5 animate-spin motion-reduce:animate-none"
            aria-hidden
          />
          Checking what's inside…
        </p>
      </Shell>
    );
  }
  if (item.status === "error") {
    return (
      <Shell onRemove={onRemove}>
        <Title name={fileName(item.path)} />
        <p className="text-[13px] text-status-error">{item.error}</p>
      </Shell>
    );
  }
  return <InspectionView inspection={item.inspection} onRemove={onRemove} />;
}

/** A pasted link: fetches its details, then offers a one-click download with the defaults. */
function LinkCard({ url, onRemove }: { url: string; onRemove: () => void }) {
  const { openDownload } = useAppNav();
  const { start } = useJobs();
  const [info, setInfo] = useState<LinkInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [queued, setQueued] = useState(false);

  useEffect(() => {
    let alive = true;
    fetchLinkInfo(url, false)
      .then((i) => alive && setInfo(i))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [url]);

  const download = async () => {
    if (!info) return;
    setQueued(true);
    await start("download", [url], {
      ...(info.audioOnly
        ? { quality: "audio" }
        : { quality: "premiere", ...PREMIERE_SAFE_IF_NEEDED }),
      ...linkOptions(info),
    } satisfies DownloadOptions);
  };

  // One click is fine for a playlist of known size; a channel, or a list too
  // long to count, goes to Download to choose how many.
  const list = info?.playlist ?? null;
  const oneClick =
    info != null &&
    (list == null ||
      (info.kind === "playlist" && !list.more && list.count != null));
  const label = !info
    ? "Download"
    : list
      ? `Download all ${videoCount(list.count, list.more, linkNoun(info)).toLowerCase()}`
      : info.bestH264
        ? `Download ${info.bestH264}`
        : "Download";

  return (
    <Shell
      onRemove={onRemove}
      thumb={
        <Thumb
          url={info?.thumbnail}
          kind={info?.audioOnly ? "audio" : "video"}
          duration={info?.duration}
        />
      }
    >
      <div className="flex min-w-0 items-center gap-1.5 text-xs text-faint">
        <Link2 className="size-3.5 shrink-0" aria-hidden />
        <span className="truncate selectable" title={url}>
          {info ? `${siteName(info.site) ?? "Link"} · ${url}` : url}
        </span>
      </div>
      {!info && !error && (
        <p className="flex items-center gap-2 text-[13px] text-muted-foreground">
          <LoaderCircle
            className="size-3.5 animate-spin motion-reduce:animate-none"
            aria-hidden
          />
          Looking up the link…
        </p>
      )}
      {error && <p className="text-[13px] text-status-error">{error}</p>}
      {info && <LinkSummary info={info} onOpenLink={openDownload} />}
      <div className="flex flex-wrap gap-2">
        {oneClick && (
          <Button size="sm" onClick={download} disabled={queued}>
            {queued ? <Check /> : <DownloadIcon />}
            {queued ? "Added to queue" : label}
          </Button>
        )}
        {!(list && list.tabs.length > 0) && (
          <Button
            size="sm"
            variant={info && !oneClick ? "default" : "outline"}
            onClick={() => openDownload(url)}
          >
            {error
              ? "Open in Download"
              : info && !oneClick
                ? `Choose ${linkNoun(info)}s`
                : "More options"}
          </Button>
        )}
      </div>
    </Shell>
  );
}

/** A pasted picture, or a link to one: one click saves it, ready for Premiere. */
function ImageCard({
  label,
  save,
  onRemove,
}: {
  label: string;
  save: () => Promise<SavedImage>;
  onRemove: () => void;
}) {
  const [state, setState] = useState<"idle" | "saving" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<SavedImage | null>(null);
  const run = async () => {
    setState("saving");
    setError(null);
    try {
      setSaved(await save());
      setState("idle");
    } catch (e) {
      setError(String(e));
      setState("error");
    }
  };
  return (
    <Shell
      onRemove={onRemove}
      thumb={<Thumb url={/^https?:/.test(label) ? label : null} kind="image" />}
    >
      <div className="flex min-w-0 items-center gap-2">
        <ImageIcon
          className="size-4 shrink-0 text-muted-foreground"
          aria-hidden
        />
        <span
          className="truncate font-mono text-xs text-muted-foreground selectable"
          title={label}
        >
          {label}
        </span>
      </div>
      {saved ? (
        <SavedImageRow image={saved} />
      ) : (
        <>
          <p className="text-[13px] text-muted-foreground">
            Saves the full-size picture to your downloads' Images folder, as a
            PNG if Premiere can't open it.
          </p>
          {error && (
            <p className="text-[13px] break-words text-status-error">{error}</p>
          )}
          <Button
            size="sm"
            className="self-start"
            onClick={run}
            disabled={state === "saving"}
          >
            {state === "saving" ? (
              <LoaderCircle className="animate-spin motion-reduce:animate-none" />
            ) : (
              <ImageDown />
            )}
            {state === "error" ? "Try again" : "Save image"}
          </Button>
        </>
      )}
    </Shell>
  );
}

/** A card on Home: the picture on the left (when there is one), the content beside it. */
function Shell({
  children,
  onRemove,
  thumb,
}: {
  children: React.ReactNode;
  onRemove: () => void;
  thumb?: React.ReactNode;
}) {
  return (
    <div className="relative flex flex-wrap gap-x-4.5 gap-y-3 rounded-[14px] border bg-card p-4 pr-11">
      {thumb}
      <div className="flex min-w-0 flex-[1_1_320px] flex-col gap-2.5">
        {children}
      </div>
      <Button
        variant="ghost"
        size="icon-xs"
        className="absolute top-3 right-3 text-muted-foreground"
        aria-label="Dismiss"
        onClick={onRemove}
      >
        <X />
      </Button>
    </div>
  );
}

function Title({ name, meta }: { name: string; meta?: string }) {
  return (
    <div className="flex min-w-0 flex-wrap items-baseline gap-x-2">
      <span
        className="truncate font-mono text-[14px] font-semibold tracking-[-0.01em] selectable"
        title={name}
      >
        {name}
      </span>
      {meta && (
        <span className="text-xs text-muted-foreground tabular-nums">
          {meta}
        </span>
      )}
    </div>
  );
}

function InspectionView({
  inspection: ins,
  onRemove,
}: {
  inspection: Inspection;
  onRemove: () => void;
}) {
  const [showDetails, setShowDetails] = useState(false);
  const problems = ins.findings.filter((f) => f.severity === "warn");
  const facts = ins.findings.filter((f) => f.severity !== "warn");
  const recommended = ins.suggestions.find((s) => s.recommended) ?? null;
  const others = ins.suggestions.filter((s) => s !== recommended);
  const meta = [
    ins.headline,
    ins.duration ? clock(ins.duration) : null,
    ins.sizeBytes ? bytes(ins.sizeBytes) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <Shell
      onRemove={onRemove}
      thumb={
        <Thumb
          path={ins.path}
          kind={ins.kind}
          duration={ins.kind === "video" ? ins.duration : null}
        />
      }
    >
      <Title name={ins.name} meta={meta} />
      {problems.length > 0 ? (
        <ul className="flex flex-col gap-1.5">
          {problems.map((f) => (
            <li key={f.label} className="flex gap-2 text-[13px]">
              <span
                className="mt-[7px] size-1.5 shrink-0 rounded-full bg-status-warn"
                aria-hidden
              />
              <span>
                <span className="font-medium">{f.label}.</span>{" "}
                {f.detail && (
                  <span className="text-muted-foreground">{f.detail}</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="max-w-[70ch] text-[13px] text-muted-foreground">
          {ins.summary}
        </p>
      )}
      {facts.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {facts.map((f) => (
            <Chip key={f.label} title={f.detail ?? undefined}>
              {f.severity === "ok" && (
                <Check
                  className="-mt-px mr-1 inline size-3 text-status-ok"
                  aria-hidden
                />
              )}
              {f.label}
            </Chip>
          ))}
        </div>
      )}

      {recommended && (
        <Recommended
          path={ins.path}
          suggestion={recommended}
          fix={problems.length > 0}
        />
      )}
      {others.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="mr-0.5 text-xs text-faint">
            {recommended ? "Or:" : "Tools for this file:"}
          </span>
          {others.map((s) => (
            <OtherTool key={s.module} path={ins.path} suggestion={s} />
          ))}
        </div>
      )}
      {ins.suggestions.length === 0 && problems.length > 0 && (
        <p className="text-xs text-muted-foreground">
          The tools that fix this arrive in a later update.
        </p>
      )}

      <button
        type="button"
        onClick={() => setShowDetails((s) => !s)}
        aria-expanded={showDetails}
        className="flex items-center gap-1 self-start rounded-md text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <ChevronDown
          className={cn("size-3.5", showDetails && "rotate-180")}
          aria-hidden
        />
        {showDetails ? "Hide details" : "Technical details"}
      </button>
      {showDetails && (
        <dl className="grid grid-cols-[120px_1fr] gap-x-4 gap-y-1 rounded-lg border bg-background p-3 text-xs">
          {ins.details.map((d, i) => (
            <div key={i} className="contents">
              <dt className="truncate text-muted-foreground" title={d.label}>
                {d.label}
              </dt>
              <dd className="min-w-0 font-mono break-words selectable">
                {d.value}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </Shell>
  );
}

type Suggestion = Inspection["suggestions"][number];

/** The one fix worth doing, in a tinted box with the card's only accent button. */
function Recommended({
  path,
  suggestion: s,
  fix,
}: {
  path: string;
  suggestion: Suggestion;
  fix: boolean;
}) {
  const { start } = useJobs();
  const [queued, setQueued] = useState(false);
  const run = async () => {
    setQueued(true);
    await start(s.module, [path]);
  };
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-[10px] bg-primary-soft p-3">
      <div className="flex min-w-0 flex-[1_1_220px] flex-col gap-0.5">
        <span className="text-[11px] font-semibold tracking-[0.02em] text-primary-ink">
          Recommended
        </span>
        <span className="text-[13px] font-semibold">{s.name}</span>
        <span className="text-xs text-muted-foreground">{s.reason}</span>
      </div>
      <Button size="sm" disabled={queued} onClick={run}>
        {queued ? (
          <>
            <Check />
            Added to queue
          </>
        ) : fix ? (
          "Fix it"
        ) : (
          "Run"
        )}
      </Button>
    </div>
  );
}

/** Another tool that fits this file, one click away. */
function OtherTool({
  path,
  suggestion: s,
}: {
  path: string;
  suggestion: Suggestion;
}) {
  const { start } = useJobs();
  const [queued, setQueued] = useState(false);
  return (
    <Button
      size="xs"
      variant="outline"
      title={s.reason}
      disabled={queued}
      className="bg-transparent font-normal text-muted-foreground"
      onClick={async () => {
        setQueued(true);
        await start(s.module, [path]);
      }}
    >
      {queued && <Check />}
      {s.name}
    </Button>
  );
}
