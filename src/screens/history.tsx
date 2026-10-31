import { ACTIVE, useJobs } from "@/lib/jobs";
import { JobCard } from "@/components/job-card";
import { PAGE_FRAME } from "@/lib/page";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

export function History() {
  const { jobs, clearFinished, error } = useJobs();
  const finished = jobs.filter((j) => !ACTIVE.has(j.status)).length;

  return (
    <div className={cn(PAGE_FRAME, "gap-4")}>
      <div className="flex items-end justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-xl font-semibold tracking-tight">History</h1>
          <p className="text-muted-foreground">
            Every job, newest first. Open one to see the exact command it ran and its log.
          </p>
        </div>
        {finished > 0 && (
          <Button variant="outline" size="sm" onClick={clearFinished}>
            Clear finished
          </Button>
        )}
      </div>
      {error && <p className="text-[13px] text-status-error">{error}</p>}
      {jobs.length === 0 ? (
        <p className="rounded-lg border border-dashed px-4 py-10 text-center text-[13px] text-muted-foreground">
          No jobs yet. Finished downloads and conversions show up here, with their files, commands and logs.
        </p>
      ) : (
        <div className="flex flex-col gap-2">
          {jobs.map((job) => (
            <JobCard key={job.id} job={job} roomy />
          ))}
        </div>
      )}
      <p className="text-xs text-muted-foreground">Clearing history never deletes the files jobs made.</p>
    </div>
  );
}
