import {
  Check,
  ChevronDown,
  FolderOpen,
  ListVideo,
  Play,
  X,
} from "lucide-react";
import { useState } from "react";
import { ago, fileName } from "@/lib/format";
import { ACTIVE, useJobs } from "@/lib/jobs";
import { openJobOutput, revealJobOutput, type Job } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { JobCard, JobDetails } from "@/components/job-card";
import { Button } from "@/components/ui/button";

/** Finished jobs stay in the rail this long, then live only in History. */
const RECENT_MS = 60 * 60 * 1000;

const needsAttention = (j: Job) =>
  j.status === "failed" ||
  j.status === "interrupted" ||
  (j.status === "done" && !!j.error);

export function QueueRail({
  onOpenHistory,
  onClose,
  className,
}: {
  onOpenHistory: () => void;
  /** Set when the rail is a drawer. */
  onClose?: () => void;
  className?: string;
}) {
  const { jobs, error } = useJobs();
  const now = Date.now();
  const recent = jobs
    .filter(
      (j) => ACTIVE.has(j.status) || (j.finishedAt ?? 0) > now - RECENT_MS,
    )
    .slice(0, 40);
  // Running first, then what's waiting.
  const active = recent
    .filter((j) => ACTIVE.has(j.status))
    .sort(
      (a, b) => Number(b.status === "running") - Number(a.status === "running"),
    );
  const attention = recent.filter(needsAttention);
  const done = recent.filter(
    (j) => !ACTIVE.has(j.status) && !needsAttention(j),
  );
  const running = active.filter((j) => j.status === "running").length;
  const waiting = active.length - running;
  const summary = [
    running > 0 && `${running} running`,
    waiting > 0 && `${waiting} waiting`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <aside
      className={cn(
        "flex w-80 shrink-0 flex-col border-l bg-chrome",
        className,
      )}
      aria-label="Queue"
    >
      <div className="flex items-center gap-2 px-4 pt-4 pb-2">
        <h2 className="text-[14px] font-semibold">Queue</h2>
        {summary && (
          <span className="text-xs text-faint tabular-nums">{summary}</span>
        )}
        <span className="flex-1" />
        <button
          type="button"
          onClick={onOpenHistory}
          className="rounded-md text-xs text-faint outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          History
        </button>
        {onClose && (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Close the queue"
            onClick={onClose}
            className="text-muted-foreground"
          >
            <X />
          </Button>
        )}
      </div>
      {error && <p className="mx-4 mb-2 text-xs text-status-error">{error}</p>}
      {recent.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-8 text-center">
          <ListVideo className="size-5 text-faint" aria-hidden />
          <p className="text-[13px] text-muted-foreground">
            Nothing running. Drop a file or paste a link and its jobs show up
            here.
          </p>
        </div>
      ) : (
        // A plain scroller: Radix ScrollArea sizes its content as a table, which lets
        // long file names push cards wider than the rail.
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-x-hidden overflow-y-auto px-4 pb-4">
          {active.map((job) => (
            <JobCard key={job.id} job={job} />
          ))}
          {attention.length > 0 && <GroupTitle>Needs attention</GroupTitle>}
          {attention.map((job) => (
            <JobCard key={job.id} job={job} />
          ))}
          {done.length > 0 && <GroupTitle>Done</GroupTitle>}
          {done.length > 0 && (
            <div className="flex flex-col">
              {done.map((job) => (
                <DoneRow key={job.id} job={job} />
              ))}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function GroupTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="mt-2 text-xs font-semibold text-faint">{children}</h3>;
}

/** A finished job in one line: open it, show it, or unfold the details. */
function DoneRow({ job }: { job: Job }) {
  const [open, setOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const cancelled = job.status === "cancelled";
  const title = job.outputs.length === 1 ? fileName(job.outputs[0]) : job.title;
  const act = (fn: () => Promise<unknown>) => () => {
    setActionError(null);
    fn().catch((e) => setActionError(String(e)));
  };
  return (
    <div className="flex flex-col border-b border-border/70 last:border-b-0">
      <div className="flex min-w-0 items-center gap-2 py-1.5">
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          {cancelled ? (
            <X
              className="size-3.5 shrink-0 text-faint"
              aria-label="Cancelled"
            />
          ) : (
            <Check
              className="size-3.5 shrink-0 text-status-ok"
              strokeWidth={2.6}
              aria-label="Done"
            />
          )}
          <span
            className={cn(
              "min-w-0 flex-1 truncate text-[13px]",
              cancelled && "text-muted-foreground",
            )}
            title={title}
          >
            {title}
          </span>
          <span className="shrink-0 text-xs text-faint tabular-nums">
            {ago(job.finishedAt ?? job.createdAt)}
          </span>
          <ChevronDown
            className={cn("size-3.5 shrink-0 text-faint", open && "rotate-180")}
            aria-hidden
          />
        </button>
        {!cancelled && job.outputs.length === 1 && (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`Open ${title}`}
            className="text-muted-foreground"
            onClick={act(() => openJobOutput(job.id))}
          >
            <Play />
          </Button>
        )}
        {!cancelled && job.outputs.length > 0 && (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`Show ${title} in its folder`}
            className="text-muted-foreground"
            onClick={act(() => revealJobOutput(job.id))}
          >
            <FolderOpen />
          </Button>
        )}
      </div>
      {actionError && (
        <p className="pb-1.5 text-xs text-status-error">{actionError}</p>
      )}
      {open && (
        <div className="pb-2">
          <JobDetails job={job} />
        </div>
      )}
    </div>
  );
}
