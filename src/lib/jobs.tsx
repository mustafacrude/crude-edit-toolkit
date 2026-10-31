import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  clearFinished as clearFinishedCmd,
  isTauri,
  listJobs,
  onJobUpdated,
  onJobsRemoved,
  startJobs,
  type Job,
  type JobStatus,
} from "@/lib/tauri";

export const ACTIVE: ReadonlySet<JobStatus> = new Set(["queued", "running"]);

interface JobsState {
  /** Newest first. */
  jobs: Job[];
  /** Queues the work and resolves to the new job ids (none if it failed to start). */
  start: (module: string, inputs: string[], options?: unknown) => Promise<string[]>;
  clearFinished: () => Promise<void>;
  /** Set when talking to the backend failed; shown where the jobs would be. */
  error: string | null;
}

const JobsContext = createContext<JobsState | null>(null);

export function JobsProvider({ children }: { children: ReactNode }) {
  const [byId, setById] = useState<Map<string, Job>>(new Map());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    const unlisteners: (() => void)[] = [];
    // Subscribe before loading, so no update is lost in between.
    onJobUpdated((job) => setById((prev) => new Map(prev).set(job.id, job))).then((u) => unlisteners.push(u));
    onJobsRemoved((ids) =>
      setById((prev) => {
        const next = new Map(prev);
        for (const id of ids) next.delete(id);
        return next;
      }),
    ).then((u) => unlisteners.push(u));
    listJobs()
      .then((list) =>
        setById((prev) => {
          const next = new Map(list.map((j) => [j.id, j]));
          for (const [id, job] of prev) next.set(id, job); // events that beat the list win
          return next;
        }),
      )
      .catch((e) => setError(String(e)));
    return () => unlisteners.forEach((u) => u());
  }, []);

  const start = useCallback(async (module: string, inputs: string[], options?: unknown) => {
    try {
      const ids = await startJobs(module, inputs, options);
      setError(null);
      return ids;
    } catch (e) {
      setError(String(e));
      return [];
    }
  }, []);

  const clearFinished = useCallback(async () => {
    try {
      await clearFinishedCmd();
      setById((prev) => new Map([...prev].filter(([, j]) => ACTIVE.has(j.status))));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const jobs = useMemo(() => [...byId.values()].sort((a, b) => b.createdAt - a.createdAt), [byId]);

  return <JobsContext.Provider value={{ jobs, start, clearFinished, error }}>{children}</JobsContext.Provider>;
}

export function useJobs() {
  const ctx = useContext(JobsContext);
  if (!ctx) throw new Error("useJobs must be used inside JobsProvider");
  return ctx;
}
