import { Image as ImageIcon } from "lucide-react";
import { useState } from "react";
import { useIntakeSink } from "@/lib/intake";
import { useJobs } from "@/lib/jobs";
import { expandPaths, pickFiles, pickFolder, skippedNotice } from "@/lib/tauri";
import { ImageSaverPanel, useImageSaver } from "@/components/image-saver";
import { Thumb } from "@/components/thumb";
import { Button } from "@/components/ui/button";
import {
  DropZone,
  FlowFrame,
  FlowProgress,
  OptionCards,
} from "@/screens/simple/flow";

/** Chosen images shown as previews before "+N more". */
const PREVIEWS = 8;

type Task = "png" | "strip" | "webp";

const TASKS: Record<Task, { module: string; label: string; body: string }> = {
  png: {
    module: "png",
    label: "Make Premiere-ready",
    body: "WebP, AVIF, HEIC (iPhone photos) and SVG become PNGs Premiere imports. No loss, transparency kept.",
  },
  strip: {
    module: "strip-image",
    label: "Remove hidden info",
    body: "Takes out location, camera details and AI labels. The picture itself isn't changed.",
  },
  webp: {
    module: "webp",
    label: "Make smaller (WebP)",
    body: "A WebP about a third smaller, with the hidden info removed. Transparency kept.",
  },
};

/** Simple mode: choose images (or a folder of them), choose what to do, press the button. */
export function ImagesFlow() {
  const { start } = useJobs();
  const [paths, setPaths] = useState<string[]>([]);
  const [task, setTask] = useState<Task>("png");
  const [ids, setIds] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const saver = useImageSaver();

  const load = async (raw: string[]) => {
    if (raw.length === 0) return;
    setIds(null);
    const { files: found, skipped } = await expandPaths(raw, "image").catch(
      () => ({ files: [], skipped: 0 }),
    );
    if (found.length === 0) {
      setPaths([]);
      setError(
        "There are no images (PNG, JPG, WebP, AVIF, HEIC, SVG, TIFF or BMP) in what you chose.",
      );
      return;
    }
    setError(null);
    setNotice(skippedNotice(found.length, skipped));
    setPaths(found);
    // Offer the fix Premiere needs when something it can't open is among them.
    setTask(
      found.some((f) => /\.(webp|avif|heic|heif|svg|jxl)$/i.test(f))
        ? "png"
        : "strip",
    );
  };

  useIntakeSink(({ paths: dropped, urls, images }) => {
    if (dropped.length > 0) void load(dropped);
    urls.forEach((url) => void saver.saveLink(url));
    images.forEach((image) => void saver.savePaste(image));
  });

  const browse = async () => load(await pickFiles("image").catch(() => []));
  const browseFolder = async () => {
    const dir = await pickFolder().catch(() => null);
    if (dir) await load([dir]);
  };

  const run = async () => {
    setError(null);
    const started = await start(
      TASKS[task].module,
      paths,
      task === "webp"
        ? { quality: 85, lossless: false, strip: true }
        : undefined,
    );
    if (started.length === 0) setError("Couldn't start. Try again.");
    else setIds(started);
  };

  const again = () => {
    setPaths([]);
    setIds(null);
  };

  if (ids) {
    return (
      <FlowFrame
        title="Your images"
        intro="Results are saved in a Toolkit folder next to the originals. The originals aren't touched."
        step={3}
        steps={3}
      >
        <FlowProgress
          ids={ids}
          onChange={setIds}
          onAgain={again}
          againLabel="Do more images"
        />
      </FlowFrame>
    );
  }

  return (
    <FlowFrame
      title="Images"
      intro="Save a picture from the web, or drop images or a folder to make them Premiere-ready or remove hidden info."
      step={paths.length > 0 ? 2 : 1}
      steps={3}
    >
      {paths.length === 0 ? (
        <DropZone
          icon={ImageIcon}
          title="Drop images or a folder anywhere on this window"
          body="PNG, JPG, WebP, AVIF, HEIC, SVG, TIFF and BMP. A folder does every image inside it."
          chooseLabel="Choose images"
          onChoose={browse}
          onChooseFolder={browseFolder}
        />
      ) : (
        <div className="flex flex-col gap-5 rounded-2xl border bg-card p-5">
          <div className="flex flex-col gap-2.5">
            <p className="text-[15px] font-semibold tabular-nums">
              {paths.length === 1 ? "1 image" : `${paths.length} images`} ready
            </p>
            <div className="flex flex-wrap gap-2">
              {paths.slice(0, PREVIEWS).map((path) => (
                <Thumb
                  key={path}
                  path={path}
                  kind="image"
                  className="size-16"
                />
              ))}
              {paths.length > PREVIEWS && (
                <span className="grid size-16 place-items-center rounded-[9px] bg-accent text-xs text-muted-foreground tabular-nums">
                  +{paths.length - PREVIEWS}
                </span>
              )}
            </div>
          </div>
          <OptionCards<Task>
            label="What do you want to do?"
            value={task}
            onChange={setTask}
            options={(Object.keys(TASKS) as Task[]).map((t) => ({
              value: t,
              title: TASKS[t].label,
              body: TASKS[t].body,
            }))}
          />
          <div className="flex flex-wrap items-center gap-3 border-t pt-4">
            <Button
              size="lg"
              className="h-11 rounded-[10px] px-6 text-[14px]"
              onClick={run}
            >
              {paths.length === 1 ? "Do it" : `Do all ${paths.length}`}
            </Button>
            <Button size="lg" variant="ghost" onClick={again}>
              Choose different images
            </Button>
          </div>
        </div>
      )}
      {error && <p className="text-[13px] text-status-error">{error}</p>}
      {notice && <p className="text-[13px] text-status-warn">{notice}</p>}

      {paths.length === 0 && (
        <div className="flex flex-col gap-3 rounded-2xl border bg-card p-5">
          <span className="text-[15px] font-semibold">
            Save a picture from the web
          </span>
          <ImageSaverPanel saver={saver} />
        </div>
      )}
    </FlowFrame>
  );
}
