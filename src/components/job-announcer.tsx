import { useEffect, useRef, useState } from "react";
import { useJobs } from "@/lib/jobs";
import { fileName } from "@/lib/format";

/** Says out loud (to screen readers) when a job finishes or fails; nothing is shown. */
export function JobAnnouncer() {
  const { jobs } = useJobs();
  const seen = useRef<Map<string, string>>(new Map());
  const first = useRef(true);
  const [message, setMessage] = useState("");

  useEffect(() => {
    const heard: string[] = [];
    for (const job of jobs) {
      const before = seen.current.get(job.id);
      seen.current.set(job.id, job.status);
      if (first.current || before === job.status) continue;
      const name = job.outputs.length === 1 ? fileName(job.outputs[0]) : job.title;
      if (job.status === "done") heard.push(`${name} finished.`);
      else if (job.status === "failed" || job.status === "interrupted") heard.push(`${name} failed.`);
    }
    first.current = false;
    if (heard.length > 0) setMessage(heard.join(" "));
  }, [jobs]);

  return (
    <div role="status" aria-live="polite" className="sr-only">
      {message}
    </div>
  );
}
