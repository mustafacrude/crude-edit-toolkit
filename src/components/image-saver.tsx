// Saving pictures from the web: a link, a Google Images result, a page, or a
// copied image. Each lands in the downloads folder's Images folder, as PNG when
// Premiere couldn't open the original.
import {
  Check,
  FolderOpen,
  Image as ImageIcon,
  ImageDown,
  LoaderCircle,
  Search,
} from "lucide-react";
import { useState } from "react";
import { fileName } from "@/lib/format";
import { useSettings } from "@/lib/settings";
import {
  saveImageLink,
  savedImageAction,
  savePastedImage,
  type SavedImage,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { Chip } from "@/components/link-summary";
import { useThumbnail } from "@/components/thumb";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const isLink = (s: string) => /^https?:\/\/\S+$/i.test(s.trim());

export interface ImageSaver {
  saving: number;
  results: SavedImage[];
  error: string | null;
  saveLink: (url: string) => Promise<void>;
  savePaste: (image: Blob) => Promise<void>;
}

/** Saves links and pasted pictures, newest result first. */
export function useImageSaver(): ImageSaver {
  const [saving, setSaving] = useState(0);
  const [results, setResults] = useState<SavedImage[]>([]);
  const [error, setError] = useState<string | null>(null);

  const run = async (save: () => Promise<SavedImage>) => {
    setError(null);
    setSaving((n) => n + 1);
    try {
      const image = await save();
      setResults((prev) => [image, ...prev]);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving((n) => n - 1);
    }
  };

  return {
    saving,
    results,
    error,
    saveLink: (url) => run(() => saveImageLink(url.trim())),
    savePaste: (image) => run(() => savePastedImage(image)),
  };
}

/** The link box, plus what was saved. Pastes elsewhere on the screen are the parent's to route. */
export function ImageSaverPanel({
  saver,
  autoFocus,
}: {
  saver: ImageSaver;
  autoFocus?: boolean;
}) {
  const { settings } = useSettings();
  const [url, setUrl] = useState("");
  const sep = settings.downloadDir.includes("/") ? "/" : "\\";

  const submit = () => {
    if (!isLink(url)) return;
    void saver.saveLink(url);
    setUrl("");
  };

  return (
    <div className="flex flex-col gap-3">
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <Input
          autoFocus={autoFocus}
          value={url}
          aria-label="Image link"
          placeholder="Paste an image link or a page, or press Ctrl+V with a copied picture"
          className={cn("h-11 rounded-[10px]", url && "font-mono text-[13px]")}
          onChange={(e) => setUrl(e.target.value)}
          onPaste={(e) => {
            // A copied picture pasted straight into the box.
            const image = [...e.clipboardData.files].find((f) =>
              f.type.startsWith("image/"),
            );
            if (image && !e.clipboardData.getData("text")) {
              e.preventDefault();
              void saver.savePaste(image);
            }
          }}
        />
        <Button
          type="submit"
          className="h-11 rounded-[10px] px-4.5"
          disabled={!isLink(url)}
        >
          <ImageDown />
          Save image
        </Button>
      </form>
      <p className="-mt-1 text-xs text-faint">
        In Google Images: open the result, right-click the large preview, Copy
        image address. Anything Premiere can't open arrives as a PNG.
      </p>

      {(saver.results.length > 0 || saver.saving > 0 || saver.error) && (
        <div className="mt-1 flex items-baseline justify-between gap-3">
          <h2 className="text-[13px] font-semibold text-muted-foreground">
            Saved this session
          </h2>
          <span className="truncate font-mono text-xs text-faint selectable">
            {settings.downloadDir}
            {sep}Images
          </span>
        </div>
      )}
      {saver.error && (
        <p className="text-[13px] break-words text-status-error">
          {saver.error}
        </p>
      )}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-3.5">
        {saver.saving > 0 &&
          Array.from({ length: saver.saving }, (_, i) => (
            <div
              key={`saving-${i}`}
              className="flex min-h-56 flex-col items-center justify-center gap-2 rounded-[12px] border bg-card text-[13px] text-muted-foreground"
            >
              <LoaderCircle
                className="size-5 animate-spin motion-reduce:animate-none"
                aria-hidden
              />
              Saving…
            </div>
          ))}
        {saver.results.map((image) => (
          <SavedImageTile key={image.id} image={image} />
        ))}
      </div>
    </div>
  );
}

function useImageActions(image: SavedImage) {
  const [error, setError] = useState<string | null>(null);
  const act = (action: "open" | "reveal" | "lens") => () => {
    setError(null);
    savedImageAction(image.id, action).catch((e) => setError(String(e)));
  };
  const small =
    image.width != null &&
    image.height != null &&
    Math.max(image.width, image.height) < 1000;
  const size =
    image.width && image.height ? `${image.width}×${image.height}` : null;
  return { error, act, small, size };
}

/** A saved picture as a tile: its preview, its size and format, and what to do next. */
function SavedImageTile({ image }: { image: SavedImage }) {
  const { error, act, small, size } = useImageActions(image);
  const src = useThumbnail({ path: image.path });
  // Small pictures lead with finding a bigger copy; the rest with opening.
  const lensFirst = small && image.source != null;
  return (
    <article className="flex flex-col overflow-hidden rounded-[12px] border bg-card">
      <div
        className={cn(
          "relative grid h-36 shrink-0 place-items-center overflow-hidden text-faint",
          src ? "checker" : "bg-accent",
        )}
      >
        {src ? (
          <img
            src={src}
            alt=""
            className="absolute inset-0 size-full object-contain p-2"
          />
        ) : (
          <ImageIcon className="size-6" strokeWidth={1.6} aria-hidden />
        )}
      </div>
      <div className="flex flex-col gap-1.5 p-3">
        <span
          className="truncate text-[13px] font-medium selectable"
          title={image.path}
        >
          {fileName(image.path)}
        </span>
        <div className="flex flex-wrap gap-1.5">
          {size && (
            <Chip tone={small ? "warn" : "plain"}>
              {small ? `Small · ${size}` : size}
            </Chip>
          )}
          {image.convertedFrom && (
            <Chip tone="accent">{image.convertedFrom} → PNG</Chip>
          )}
          {!image.source && <Chip>From clipboard</Chip>}
        </div>
        {image.notes
          .filter((n) => !n.startsWith("It's only"))
          .map((note) => (
            <p key={note} className="text-xs text-status-warn">
              {note}
            </p>
          ))}
        {lensFirst && (
          <Button
            size="sm"
            className="mt-1 w-full"
            onClick={act("lens")}
            title="Opens Google Lens in your browser with this picture's address"
          >
            <Search />
            Find a bigger one
          </Button>
        )}
        <div className={cn("flex gap-1.5", !lensFirst && "mt-1")}>
          <Button
            size="sm"
            variant="outline"
            className="flex-1"
            onClick={act("open")}
          >
            Open
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="flex-1"
            onClick={act("reveal")}
          >
            <FolderOpen />
            Show
          </Button>
        </div>
        {error && <p className="text-xs text-status-error">{error}</p>}
      </div>
    </article>
  );
}

/** One saved picture in a row, for a card on Home: where it went, its size, and anything worth knowing. */
export function SavedImageRow({ image }: { image: SavedImage }) {
  const { error, act, size } = useImageActions(image);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex min-w-0 items-center gap-2">
        <Check className="size-4 shrink-0 text-status-ok" aria-hidden />
        <span
          className="truncate text-[13px] font-medium selectable"
          title={image.path}
        >
          {fileName(image.path)}
        </span>
        <span className="shrink-0 text-xs text-faint tabular-nums">
          {[size, image.convertedFrom ? `${image.convertedFrom} → PNG` : null]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </div>
      {image.notes.map((note) => (
        <p key={note} className="text-[13px] text-status-warn">
          {note}
        </p>
      ))}
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={act("open")}>
          Open it
        </Button>
        <Button size="sm" variant="outline" onClick={act("reveal")}>
          <FolderOpen />
          Show in folder
        </Button>
        {image.source && (
          <Button
            size="sm"
            variant="outline"
            onClick={act("lens")}
            title="Opens Google Lens in your browser with this picture's address, to find other copies"
          >
            <Search />
            Find a bigger version
          </Button>
        )}
      </div>
      {error && <p className="text-[13px] text-status-error">{error}</p>}
    </div>
  );
}
