import {
  Check,
  Download,
  Film,
  Image,
  LoaderCircle,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { ago, fileName } from "@/lib/format";
import { useJobs } from "@/lib/jobs";
import type { Screen } from "@/lib/nav";
import {
  isImageLink,
  openJobOutput,
  saveImageLink,
  type SavedImage,
} from "@/lib/tauri";
import { SavedImageRow } from "@/components/image-saver";
import { IntakeList } from "@/components/intake-list";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface Choice {
  screen: Screen;
  icon: LucideIcon;
  title: string;
  body: string;
}

const CHOICES: Choice[] = [
  {
    screen: "download",
    icon: Download,
    title: "Download a video or track",
    body: "YouTube, Twitch, SoundCloud and 1,800 other sites. The whole thing, or only the sound.",
  },
  {
    screen: "video",
    icon: Film,
    title: "Fix a video for Premiere",
    body: "Stops sync drift, codec errors and stuttery scrubbing. Checks first and says why.",
  },
  {
    screen: "images",
    icon: Image,
    title: "Images",
    body: "Save pictures from the web, fix WebP and iPhone photos, remove hidden info.",
  },
];

const isLink = (s: string) => /^https?:\/\/\S+$/i.test(s.trim());
/** "Recently finished" looks back this far. */
const RECENT_MS = 24 * 60 * 60 * 1000;

export function SimpleHome({
  onChoose,
}: {
  onChoose: (screen: Screen) => void;
}) {
  const { openDownload } = useAppNav();
  const { jobs } = useJobs();
  const [link, setLink] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<SavedImage | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The last few results, the newest first. A conversion that replaced a download stands for both.
  const parents = new Set(jobs.map((j) => j.parent));
  const recent = jobs
    .filter(
      (j) => j.status === "done" && j.outputs.length > 0 && !parents.has(j.id),
    )
    .filter((j) => (j.finishedAt ?? 0) > Date.now() - RECENT_MS)
    .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0))
    .slice(0, 3);

  // A video link goes to the download flow; a picture is saved straight away.
  const go = async () => {
    const url = link.trim();
    if (!isLink(url)) return;
    setError(null);
    if (!isImageLink(url)) {
      openDownload(url);
      return;
    }
    setSaving(true);
    try {
      setSaved(await saveImageLink(url));
      setLink("");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mx-auto flex w-full max-w-[880px] flex-col gap-7 pt-8">
      <IntakeList />
      <div className="flex flex-col gap-2 text-center">
        <h1 className="text-[30px] font-semibold tracking-[-0.025em]">
          What are you prepping?
        </h1>
        <p className="text-muted-foreground">
          Paste a link or drop files anywhere on this window. Everything comes
          out ready for Premiere.
        </p>
      </div>

      <form
        className="flex gap-2 rounded-[14px] border border-input bg-card p-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          void go();
        }}
      >
        <Input
          value={link}
          aria-label="Link"
          placeholder="Paste a YouTube, Twitch, SoundCloud or image link"
          className="h-11 border-0 bg-transparent text-[15px] shadow-none focus-visible:ring-0 dark:bg-transparent"
          onChange={(e) => setLink(e.target.value)}
        />
        <Button
          type="submit"
          className="h-11 rounded-[10px] px-5 text-[14px]"
          disabled={!isLink(link) || saving}
        >
          {saving && (
            <LoaderCircle className="animate-spin motion-reduce:animate-none" />
          )}
          Go
        </Button>
      </form>
      {error && <p className="-mt-4 text-[13px] text-status-error">{error}</p>}
      {saved && (
        <div className="-mt-3 rounded-[12px] border bg-card p-3.5">
          <SavedImageRow image={saved} />
        </div>
      )}

      <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3.5">
        {CHOICES.map(({ screen, icon: Icon, title, body }) => (
          <button
            key={screen}
            type="button"
            onClick={() => onChoose(screen)}
            className="flex flex-col gap-3 rounded-2xl border bg-card p-5 text-left outline-none hover:border-input hover:bg-raised focus-visible:ring-3 focus-visible:ring-ring/50"
          >
            <span className="grid size-10 place-items-center rounded-[10px] bg-primary-soft">
              <Icon className="size-5 text-primary-ink" aria-hidden />
            </span>
            <span className="flex flex-col gap-1">
              <span className="text-base font-semibold">{title}</span>
              <span className="text-[13px] leading-relaxed text-muted-foreground">
                {body}
              </span>
            </span>
          </button>
        ))}
      </div>

      {recent.length > 0 && (
        <section aria-labelledby="recent-h" className="flex flex-col gap-2">
          <h2
            id="recent-h"
            className="text-[13px] font-semibold text-muted-foreground"
          >
            Recently finished
          </h2>
          <div className="flex flex-col rounded-[12px] border bg-card">
            {recent.map((job) => {
              const name =
                job.outputs.length === 1 ? fileName(job.outputs[0]) : job.title;
              return (
                <div
                  key={job.id}
                  className="flex items-center gap-3 border-b px-3.5 py-2.5 last:border-b-0"
                >
                  <Check
                    className="size-4 shrink-0 text-status-ok"
                    strokeWidth={2.6}
                    aria-hidden
                  />
                  <span
                    className="min-w-0 flex-1 truncate text-[13px]"
                    title={name}
                  >
                    {name}
                  </span>
                  <span className="shrink-0 text-xs text-faint tabular-nums">
                    {ago(job.finishedAt ?? job.createdAt)}
                  </span>
                  {job.outputs.length === 1 && (
                    <Button
                      size="xs"
                      variant="outline"
                      onClick={() => void openJobOutput(job.id).catch(() => {})}
                    >
                      Open
                    </Button>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}
