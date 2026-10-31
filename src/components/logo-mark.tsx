import { Crop } from "lucide-react";
import { cn } from "@/lib/utils";

/** The app's mark: crop corners on the accent, as in the app icon. */
export function LogoMark({ className }: { className?: string }) {
  return (
    <span
      className={cn("grid size-6.5 shrink-0 place-items-center rounded-[7px] bg-brand text-white", className)}
      aria-hidden
    >
      <Crop className="size-[60%]" strokeWidth={2.4} />
    </span>
  );
}
