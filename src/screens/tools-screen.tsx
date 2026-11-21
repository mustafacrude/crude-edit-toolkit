import { Check, FilePlus2, FolderOpen, Play, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { fileName } from "@/lib/format";
import { useIntakeSink } from "@/lib/intake";
import { useJobs } from "@/lib/jobs";
import { useModules } from "@/lib/modules";
import { PAGE_FRAME } from "@/lib/page";
import {
  expandPaths,
  inspectFile,
  pickFiles,
  pickFolder,
  skippedNotice,
  type ModuleInfo,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { ImageSaverPanel, useImageSaver } from "@/components/image-saver";
import { IntakeCard } from "@/components/inspection-card";
import {
  remember,
  remembered,
  toOptions,
  ToolForm,
  type FormValues,
} from "@/components/tool-form";
import { UntruncSetup, useUntrunc } from "@/components/untrunc-setup";
import { Button } from "@/components/ui/button";

const INSPECT = "inspect";
/** One line under each tool's name in the side list: what it's for, in a few words. */
const BLURBS: Record<string, string> = {
  web: "Links, Google Images, copied pictures",
  png: "WebP, AVIF, HEIC, SVG to PNG",
  "strip-image": "Location, camera, AI labels",
  webp: "Smaller files for sites",
  "premiere-safe": "Fixes codec and sync problems",
  remux: "MKV to MP4, no re-encode",
  cfr: "Stops audio drifting",
  reencode: "Your own codec settings",
  clip: "Cut out the parts you want",
  merge: "Join clips end to end",
  "strip-metadata": "Location, camera, AI labels",
  "frame-grab": "Full-size stills",
  "audio-extract": "Every track, or one",
  compress: "Hit a size, e.g. 10 MB",
  proxy: "Light copies for smooth editing",
  untrunc: "Recover a cut-off recording",
  inspect: "What's inside, in detail",
};
/** Images only: saving pictures from links and the clipboard, not files. */
const WEB = "web";
/** Chips shown before "and N more", so a big folder doesn't flood the panel. */
const CHIPS = 12;

const SCREENS = {
  video: {
    title: "Video tools",
    description:
      "Fix, convert and cut local video files. Your originals are never changed.",
    first: "premiere-safe",
    noun: ["video", "videos"],
  },
  image: {
    title: "Image tools",
    description:
      "Clean up and convert images, one at a time or a whole folder. Your originals are never changed.",
    first: WEB,
    noun: ["image", "images"],
  },
} as const;

/** The Video and Images screens: every tool in the category, each with its own form. */
export function ToolsScreen({ category }: { category: keyof typeof SCREENS }) {
  const screen = SCREENS[category];
  const { modules } = useModules();
  const tools = useMemo(
    () => modules.filter((m) => m.category === category),
    [modules, category],
  );
  const [active, setActive] = useState<string>(screen.first);
  // Files are shared across tools, so you can choose once and try several.
  const [files, setFiles] = useState<string[]>([]);
  // Set when a chosen folder had more files than one batch takes.
  const [notice, setNotice] = useState<string | null>(null);
  const tool = tools.find((t) => t.id === active);

  const add = (picked: string[]) =>
    setFiles((prev) => [...prev, ...picked.filter((p) => !prev.includes(p))]);
  const chooseFiles = async () =>
    add(await pickFiles(category).catch(() => []));
  const chooseFolder = async () => {
    const dir = await pickFolder().catch(() => null);
    if (!dir) return;
    const { files: found, skipped } = await expandPaths([dir], category).catch(
      () => ({ files: [], skipped: 0 }),
    );
    setNotice(skippedNotice(found.length, skipped));
    add(found);
  };

  return (
    <div className={cn(PAGE_FRAME, "gap-5")}>
      <div className="flex flex-col gap-1">
        <h1 className="text-xl font-semibold tracking-tight">{screen.title}</h1>
        <p className="text-muted-foreground">{screen.description}</p>
      </div>

      <div className="grid grid-cols-[230px_1fr] items-start gap-5">
        <nav className="flex flex-col gap-1" aria-label={screen.title}>
          {[
            ...(category === "image"
              ? [{ id: WEB, name: "Save from the web" }]
              : []),
            ...tools.map((t) => ({ id: t.id, name: t.name })),
            { id: INSPECT, name: "Inspect" },
          ].map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setActive(t.id)}
              aria-current={active === t.id ? "page" : undefined}
              className={cn(
                "flex flex-col gap-0.5 rounded-[10px] px-3 py-2 text-left outline-none hover:bg-card focus-visible:ring-3 focus-visible:ring-ring/50",
                "aria-[current=page]:bg-primary-soft",
              )}
            >
              <span
                className={cn(
                  "text-[13px]",
                  active === t.id
                    ? "font-semibold text-primary-ink"
                    : "font-medium",
                )}
              >
                {t.name}
              </span>
              {BLURBS[t.id] && (
                <span className="text-xs text-faint">{BLURBS[t.id]}</span>
              )}
            </button>
          ))}
        </nav>

        {active === WEB ? (
          <div className="flex min-w-0 flex-col gap-4 rounded-[14px] border bg-card p-5">
            <WebImages />
          </div>
        ) : (
          <div className="flex min-w-0 flex-col gap-4 rounded-[14px] border bg-card p-5">
            <FilePicker
              files={files}
              noun={screen.noun}
              onChooseFiles={chooseFiles}
              onChooseFolder={category === "image" ? chooseFolder : undefined}
              onChange={(next) => {
                setFiles(next);
                if (next.length === 0) setNotice(null);
              }}
              single={active === INSPECT}
              hideList={tool?.combinesInputs}
            />
            {notice && <p className="text-[13px] text-status-warn">{notice}</p>}
            {active === INSPECT ? (
              <InspectPanel path={files[0]} noun={screen.noun[0]} />
            ) : tool ? (
              <ToolPanel
                key={tool.id}
                tool={tool}
                files={files}
                noun={screen.noun[0]}
                onFilesChange={setFiles}
              />
            ) : (
              <p className="text-[13px] text-muted-foreground">
                Loading tools…
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** Save from the web: links and copied pictures pasted on this screen are saved here. */
function WebImages() {
  const saver = useImageSaver();
  useIntakeSink(({ urls, images }) => {
    urls.forEach((url) => void saver.saveLink(url));
    images.forEach((image) => void saver.savePaste(image));
  });
  return <ImageSaverPanel saver={saver} autoFocus />;
}

function FilePicker({
  files,
  noun,
  onChooseFiles,
  onChooseFolder,
  onChange,
  single,
  hideList,
}: {
  files: string[];
  noun: readonly [string, string];
  onChooseFiles: () => void;
  onChooseFolder?: () => void;
  onChange: (f: string[]) => void;
  single: boolean;
  hideList?: boolean;
}) {
  const shown = single ? files.slice(0, 1) : files.slice(0, CHIPS);
  const more = single ? 0 : files.length - shown.length;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" size="sm" onClick={onChooseFiles}>
          <FilePlus2 />
          {files.length ? `Add ${noun[1]}` : `Choose ${noun[1]}`}
        </Button>
        {onChooseFolder && (
          <Button variant="outline" size="sm" onClick={onChooseFolder}>
            <FolderOpen />
            Choose a folder
          </Button>
        )}
        {files.length > 0 && (
          <Button variant="ghost" size="sm" onClick={() => onChange([])}>
            Clear
          </Button>
        )}
        <span className="text-xs text-muted-foreground">
          {files.length === 0
            ? "Or drop files on Home, where the app suggests the right tool."
            : `${files.length} ${files.length === 1 ? noun[0] : noun[1]}${single && files.length > 1 ? " (Inspect shows the first)" : ""}`}
        </span>
      </div>
      {!hideList && shown.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          {shown.map((f) => (
            <span
              key={f}
              className="flex max-w-full items-center gap-1 rounded-md border bg-background py-0.5 pr-1 pl-2 text-xs"
              title={f}
            >
              <span className="truncate font-mono">{fileName(f)}</span>
              <button
                type="button"
                aria-label={`Remove ${fileName(f)}`}
                onClick={() => onChange(files.filter((x) => x !== f))}
                className="rounded p-0.5 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
              >
                <X className="size-3" />
              </button>
            </span>
          ))}
          {more > 0 && (
            <span className="text-xs text-muted-foreground">
              and {more} more
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function ToolPanel({
  tool,
  files,
  noun,
  onFilesChange,
}: {
  tool: ModuleInfo;
  files: string[];
  noun: string;
  onFilesChange: (f: string[]) => void;
}) {
  const { start, error } = useJobs();
  const [values, setValues] = useState<FormValues>(() => remembered(tool.id, tool.schema));
  const [queued, setQueued] = useState(false);
  // Repair needs untrunc downloaded first, and a working example chosen.
  const needsUntrunc = tool.id === "untrunc";
  const untrunc = useUntrunc();
  const missing = tool.schema.some(
    (f) => f.kind === "file" && f.required && !values[f.key],
  );
  const blocked = needsUntrunc && (!untrunc.installed || missing);

  // Picking different files or settings re-arms the button.
  useEffect(() => setQueued(false), [files, values]);
  // The settings you used last time for this tool are there next time.
  useEffect(() => remember(tool.id, tool.schema, values), [tool, values]);

  const run = async () => {
    await start(tool.id, files, toOptions(tool.schema, values, files));
    setQueued(true);
  };
  const runLabel = tool.combinesInputs
    ? `Merge ${files.length} files`
    : files.length > 1
      ? `Run on ${files.length} files`
      : "Run";

  return (
    <>
      <div className="flex flex-col gap-1">
        <h2 className="text-base font-semibold">{tool.name}</h2>
        <p className="max-w-[70ch] text-[13px] text-muted-foreground">
          {tool.description}
        </p>
      </div>
      {needsUntrunc && <UntruncSetup />}
      {tool.schema.length > 0 && (
        <ToolForm
          schema={tool.schema}
          values={values}
          onChange={setValues}
          files={files}
          onFilesChange={onFilesChange}
        />
      )}
      <div className="flex flex-wrap items-center gap-3 border-t pt-4">
        <Button
          onClick={run}
          disabled={
            files.length === 0 ||
            queued ||
            blocked ||
            (tool.combinesInputs && files.length < 2)
          }
        >
          {queued ? <Check /> : <Play />}
          {queued ? "Added to queue" : runLabel}
        </Button>
        <span className="text-xs text-muted-foreground">
          {files.length === 0
            ? `Choose a ${noun} first.`
            : tool.combinesInputs && files.length < 2
              ? "Choose at least two files."
              : needsUntrunc && !untrunc.installed
                ? "Download untrunc first."
                : blocked
                  ? "Choose a working example first."
                  : "New files go in a Toolkit folder next to each original."}
        </span>
      </div>
      {error && <p className="text-[13px] text-status-error">{error}</p>}
    </>
  );
}

/** Inspect, inline: the same card Home shows, for the first chosen file. */
function InspectPanel({
  path,
  noun,
}: {
  path: string | undefined;
  noun: string;
}) {
  const [state, setState] = useState<
    Parameters<typeof IntakeCard>[0]["item"] | null
  >(null);

  useEffect(() => {
    if (!path) {
      setState(null);
      return;
    }
    let alive = true;
    setState({ key: path, type: "file", path, status: "loading" });
    inspectFile(path)
      .then(
        (inspection) =>
          alive &&
          setState({
            key: path,
            type: "file",
            path,
            status: "ready",
            inspection,
          }),
      )
      .catch(
        (e) =>
          alive &&
          setState({
            key: path,
            type: "file",
            path,
            status: "error",
            error: String(e),
          }),
      );
    return () => {
      alive = false;
    };
  }, [path]);

  if (!state) {
    return (
      <p className="text-[13px] text-muted-foreground">
        Choose a {noun} to see what's inside it.
      </p>
    );
  }
  return <IntakeCard item={state} onRemove={() => setState(null)} />;
}
