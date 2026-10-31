import { ArrowDownToLine } from "lucide-react";
import { useIntake } from "@/lib/intake";

/** Shown over the whole window while files are dragged in. */
export function DropOverlay() {
  const { dragging } = useIntake();
  if (!dragging) return null;
  return (
    <div className="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-background/80 p-6">
      <div className="flex h-full w-full flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed border-primary">
        <ArrowDownToLine className="size-8 text-primary-ink" aria-hidden />
        <p className="text-lg font-semibold">Drop to check what's inside</p>
        <p className="text-[13px] text-muted-foreground">Videos, images, audio or a whole folder</p>
      </div>
    </div>
  );
}
