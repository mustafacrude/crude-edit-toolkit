import { Film, Image as ImageIcon, Music } from "lucide-react";
import { useEffect, useState } from "react";
import { clock } from "@/lib/format";
import { thumbnailOf } from "@/lib/tauri";
import { cn } from "@/lib/utils";

/** A preview of a file on disk, or of a link's thumbnail address. Null until (or unless) one exists. */
export function useThumbnail(source: {
  path?: string | null;
  url?: string | null;
}): string | null {
  const [src, setSrc] = useState<string | null>(null);
  const { path, url } = source;
  useEffect(() => {
    let alive = true;
    setSrc(null);
    if (!path && !url) return;
    thumbnailOf({ path: path ?? undefined, url: url ?? undefined })
      .then((s) => alive && setSrc(s))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [path, url]);
  return src;
}

/**
 * The picture on a card: a 16:9 frame for video and links, a square for images
 * and sound. Shows the kind's icon until the preview arrives, or if there's none.
 */
export function Thumb({
  path,
  url,
  kind,
  duration,
  className,
}: {
  path?: string | null;
  url?: string | null;
  kind: "video" | "audio" | "image";
  duration?: number | null;
  className?: string;
}) {
  const src = useThumbnail({ path: kind === "audio" ? null : path, url });
  const Icon = kind === "image" ? ImageIcon : kind === "audio" ? Music : Film;
  const wide = kind === "video";
  return (
    <div
      className={cn(
        "relative grid shrink-0 place-items-center self-start overflow-hidden rounded-[9px] bg-accent text-faint",
        wide ? "aspect-video w-52" : "size-18",
        kind === "image" && src && "checker",
        className,
      )}
    >
      {src ? (
        <img
          src={src}
          alt=""
          className={cn(
            "size-full",
            kind === "image" ? "object-contain" : "object-cover",
          )}
        />
      ) : (
        <Icon
          className={wide ? "size-7" : "size-6"}
          strokeWidth={1.6}
          aria-hidden
        />
      )}
      {duration != null && duration > 0 && (
        <span className="absolute right-1.5 bottom-1.5 rounded-[5px] bg-black/75 px-1.5 py-px text-[11px] text-white tabular-nums">
          {clock(duration)}
        </span>
      )}
    </div>
  );
}
