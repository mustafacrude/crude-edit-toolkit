import {
  ChevronDown,
  Copy,
  FolderOpen,
  Play,
  RotateCcw,
  X,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ACTIVE } from "@/lib/jobs";
import { clock, ago, fileName } from "@/lib/format";
import {
  cancelJob,
  jobLog,
  openJobOutput,
  retryJob,
  revealJobOutput,
  type Job,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { FixButton } from "@/components/fix-button";
import { Button } from "@/components/ui/button";

const DOT: Record<Job["status"], string> = {
  queued: "bg-muted-foreground/50",
  running: "bg-progress",
  done: "bg-status-ok",
  failed: "bg-status-error",
  interrupted: "bg-status-error",
  cancelled: "bg-muted-foreground/50",
};

function statusLine(job: Job): ReactNode {
  switch (job.status) {
    case "queued":
      return job.lane === "process"
        ? "Waiting for the job ahead to finish"
        : "Waiting to start";
    case "running": {
      const parts = [job.steps[job.currentStep]?.label ?? "Working"];
      if (job.progress != null)
        parts.push(`${Math.floor(job.progress * 100)}%`);
      if (job.etaSecs != null) parts.push(`${clock(job.etaSecs)} left`);
      if (job.speed) parts.push(job.speed);
      return parts.join(" · ");
    }
    case "done": {
      const took =
        job.startedAt && job.finishedAt ? (
          <>
            {" in "}
            <span className="font-mono">
              {clock((job.finishedAt - job.startedAt) / 1000)}
            </span>
          </>
        ) : null;
      const files =
        job.outputs.length > 1 ? ` · ${job.outputs.length} files` : "";
      return (
        <>
          <span className="font-medium text-status-ok">Done</span>
          {took}
          {files} · {ago(job.finishedAt ?? job.createdAt)}
        </>
      );
    }
    case "cancelled":
      return "Cancelled";
    case "failed":
    case "interrupted":
      return job.error ?? "Failed";
  }
}

export function JobCard({ job, roomy = false }: { job: Job; roomy?: boolean }) {
  const [open, setOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const active = ACTIVE.has(job.status);
  const failed = job.status === "failed" || job.status === "interrupted";
  // A done job's error is a warning: part of a playlist didn't download.
  const warning = job.status === "done" ? job.error : null;
  const title = job.outputs.length === 1 ? fileName(job.outputs[0]) : job.title;

  const act = (fn: () => Promise<unknown>) => () => {
    setActionError(null);
    fn().catch((e) => setActionError(String(e)));
  };

  return (
    <div
      className={cn(
        "flex min-w-0 flex-col gap-2 rounded-[10px] border",
        failed ? "border-status-error/25 bg-status-error/[0.06]" : "bg-card",
        roomy ? "p-3.5" : "p-3",
      )}
    >
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex items-start gap-2 rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        {!failed && (
          <span
            className={cn(
              "mt-1.5 size-2 shrink-0 rounded-full",
              warning ? "bg-status-warn" : DOT[job.status],
            )}
            aria-hidden
          />
        )}
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate text-[13px] font-medium" title={title}>
            {title}
          </span>
          <span
            className={cn(
              "selectable text-xs tabular-nums",
              failed ? "break-words text-status-error" : "text-faint",
              !roomy && !failed && job.status !== "running" && "truncate",
            )}
          >
            {statusLine(job)}
          </span>
        </span>
        <ChevronDown
          className={cn(
            "mt-0.5 size-4 shrink-0 text-muted-foreground",
            open && "rotate-180",
          )}
          aria-label={open ? "Hide details" : "Show details"}
        />
      </button>

      {job.status === "running" && <ProgressBar value={job.progress} />}
      {warning && (
        <p className="text-xs break-words text-status-warn">{warning}</p>
      )}

      <div className="flex flex-wrap gap-1.5 empty:hidden">
        {active && (
          <Button
            variant="ghost"
            size="xs"
            onClick={act(() => cancelJob(job.id))}
          >
            <X />
            Cancel
          </Button>
        )}
        {job.status === "done" && job.outputs.length === 1 && (
          <Button
            variant="default"
            size="xs"
            onClick={act(() => openJobOutput(job.id))}
          >
            <Play />
            Open
          </Button>
        )}
        {job.status === "done" && job.outputs.length > 0 && (
          <Button
            variant="outline"
            size="xs"
            onClick={act(() => revealJobOutput(job.id))}
          >
            <FolderOpen />
            Show in folder
          </Button>
        )}
        {(failed ||
          job.status === "cancelled" ||
          (warning && job.outputs.length > 0)) && (
          <Button
            variant="outline"
            size="xs"
            onClick={act(() => retryJob(job.id))}
          >
            <RotateCcw />
            Retry
          </Button>
        )}
        {(failed || warning) && <FixButton job={job} />}
      </div>

      {actionError && (
        <p className="text-xs text-status-error">{actionError}</p>
      )}
      {open && <JobDetails job={job} />}
    </div>
  );
}

function ProgressBar({ value }: { value: number | null }) {
  return (
    <div
      className="h-1 overflow-hidden rounded-full bg-accent"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value == null ? undefined : Math.round(value * 100)}
    >
      {value == null ? (
        <div className="h-full w-1/3 animate-pulse rounded-full bg-progress/60 motion-reduce:animate-none" />
      ) : (
        <div
          className="h-full rounded-full bg-progress"
          style={{ width: `${Math.max(2, value * 100)}%` }}
        />
      )}
    </div>
  );
}

export function JobDetails({ job }: { job: Job }) {
  const [log, setLog] = useState("");
  const logRef = useRef<HTMLPreElement>(null);
  const running = ACTIVE.has(job.status);

  // Re-read the log while the job runs; once more when it finishes.
  useEffect(() => {
    let alive = true;
    const load = () =>
      jobLog(job.id)
        .then((text) => alive && setLog(text))
        .catch(() => {});
    load();
    const timer = running ? setInterval(load, 1000) : undefined;
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [job.id, running]);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log]);

  return (
    <div className="flex flex-col gap-2 border-t pt-2">
      {job.inputs[0] && (
        <p
          className="truncate font-mono text-[11px] text-muted-foreground selectable"
          title={job.inputs[0]}
        >
          From {job.inputs[0]}
        </p>
      )}
      {job.outputs.slice(0, 12).map((out) => (
        <p
          key={out}
          className="truncate font-mono text-[11px] text-muted-foreground selectable"
          title={out}
        >
          Saved to {out}
        </p>
      ))}
      {job.outputs.length > 12 && (
        <p className="font-mono text-[11px] text-muted-foreground">
          and {job.outputs.length - 12} more
        </p>
      )}
      {job.steps.map((step, i) => (
        <CommandBox key={i} command={step.command} />
      ))}
      <div className="flex flex-col gap-1">
        <span className="text-[11px] font-medium text-muted-foreground">
          Log
        </span>
        <pre
          ref={logRef}
          className="max-h-48 overflow-auto rounded-md border bg-background p-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all text-muted-foreground selectable"
        >
          {log || (running ? "Waiting for output…" : "No log for this job.")}
        </pre>
      </div>
      {job.status === "done" && (
        <Button
          variant="ghost"
          size="xs"
          className="self-start"
          onClick={() => retryJob(job.id).catch(() => {})}
        >
          <RotateCcw />
          Run again
        </Button>
      )}
    </div>
  );
}

function CommandBox({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-medium text-muted-foreground">
          Command
        </span>
        <Button variant="ghost" size="xs" onClick={copy}>
          <Copy />
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <pre className="overflow-x-auto rounded-md border bg-background p-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all selectable">
        {command}
      </pre>
    </div>
  );
}
