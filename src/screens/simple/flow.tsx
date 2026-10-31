import {
  Check,
  FolderOpen,
  Play,
  RotateCcw,
  type LucideIcon,
} from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import { clock } from "@/lib/format";
import { ACTIVE, useJobs } from "@/lib/jobs";
import {
  openJobOutput,
  retryJob,
  revealJobOutput,
  type Job,
} from "@/lib/tauri";
import { cn, radioArrows } from "@/lib/utils";
import { Thumb } from "@/components/thumb";
import { FixButton } from "@/components/fix-button";
import { Button } from "@/components/ui/button";

const VIDEO = /\.(mkv|mp4|mov|webm|m4v|avi|ts|flv)$/i;
const AUDIO = /\.(wav|mp3|m4a|aac|flac|opus|ogg|aif|aiff)$/i;
const folderOf = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

/** A guided Simple-mode screen: where you are, a title, and one thing to do. */
export function FlowFrame({
  title,
  intro,
  step,
  steps,
  children,
}: {
  title: string;
  intro: string;
  step: number;
  steps: number;
  children: ReactNode;
}) {
  return (
    <div className="mx-auto flex w-full max-w-[760px] flex-col gap-5 pt-2">
      <div className="flex flex-col gap-2.5">
        <div className="flex items-center gap-3">
          <div className="flex gap-1" aria-hidden>
            {Array.from({ length: steps }, (_, i) => (
              <span
                key={i}
                className={cn(
                  "h-1 w-8 rounded-full",
                  i < step ? "bg-primary" : "bg-accent",
                )}
              />
            ))}
          </div>
          <span className="text-xs text-faint tabular-nums">
            Step {step} of {steps}
          </span>
        </div>
        <h1 className="text-[26px] font-semibold tracking-[-0.02em]">
          {title}
        </h1>
        <p className="text-muted-foreground">{intro}</p>
      </div>
      {children}
    </div>
  );
}

/** The big dashed area a flow starts with: drop here, or choose. */
export function DropZone({
  icon: Icon,
  title,
  body,
  chooseLabel,
  onChoose,
  onChooseFolder,
}: {
  icon: LucideIcon;
  title: string;
  body: string;
  chooseLabel: string;
  onChoose: () => void;
  onChooseFolder: () => void;
}) {
  return (
    <div className="flex flex-col items-center gap-4 rounded-2xl border-[1.5px] border-dashed border-input bg-chrome px-6 py-12 text-center">
      <span className="grid size-12 place-items-center rounded-xl bg-primary-soft">
        <Icon className="size-6 text-primary-ink" aria-hidden />
      </span>
      <div className="flex flex-col gap-1">
        <span className="text-base font-semibold">{title}</span>
        <span className="text-[13px] text-muted-foreground">{body}</span>
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        <Button size="lg" onClick={onChoose}>
          {chooseLabel}
        </Button>
        <Button size="lg" variant="outline" onClick={onChooseFolder}>
          <FolderOpen />
          Choose a folder
        </Button>
      </div>
    </div>
  );
}

export interface Option<T extends string> {
  value: T;
  title: string;
  body: string;
  icon?: LucideIcon;
}

