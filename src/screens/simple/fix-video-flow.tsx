import { Check, Film, LoaderCircle, Wrench } from "lucide-react";
import { useRef, useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { bytes, clock } from "@/lib/format";
import { useIntakeSink } from "@/lib/intake";
import { useJobs } from "@/lib/jobs";
import {
  expandPaths,
  inspectFile,
  pickFiles,
  pickFolder,
  skippedNotice,
  type Inspection,
} from "@/lib/tauri";
import { Thumb } from "@/components/thumb";
import { Button } from "@/components/ui/button";
import { DropZone, FlowFrame, FlowProgress } from "@/screens/simple/flow";

type Row =
  | { path: string; name: string; state: "loading" }
  | { path: string; name: string; state: "ready"; inspection: Inspection }
  | { path: string; name: string; state: "error"; error: string };

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** The one fix this file should get, or null when Premiere can use it as it is. */
const fixFor = (ins: Inspection) =>
  ins.kind === "video"
    ? (ins.suggestions.find((s) => s.recommended) ?? null)
    : null;

/** Simple mode: choose a video, see what's wrong in plain words, press one button. */
export function FixVideoFlow() {
  const { navigate } = useAppNav();
  const { start } = useJobs();
  const [rows, setRows] = useState<Row[]>([]);
  const [ids, setIds] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const generation = useRef(0);

  const load = async (raw: string[]) => {
    if (raw.length === 0) return;
    const mine = ++generation.current;
    setIds(null);
    setError(null);
    const expanded = await expandPaths(raw).catch(() => ({
      files: raw,
      skipped: 0,
    }));
    const paths = expanded.files;
    if (generation.current !== mine) return;
    setNotice(skippedNotice(paths.length, expanded.skipped));
    if (paths.length === 0) {
      setRows([]);
      setError("There are no videos, images or audio files in what you chose.");
      return;
    }
    setRows(
      paths.map((path) => ({
        path,
        name: fileName(path),
        state: "loading" as const,
      })),
    );
    // A few at a time, so a big folder doesn't start dozens of checks at once.
    const queue = [...paths];
    const worker = async () => {
      for (let path = queue.shift(); path; path = queue.shift()) {
        const row = await inspectFile(path).then(
          (inspection): Row => ({
            path,
            name: fileName(path),
            state: "ready",
            inspection,
          }),
          (e): Row => ({
            path,
            name: fileName(path),
            state: "error",
            error: String(e),
          }),
        );
        if (generation.current !== mine) return;
        setRows((prev) => prev.map((r) => (r.path === path ? row : r)));
      }
    };
    await Promise.all([worker(), worker(), worker()]);
  };

  useIntakeSink(({ paths }) => {
    if (paths.length > 0) void load(paths);
  });

  const browse = async () => load(await pickFiles("video").catch(() => []));
  const browseFolder = async () => {
    const dir = await pickFolder().catch(() => null);
    if (dir) await load([dir]);
  };

  const checking = rows.some((r) => r.state === "loading");
  const fixes = rows.flatMap((r) =>
    r.state === "ready" && fixFor(r.inspection)
      ? [{ row: r, fix: fixFor(r.inspection)! }]
      : [],
  );

  const run = async () => {
    setError(null);
    // One job set per tool, so each file gets the fix it needs.
    const byModule = new Map<string, string[]>();
    for (const { row, fix } of fixes)
      byModule.set(fix.module, [...(byModule.get(fix.module) ?? []), row.path]);
    const started = (
      await Promise.all(
        [...byModule].map(([module, paths]) => start(module, paths)),
      )
    ).flat();
    if (started.length === 0) setError("Couldn't start. Try again.");
    else setIds(started);
  };

  const again = () => {
    generation.current++;
    setRows([]);
    setIds(null);
  };

  if (ids) {
    return (
      <FlowFrame
        title="Your video"
        intro="The fixed copy is saved next to the original. The original isn't touched."
        step={3}
        steps={3}
      >
        <FlowProgress
          ids={ids}
          onChange={setIds}
          onAgain={again}
          againLabel="Fix another video"
        />
      </FlowFrame>
    );
  }

  return (
    <FlowFrame
      title="Fix a video for Premiere"
      intro="Drop a video here, or choose one. We'll check it and fix whatever would give Premiere trouble."
      step={rows.length > 0 ? 2 : 1}
      steps={3}
    >
      {rows.length === 0 ? (
        <DropZone
          icon={Film}
          title="Drop a video anywhere on this window"
          body="MKV, MP4, MOV, WebM and more. A phone recording, a download, an OBS file."
          chooseLabel="Choose a video"
          onChoose={browse}
          onChooseFolder={browseFolder}
        />
      ) : (
        <div className="flex flex-col gap-3">
          {rows.map((row) => (
            <FileRow
              key={row.path}
              row={row}
              onOpenImages={() => navigate("images")}
            />
          ))}
        </div>
      )}
      {error && <p className="text-[13px] text-status-error">{error}</p>}
      {notice && <p className="text-[13px] text-status-warn">{notice}</p>}

      {rows.length > 0 && (
        <div className="flex flex-wrap items-center gap-3 border-t pt-4">
          {fixes.length > 0 && (
            <Button
              size="lg"
              className="h-11 rounded-[10px] px-6 text-[14px]"
              onClick={run}
              disabled={checking}
            >
              <Wrench />
              {fixes.length === 1 ? "Fix it" : `Fix these ${fixes.length}`}
            </Button>
          )}
          <Button
            size="lg"
            variant={fixes.length > 0 ? "ghost" : "outline"}
            onClick={again}
          >
            {checking ? "Cancel" : "Choose different files"}
          </Button>
          {!checking &&
            fixes.length === 0 &&
            rows.some((r) => r.state === "ready") && (
              <span className="text-[13px] text-muted-foreground">
                Nothing here needs fixing.
              </span>
            )}
        </div>
      )}
    </FlowFrame>
  );
}

function FileRow({
  row,
  onOpenImages,
}: {
  row: Row;
  onOpenImages: () => void;
}) {
  const ins = row.state === "ready" ? row.inspection : null;
  return (
    <div className="flex flex-wrap gap-4 rounded-2xl border bg-card p-4">
      <Thumb
        path={row.path}
        kind={ins?.kind ?? "video"}
        duration={ins?.kind === "video" ? ins.duration : null}
        className={ins?.kind === "video" || !ins ? "w-40" : undefined}
      />
      <div className="flex min-w-0 flex-[1_1_300px] flex-col gap-2">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span
            className="truncate font-mono text-[14px] font-semibold selectable"
            title={row.path}
          >
            {row.name}
          </span>
          {ins && (
            <span className="text-xs text-faint tabular-nums">
              {[
                ins.headline,
                ins.duration ? clock(ins.duration) : null,
                ins.sizeBytes ? bytes(ins.sizeBytes) : null,
              ]
                .filter(Boolean)
                .join(" · ")}
            </span>
          )}
        </div>
        {row.state === "loading" && (
          <p className="flex items-center gap-2 text-[13px] text-muted-foreground">
            <LoaderCircle
              className="size-3.5 animate-spin motion-reduce:animate-none"
              aria-hidden
            />
            Checking what's inside…
          </p>
        )}
        {row.state === "error" && (
          <p className="text-[13px] break-words text-status-error">
            {row.error}
          </p>
        )}
        {ins && <ReadyBody inspection={ins} onOpenImages={onOpenImages} />}
      </div>
    </div>
  );
}

function ReadyBody({
  inspection: ins,
  onOpenImages,
}: {
  inspection: Inspection;
  onOpenImages: () => void;
}) {
  if (ins.kind === "image") {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <p className="text-[13px] text-muted-foreground">
          That's an image, not a video.
        </p>
        <Button size="xs" variant="outline" onClick={onOpenImages}>
          Go to Images instead
        </Button>
      </div>
    );
  }
  if (ins.kind === "audio") {
    return (
      <p className="text-[13px] text-muted-foreground">
        That's an audio file. Premiere opens these as they are.
      </p>
    );
  }
  const fix = fixFor(ins);
  const problems = ins.findings.filter((f) => f.severity === "warn");
  return (
    <>
      {problems.length > 0 && (
        <ul className="flex flex-col gap-1">
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
      )}
      {fix ? (
        <div className="flex flex-col gap-0.5 rounded-[10px] bg-primary-soft px-3 py-2.5">
          <span className="text-[11px] font-semibold tracking-[0.02em] text-primary-ink">
            We'll fix it
          </span>
          <span className="text-[13px]">{fix.reason}</span>
        </div>
      ) : (
        <p className="flex items-center gap-2 rounded-[10px] bg-status-ok/10 px-3 py-2.5 text-[13px] font-medium text-status-ok">
          <Check className="size-4" aria-hidden />
          This one is fine for Premiere as it is.
        </p>
      )}
    </>
  );
}
