// Repair damaged video needs untrunc, which isn't bundled: it's downloaded here
// on request, and updated here when the author publishes a newer build.
import { Download, LoaderCircle, RefreshCw } from "lucide-react";
import { useState } from "react";
import { ACTIVE, useJobs } from "@/lib/jobs";
import { updateUntrunc } from "@/lib/tauri";
import { useTools } from "@/lib/tools";
import { useUpdates, withoutUntrunc } from "@/lib/updates";
import { Button } from "@/components/ui/button";

/** Whether untrunc has been downloaded, from the tool list the app already keeps. */
export function useUntrunc() {
  const { tools, loading } = useTools();
  const tool = tools.find((t) => t.id === "untrunc");
  return { installed: !!tool && !tool.missing && !tool.error, version: tool?.version ?? null, loading };
}

/** Download or update untrunc. Shown above the Repair form. */
export function UntruncSetup() {
  const { installed, version, loading } = useUntrunc();
  const { refresh } = useTools();
  const { found, setFound } = useUpdates();
  const { jobs } = useJobs();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const newer = found?.untrunc?.available ? found.untrunc : null;
  const working = jobs.some((j) => ACTIVE.has(j.status));

  const run = async () => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const now = await updateUntrunc();
      setDone(now ? "untrunc is ready." : "untrunc is already the newest build.");
      setFound(withoutUntrunc(found));
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (loading && !installed) return null;

  if (!installed) {
    return (
      <div className="flex flex-col gap-2 rounded-[10px] border bg-background p-3">
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex min-w-0 flex-[1_1_300px] flex-col gap-0.5">
            <span className="text-[13px] font-medium">First, download untrunc</span>
            <span className="max-w-[62ch] text-xs text-muted-foreground">
              This tool uses untrunc, a free program by anthwlock that isn't built into the app. It's about 13 MB,
              comes straight from its GitHub page, and is checked before it's used. You only do this once.
            </span>
          </div>
          <Button size="sm" onClick={run} disabled={busy}>
            {busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : <Download />}
            {busy ? "Downloading…" : "Download untrunc"}
          </Button>
        </div>
        {error && <p className="text-[13px] break-words text-status-error">{error}</p>}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-xs text-muted-foreground">
          {newer
            ? `A newer untrunc build is available (${newer.latest}).`
            : `untrunc ${version ?? ""} is installed.`}
        </span>
        {newer && (
          <Button size="xs" variant="outline" onClick={run} disabled={busy || working} title={working ? "Wait for the queue to finish" : undefined}>
            {busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" /> : <RefreshCw />}
            Update untrunc
          </Button>
        )}
      </div>
      {done && <p className="text-xs text-status-ok">{done}</p>}
      {error && <p className="text-[13px] break-words text-status-error">{error}</p>}
    </div>
  );
}
