import { ArrowUpCircle, BookOpen, ListVideo, Settings } from "lucide-react";
import { cn } from "@/lib/utils";
import { ACTIVE, useJobs } from "@/lib/jobs";
import { NAV, type Screen } from "@/lib/nav";
import type { Mode } from "@/lib/tauri";
import { useAppNav } from "@/lib/app-nav";
import { useTools } from "@/lib/tools";
import { hasUpdate, useUpdates } from "@/lib/updates";
import { LogoMark } from "@/components/logo-mark";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface TopBarProps {
  mode: Mode;
  onModeChange: (mode: Mode) => void;
  screen: Screen;
  onNavigate: (screen: Screen) => void;
  /** Set when the queue isn't docked on this screen: the bar offers it instead. */
  onOpenQueue?: () => void;
}

/** Recently failed jobs still need a look; older ones live in History. */
const RECENT_MS = 60 * 60 * 1000;

// Name on the left, the screens in the middle, controls on the right. The window can
// shrink to 960px: first the idle wording goes (a dot and an icon say the same),
// then the screen names (the icons stay, with tooltips).
export function TopBar({
  mode,
  onModeChange,
  screen,
  onNavigate,
  onOpenQueue,
}: TopBarProps) {
  return (
    <header className="flex h-13 shrink-0 items-center gap-4 border-b bg-chrome px-4">
      <button
        type="button"
        onClick={() => onNavigate("home")}
        className="flex shrink-0 items-center gap-2.5 rounded-md text-[14px] font-semibold tracking-[-0.01em] whitespace-nowrap outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <LogoMark />
        Crude's Toolkit
      </button>

      <div className="flex min-w-0 flex-1 justify-center">
        <nav
          className={cn(
            "flex gap-0.5 rounded-[10px] border bg-background p-[3px]",
            mode !== "pro" && "invisible",
          )}
          aria-label="Main"
        >
          {mode === "pro" &&
            NAV.map((item) => (
              <button
                key={item.screen}
                type="button"
                title={item.label}
                onClick={() => onNavigate(item.screen)}
                aria-current={screen === item.screen ? "page" : undefined}
                className={cn(
                  "flex items-center gap-1.5 rounded-[7px] px-3 py-1.5 text-[13px] whitespace-nowrap text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50",
                  "aria-[current=page]:bg-primary-soft aria-[current=page]:font-medium aria-[current=page]:text-primary-ink",
                )}
              >
                <item.icon className="size-[15px]" aria-hidden />
                <span className="max-[1059px]:sr-only">{item.label}</span>
              </button>
            ))}
        </nav>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <UpdateButton />
        {onOpenQueue && <QueueButton onOpen={onOpenQueue} />}
        <ToolStatus onOpen={() => onNavigate("settings")} />
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          spacing={0}
          value={mode}
          onValueChange={(v) => v && onModeChange(v as Mode)}
          aria-label="Interface mode"
        >
          <ToggleGroupItem value="simple">Simple</ToggleGroupItem>
          <ToggleGroupItem value="pro">Pro</ToggleGroupItem>
        </ToggleGroup>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              aria-label="Handbook"
              aria-pressed={screen === "help"}
              onClick={() => onNavigate("help")}
              className="text-muted-foreground aria-pressed:bg-accent aria-pressed:text-foreground"
            >
              <BookOpen />
              <span className="max-[1279px]:sr-only">Handbook</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent>Handbook</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Settings"
              aria-pressed={screen === "settings"}
              onClick={() => onNavigate("settings")}
              className="text-muted-foreground aria-pressed:bg-accent aria-pressed:text-foreground"
            >
              <Settings />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Settings</TooltipContent>
        </Tooltip>
      </div>
    </header>
  );
}

/** Shown only when a check found something newer. Opens the Updates card. */
function UpdateButton() {
  const { found } = useUpdates();
  const { openSettings } = useAppNav();
  if (!hasUpdate(found)) return null;
  const what = found?.app ? "Update available" : "yt-dlp update";
  return (
    <Button
      variant="outline"
      size="sm"
      onClick={() => openSettings("updates")}
      title={what}
      className="gap-1.5 border-primary/50 bg-transparent text-xs font-normal text-primary-ink"
    >
      <ArrowUpCircle className="size-3.5" />
      <span className="max-[1279px]:sr-only">{what}</span>
    </Button>
  );
}

/** The queue, folded into the bar on screens that need the width. */
function QueueButton({ onOpen }: { onOpen: () => void }) {
  const { jobs } = useJobs();
  const running = jobs.filter((j) => j.status === "running").length;
  const waiting = jobs.filter((j) => ACTIVE.has(j.status)).length - running;
  const failed = jobs.filter(
    (j) =>
      (j.status === "failed" || j.status === "interrupted") &&
      (j.finishedAt ?? 0) > Date.now() - RECENT_MS,
  ).length;
  const label =
    running > 0
      ? `${running} running`
      : waiting > 0
        ? `${waiting} waiting`
        : failed > 0
          ? `${failed} ${failed === 1 ? "needs" : "need"} attention`
          : "idle";
  return (
    <Button
      variant="outline"
      size="sm"
      onClick={onOpen}
      title={`Queue: ${label}`}
      className="gap-2 bg-transparent text-xs font-normal"
    >
      {running > 0 ? (
        <span
          className="size-3.5 animate-spin rounded-full border-2 border-input border-t-progress motion-reduce:animate-none"
          aria-hidden
        />
      ) : (
        <ListVideo
          className={cn(
            "size-3.5",
            failed > 0 && waiting === 0
              ? "text-status-error"
              : "text-muted-foreground",
          )}
        />
      )}
      <span className={cn(label === "idle" && "max-[1279px]:sr-only")}>
        Queue · {label}
      </span>
    </Button>
  );
}

function ToolStatus({ onOpen }: { onOpen: () => void }) {
  const { tools, loading, error } = useTools();
  const broken = tools.filter((t) => t.error).length;

  let tone = "bg-status-ok";
  let label = "Ready";
  let quiet = true;
  if (loading && tools.length === 0) {
    tone = "bg-muted-foreground";
    label = "Checking tools";
    quiet = false;
  } else if (error && tools.length === 0) {
    tone = "bg-status-error";
    label = "Tools unavailable";
    quiet = false;
  } else if (broken > 0) {
    tone = "bg-status-error";
    quiet = false;
    label =
      broken === 1
        ? "1 tool needs attention"
        : `${broken} tools need attention`;
  }

  return (
    <button
      type="button"
      onClick={onOpen}
      title={label}
      className="flex items-center gap-1.5 rounded-md px-1 text-xs text-faint outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <span className={cn("size-[7px] rounded-full", tone)} aria-hidden />
      <span className={cn(quiet && "max-[1279px]:sr-only")}>{label}</span>
    </button>
  );
}
