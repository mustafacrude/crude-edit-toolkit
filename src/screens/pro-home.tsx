import { ArrowUpFromLine, FolderOpen } from "lucide-react";
import { useIntake } from "@/lib/intake";
import { pickFiles, pickFolder } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { IntakeList } from "@/components/intake-list";
import { Button } from "@/components/ui/button";

export function ProHome() {
  const { items, addPaths, clear } = useIntake();
  const compact = items.length > 0;

  const browse = async () => addPaths(await pickFiles().catch(() => []));
  const browseFolder = async () => {
    const dir = await pickFolder().catch(() => null);
    if (dir) await addPaths([dir]);
  };

  return (
    <div className="flex flex-col gap-4.5">
      <section
        aria-label="Add something"
        className={cn(
          "flex flex-wrap items-center gap-4 rounded-[14px] border-[1.5px] border-dashed border-input bg-chrome",
          compact ? "px-5 py-4" : "flex-col justify-center px-6 py-10 text-center",
        )}
      >
        <span
          className={cn(
            "grid shrink-0 place-items-center rounded-[10px] bg-primary-soft text-primary-ink",
            compact ? "size-10" : "size-12",
          )}
        >
          <ArrowUpFromLine className={compact ? "size-5" : "size-6"} aria-hidden />
        </span>
        <div className={cn("flex flex-col gap-0.5", compact && "min-w-0 flex-[1_1_260px]")}>
          <span className="text-[15px] font-semibold">Drop files or paste a link</span>
          <span className="text-[13px] text-muted-foreground">
            Videos, images, folders, YouTube or Twitch links. Ctrl+V works anywhere.
          </span>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={browse}>
            Choose files
          </Button>
          <Button variant="outline" onClick={browseFolder}>
            <FolderOpen />
            Choose a folder
          </Button>
        </div>
        {!compact && (
          <p className="max-w-[56ch] text-xs text-faint">
            Each file is checked and anything Premiere will struggle with is explained, with the right fix first.
          </p>
        )}
      </section>
      {compact && (
        <>
          <div className="flex items-baseline justify-between">
            <h1 className="text-[15px] font-semibold">Just added</h1>
            <button
              type="button"
              onClick={clear}
              className="rounded-md text-xs text-faint outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
            >
              Clear all
            </button>
          </div>
          <IntakeList hideClear />
        </>
      )}
    </div>
  );
}
