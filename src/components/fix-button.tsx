import { LoaderCircle, LogIn, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { useJobs } from "@/lib/jobs";
import {
  retryJob,
  updateYtDlp,
  type Job,
  type SettingsSection,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";

const LABELS: Record<SettingsSection, string> = {
  signin: "Set up sign-in",
  downloads: "Choose another folder",
  encoding: "Open encoding settings",
  performance: "Open performance settings",
  tools: "Check the tools",
  updates: "Check for updates",
};

/** The one button the backend says would help with a job's error, if any. */
export function FixButton({
  job,
  onStarted,
}: {
  job: Job;
  onStarted?: (ids: string[]) => void;
}) {
  const { openSettings } = useAppNav();
  const { start } = useJobs();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fix = job.fix;
  if (!fix) return null;

  if (fix.kind === "updateAndRetry") {
    return (
      <>
        <Button
          size="xs"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              // The retry runs with the newest yt-dlp, or the one in use if it already is.
              await updateYtDlp();
              onStarted?.(await retryJob(job.id));
            } catch (e) {
              setError(String(e));
              setBusy(false);
            }
          }}
        >
          {busy ? (
            <LoaderCircle className="animate-spin motion-reduce:animate-none" />
          ) : (
            <RefreshCw />
          )}
          {busy ? "Updating…" : "Update and retry"}
        </Button>
        {error && (
          <span className="basis-full text-xs break-words text-status-error">
            {error}
          </span>
        )}
      </>
    );
  }

  if (fix.kind === "retryWithSignIn") {
    return (
      <Button
        size="xs"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          const ids = await start(job.module, job.inputs, {
            ...(job.options as object),
            signIn: true,
          });
          onStarted?.(ids);
        }}
      >
        <LogIn />
        Retry with my sign-in
      </Button>
    );
  }
  return (
    <Button
      size="xs"
      variant="outline"
      onClick={() => openSettings(fix.section)}
    >
      {LABELS[fix.section]}
    </Button>
  );
}