/** A choice as a row of selectable cards, with what each one does under its name. */
export function OptionCards<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: Option<T>[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-sm font-medium">{label}</span>
      <div
        role="radiogroup"
        aria-label={label}
        onKeyDown={radioArrows}
        className="grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-2.5"
      >
        {options.map(({ value: v, title, body, icon: Icon }) => {
          const on = v === value;
          return (
            <button
              key={v}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => onChange(v)}
              className={cn(
                "flex flex-col gap-1.5 rounded-[12px] border-[1.5px] p-3.5 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
                on
                  ? "border-primary bg-primary-soft"
                  : "border-border bg-background hover:border-input",
              )}
            >
              <span className="flex items-center justify-between gap-2">
                <span className="flex items-center gap-2 text-[13px] font-semibold">
                  {Icon && (
                    <Icon
                      className={cn(
                        "size-4",
                        on ? "text-primary-ink" : "text-muted-foreground",
                      )}
                      aria-hidden
                    />
                  )}
                  {title}
                </span>
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
              <span className="text-xs leading-relaxed text-muted-foreground">
                {body}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** What kind of preview a file gets, from its name. */
const kindOf = (path: string): "video" | "audio" | "image" =>
  VIDEO.test(path) ? "video" : AUDIO.test(path) ? "audio" : "image";

/**
 * The jobs a flow started, plus the jobs those started (a download's
 * Premiere-safe step), minus any the person has since retried.
 */
export function relatedJobs(
  jobs: Job[],
  ids: string[],
  superseded: ReadonlySet<string>,
): Job[] {
  const mine = new Set(ids);
  let grew = true;
  while (grew) {
    grew = false;
    for (const job of jobs) {
      if (!mine.has(job.id) && job.parent && mine.has(job.parent)) {
        mine.add(job.id);
        grew = true;
      }
    }
  }
  return jobs
    .filter(
      (j) =>
        mine.has(j.id) &&
        !superseded.has(j.id) &&
        !(j.parent && superseded.has(j.parent)),
    )
    .sort((a, b) => a.createdAt - b.createdAt);
}

/** What the person sees while a flow's work runs, and when it ends. */
export function FlowProgress({
  ids,
  onChange,
  onAgain,
  againLabel,
}: {
  ids: string[];
  onChange: (ids: string[]) => void;
  onAgain: () => void;
  againLabel: string;
}) {
  const { jobs } = useJobs();
  const [superseded, setSuperseded] = useState<ReadonlySet<string>>(new Set());
  const [actionError, setActionError] = useState<string | null>(null);
  const mine = useMemo(
    () => relatedJobs(jobs, ids, superseded),
    [jobs, ids, superseded],
  );

  // Retrying or re-running a job replaces it with new ones.
  const replace = (old: string[], fresh: string[]) => {
    setSuperseded((prev) => new Set([...prev, ...old]));
    onChange([...ids.filter((id) => !old.includes(id)), ...fresh]);
  };

  if (mine.length === 0) return <Card>Getting started…</Card>;

  const active = mine.filter((j) => ACTIVE.has(j.status));
  if (active.length > 0) {
    const running = active.find((j) => j.status === "running") ?? active[0];
    const sum = mine.reduce(
      (n, j) => n + (j.status === "done" ? 1 : (j.progress ?? 0)),
      0,
    );
    const overall = sum / mine.length;
    const finished = mine.length - active.length;
    return (
      <Card>
        <div className="flex items-baseline justify-between gap-3">
          <span className="text-[15px] font-semibold">
            {running.status === "queued"
              ? "Waiting for a turn"
              : (running.steps[running.currentStep]?.label ?? "Working")}
            …
          </span>
          <span className="text-[22px] font-semibold tracking-[-0.02em] tabular-nums">
            {Math.floor(overall * 100)}%
          </span>
        </div>
        <div
          className="h-1.5 overflow-hidden rounded-full bg-accent"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.floor(overall * 100)}
        >
          <div
            className="h-full rounded-full bg-progress"
            style={{ width: `${Math.max(2, overall * 100)}%` }}
          />
        </div>
        <p className="text-xs text-faint tabular-nums">
          {[
            mine.length > 1 ? `${finished} of ${mine.length} finished` : null,
            running.etaSecs != null ? `${clock(running.etaSecs)} left` : null,
            running.speed,
          ]
            .filter(Boolean)
            .join(" · ") || "This can take a few minutes for long videos."}
        </p>
        <p className="text-xs text-faint">
          You can keep using other apps. It runs in the background.
        </p>
      </Card>
    );
  }

  const failed = mine.filter(
    (j) => j.status === "failed" || j.status === "interrupted",
  );
  const done = mine.filter((j) => j.status === "done");
  // The files to show are the last in each chain, not a download the conversion replaced.
  const parents = new Set(mine.map((j) => j.parent));
  const results = done.filter(
    (j) => j.outputs.length > 0 && !parents.has(j.id),
  );
  const warnings = [
    ...new Set(done.map((j) => j.error).filter((e): e is string => !!e)),
  ];
  // What was saved, counted by file: a playlist is one job with many videos.
  const files = results.flatMap((j) => j.outputs);
  // Sound-only downloads count the same way, by their audio files.
  const videoCount = files.filter((f) => VIDEO.test(f)).length;
  const media = videoCount > 0 ? VIDEO : AUDIO;
  const noun = videoCount > 0 ? "videos" : "audio files";
  const videos =
    videoCount > 0 ? videoCount : files.filter((f) => AUDIO.test(f)).length;
  const others = files.length - videos;
  const folders = new Set(files.map(folderOf));
  const savedLine =
    files.length < 2
      ? null
      : videos > 1
        ? `${videos} ${noun} saved${others > 0 ? ` and ${others} other file${others === 1 ? "" : "s"}` : ""}.`
        : `${files.length} files saved.`;
  // One video (with its chat or subtitles beside it) can be opened; a batch can't.
  const openable =
    results.length === 1 &&
    (results[0].outputs.length === 1 ||
      results[0].outputs.filter((f) => media.test(f)).length === 1);
  const firstFailed = failed[0];
  const act = (fn: () => Promise<unknown>) => () => {
    setActionError(null);
    fn().catch((e) => setActionError(String(e)));
  };

  const preview = files.find((f) => media.test(f)) ?? files[0];

  return (
    <Card>
      <div className="flex flex-wrap gap-4">
        {preview && (
          <Thumb
            path={preview}
            kind={kindOf(preview)}
            className={kindOf(preview) === "video" ? "w-44" : "size-20"}
          />
        )}
        <div className="flex min-w-0 flex-[1_1_300px] flex-col gap-3">
          <div className="flex items-center gap-2">
            <span
              className={`grid size-6 place-items-center rounded-full ${failed.length > 0 ? "bg-status-error/15 text-status-error" : "bg-status-ok/15 text-status-ok"}`}
              aria-hidden
            >
              <Check className="size-3.5" />
            </span>
            <span className="text-[15px] font-semibold">
              {failed.length === 0
                ? videos > 1
                  ? `All ${videos} done`
                  : mine.length > 1
                    ? `All ${done.length} done`
                    : results.length > 0
                      ? "Done"
                      : "Finished"
                : done.length > 0
                  ? `${done.length} done, ${failed.length} couldn't be finished`
                  : failed.length > 1
                    ? `${failed.length} couldn't be finished`
                    : "That didn't work"}
            </span>
          </div>

          {firstFailed?.error && (
            <p className="text-[13px] break-words text-status-error">
              {firstFailed.error}
            </p>
          )}
          {warnings.map((w) => (
            <p key={w} className="text-[13px] break-words text-status-warn">
              {w}
            </p>
          ))}
          {savedLine && <p className="text-[13px]">{savedLine}</p>}
          {files.length > 0 && (
            <p
              className="truncate font-mono text-xs text-muted-foreground selectable"
              title={files[0]}
            >
              {files.length === 1 || videos === 1
                ? files[0]
                : folders.size === 1
                  ? `Saved in ${[...folders][0]}`
                  : files[0]}
            </p>
          )}

          <div className="flex flex-wrap gap-2">
            {openable && (
              <Button
                size="sm"
                onClick={act(() => openJobOutput(results[0].id))}
              >
                <Play />
                Open it
              </Button>
            )}
            {results.length > 0 && (
              <Button
                size="sm"
                variant={openable ? "outline" : "default"}
                onClick={act(() => revealJobOutput(results[0].id))}
              >
                <FolderOpen />
                Show in folder
              </Button>
            )}
            {failed.length > 0 && (
              <>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={async () => {
                    setActionError(null);
                    try {
                      const fresh = (
                        await Promise.all(failed.map((j) => retryJob(j.id)))
                      ).flat();
                      replace(
                        failed.map((j) => j.id),
                        fresh,
                      );
                    } catch (e) {
                      setActionError(String(e));
                    }
                  }}
                >
                  <RotateCcw />
                  Try again
                </Button>
                {firstFailed && (
                  <FixButton
                    job={firstFailed}
                    onStarted={(fresh) => replace([firstFailed.id], fresh)}
                  />
                )}
              </>
            )}
            <Button size="sm" variant="ghost" onClick={onAgain}>
              {againLabel}
            </Button>
          </div>
          {actionError && (
            <p className="text-xs text-status-error">{actionError}</p>
          )}
        </div>
      </div>
    </Card>
  );
}

function Card({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-col gap-3 rounded-2xl border bg-card p-5">
      {children}
    </div>
  );
}
