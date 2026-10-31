import { ArrowLeft } from "lucide-react";
import { useEffect, useState } from "react";
import { AppNavProvider } from "@/lib/app-nav";
import { IntakeProvider } from "@/lib/intake";
import { ACTIVE, useJobs } from "@/lib/jobs";
import { SHOWS_QUEUE, type Screen } from "@/lib/nav";
import { useSettings } from "@/lib/settings";
import type { Mode, SettingsSection } from "@/lib/tauri";
import { DropOverlay } from "@/components/drop-overlay";
import { FirstRun } from "@/components/first-run";
import { JobAnnouncer } from "@/components/job-announcer";
import { QueueRail } from "@/components/queue-rail";
import { TopBar } from "@/components/top-bar";
import { Button } from "@/components/ui/button";
import { Download } from "@/screens/download";
import { DownloadFlow } from "@/screens/simple/download-flow";
import { FixVideoFlow } from "@/screens/simple/fix-video-flow";
import { ImagesFlow } from "@/screens/simple/images-flow";
import { Help } from "@/screens/help";
import { History } from "@/screens/history";
import { ProHome } from "@/screens/pro-home";
import { Settings } from "@/screens/settings";
import { SimpleHome } from "@/screens/simple-home";
import { ToolsScreen } from "@/screens/tools-screen";

export default function App() {
  const { settings, update } = useSettings();
  const { jobs } = useJobs();
  const mode = settings.mode;
  const [screen, setScreen] = useState<Screen>("home");
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection | null>(null);
  // The nonce remounts the Download screen even when the same link is opened twice.
  const [download, setDownload] = useState({ url: "", nonce: 0 });

  // Simple mode keeps its home uncluttered: its own "Recently finished" list covers
  // what's done, so the queue only appears while something runs or has failed.
  const hasJobs = jobs.some(
    (j) =>
      ACTIVE.has(j.status) ||
      ((j.status === "failed" || j.status === "interrupted") &&
        (j.finishedAt ?? 0) > Date.now() - 60 * 60 * 1000),
  );
  // Simple mode's guided screens show their own progress, so the queue only appears on its Home.
  const showQueue =
    mode === "pro" ? SHOWS_QUEUE.has(screen) : screen === "home" && hasJobs;
  const showBack = mode === "simple" && screen !== "home";
  // Pro screens without the docked queue offer it from the top bar, as a drawer.
  const queueInBar = mode === "pro" && !showQueue && screen !== "history";
  const [queueOpen, setQueueOpen] = useState(false);

  const nav = {
    navigate: (to: Screen) => {
      setSettingsSection(null);
      // A link opened with "More options" belongs to that visit only.
      if (to !== "download") setDownload((d) => ({ ...d, url: "" }));
      setScreen(to);
    },
    openSettings: (section: SettingsSection) => {
      setSettingsSection(section);
      setScreen("settings");
    },
    settingsSection,
    downloadUrl: download.url,
    openDownload: (url: string) => {
      setDownload((d) => ({ url, nonce: d.nonce + 1 }));
      setScreen("download");
    },
  };

  return (
    <AppNavProvider value={nav}>
      {/* Anything dropped or pasted, on any screen, lands on Home. */}
      <IntakeProvider onArrive={() => setScreen("home")}>
        {!settings.firstRunDone && <FirstRun />}
        <AppLayout
          mode={mode}
          screen={screen}
          downloadKey={download.nonce}
          blocked={!settings.firstRunDone}
          showQueue={showQueue}
          showBack={showBack}
          queueInBar={queueInBar}
          queueOpen={queueOpen && queueInBar}
          onQueueOpen={setQueueOpen}
          onModeChange={(m) => void update({ mode: m })}
          onNavigate={setScreen}
        />
      </IntakeProvider>
    </AppNavProvider>
  );
}

function AppLayout({
  mode,
  screen,
  downloadKey,
  blocked,
  showQueue,
  showBack,
  queueInBar,
  queueOpen,
  onQueueOpen,
  onModeChange,
  onNavigate: setScreen,
}: {
  mode: Mode;
  screen: Screen;
  downloadKey: number;
  /** First-run setup is up: nothing behind it can be reached. */
  blocked: boolean;
  showQueue: boolean;
  showBack: boolean;
  queueInBar: boolean;
  queueOpen: boolean;
  onQueueOpen: (open: boolean) => void;
  onModeChange: (m: Mode) => void;
  onNavigate: (s: Screen) => void;
}) {
  return (
    <div className="flex h-full flex-col" inert={blocked}>
      <DropOverlay />
      <JobAnnouncer />
      <TopBar
        mode={mode}
        onModeChange={onModeChange}
        screen={screen}
        onNavigate={(s) => {
          onQueueOpen(false);
          setScreen(s);
        }}
        onOpenQueue={queueInBar ? () => onQueueOpen(!queueOpen) : undefined}
      />
      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 [scrollbar-gutter:stable] overflow-y-auto px-6 py-5">
          {showBack && (
            <Button
              variant="ghost"
              size="sm"
              className="mb-3 -ml-2.5"
              onClick={() => setScreen("home")}
            >
              <ArrowLeft />
              Back
            </Button>
          )}
          <ScreenView
            screen={screen}
            mode={mode}
            downloadKey={downloadKey}
            onNavigate={setScreen}
          />
        </main>
        {showQueue && <QueueRail onOpenHistory={() => setScreen("history")} />}
      </div>
      {queueOpen && (
        <QueueDrawer
          onClose={() => onQueueOpen(false)}
          onOpenHistory={() => {
            onQueueOpen(false);
            setScreen("history");
          }}
        />
      )}
    </div>
  );
}

/** The queue over the right edge of the screen, for screens that use the full width. */
function QueueDrawer({
  onClose,
  onOpenHistory,
}: {
  onClose: () => void;
  onOpenHistory: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed inset-x-0 top-13 bottom-0 z-40 flex justify-end">
      <button
        type="button"
        aria-label="Close the queue"
        className="flex-1 cursor-default bg-black/40"
        onClick={onClose}
      />
      <QueueRail
        onOpenHistory={onOpenHistory}
        onClose={onClose}
        className="shadow-2xl"
      />
    </div>
  );
}

function ScreenView({
  screen,
  mode,
  downloadKey,
  onNavigate,
}: {
  screen: Screen;
  mode: Mode;
  downloadKey: number;
  onNavigate: (s: Screen) => void;
}) {
  switch (screen) {
    case "home":
      return mode === "simple" ? (
        <SimpleHome onChoose={onNavigate} />
      ) : (
        <ProHome />
      );
    case "download":
      return mode === "simple" ? (
        <DownloadFlow key={downloadKey} />
      ) : (
        <Download key={downloadKey} />
      );
    case "video":
      return mode === "simple" ? (
        <FixVideoFlow />
      ) : (
        <ToolsScreen key="video" category="video" />
      );
    case "images":
      return mode === "simple" ? (
        <ImagesFlow />
      ) : (
        <ToolsScreen key="image" category="image" />
      );
    case "history":
      return <History />;
    case "settings":
      return <Settings />;
    case "help":
      return <Help />;
  }
}
