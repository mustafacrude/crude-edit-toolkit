import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { useAppNav } from "@/lib/app-nav";
import { PAGE_FRAME } from "@/lib/page";
import { useSettings } from "@/lib/settings";
import { licensesText } from "@/lib/tauri";
import { useTools } from "@/lib/tools";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { DownloadSettings } from "@/screens/settings-downloads";
import { EncodingSettings } from "@/screens/settings-encoding";
import { PerformanceSettings } from "@/screens/settings-performance";
import { UpdateSettings } from "@/screens/settings-updates";

/** The licence texts that ship with the app, loaded when first opened (they run to a few MB). */
function Licenses() {
  const [text, setText] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = () => {
    setOpen(!open);
    if (text === null && !error)
      licensesText().then(setText, (e) => setError(String(e)));
  };

  return (
    <div className="flex flex-col gap-2">
      <Button variant="outline" size="sm" className="self-start" aria-expanded={open} onClick={toggle}>
        {open ? "Hide open-source licences" : "Open-source licences"}
      </Button>
      {open && (
        <>
          <p className="max-w-[54ch] text-xs text-muted-foreground">
            The programs and libraries inside the app, and the licence each is used under.
            FFmpeg's source code is on the project's GitHub releases page.
          </p>
          <pre
            tabIndex={0}
            aria-label="Open-source licences"
            className="max-h-80 overflow-auto whitespace-pre-wrap rounded-lg border bg-card p-3 font-mono text-[11px] leading-relaxed text-muted-foreground"
          >
            {error ?? text ?? "Loading…"}
          </pre>
        </>
      )}
    </div>
  );
}

export function Settings() {
  const { tools, loading, error, refresh } = useTools();
  const { settingsSection } = useAppNav();
  const { update } = useSettings();

  // A fix button can open Settings at the card that needs attention.
  useEffect(() => {
    if (settingsSection)
      document
        .getElementById(`settings-${settingsSection}`)
        ?.scrollIntoView({ block: "start" });
  }, [settingsSection]);

  return (
    <div className={cn(PAGE_FRAME, "gap-5")}>
      <h1 className="text-xl font-semibold tracking-tight">Settings</h1>

      <DownloadSettings />
      <EncodingSettings />
      <PerformanceSettings />
      <UpdateSettings />

      <Card id="settings-tools" className="scroll-mt-4">
        <CardHeader>
          <CardTitle>Tools</CardTitle>
          <CardDescription>
            The programs the toolkit runs for you. They come with the app.
          </CardDescription>
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={refresh}
              disabled={loading}
            >
              <RefreshCw
                className={cn(
                  loading && "animate-spin motion-reduce:animate-none",
                )}
              />
              Check again
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          {error && (
            <p className="mb-3 rounded-md border border-status-error/40 px-3 py-2 text-[13px] text-status-error">
              Couldn't check the tools: {error}
            </p>
          )}
          {tools.length === 0 && loading && (
            <p className="text-[13px] text-muted-foreground">Checking tools…</p>
          )}
          <ul className="divide-y">
            {tools.map((tool) => (
              <li
                key={tool.id}
                className="grid grid-cols-[1fr_auto_auto] items-center gap-4 py-2.5"
              >
                <div className="flex min-w-0 flex-col">
                  <span className="text-[13px] font-medium">{tool.name}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {tool.error ?? tool.purpose}
                  </span>
                </div>
                <span
                  className="font-mono text-xs text-muted-foreground tabular-nums selectable"
                  title={tool.path}
                >
                  {tool.version ?? "—"}
                </span>
                {tool.error ? (
                  <Badge
                    variant="outline"
                    className="border-status-error/40 text-status-error"
                  >
                    {tool.missing ? "Missing" : "Problem"}
                  </Badge>
                ) : (
                  <Badge
                    variant="outline"
                    className="border-status-ok/40 text-status-ok"
                  >
                    Ready
                  </Badge>
                )}
              </li>
            ))}
          </ul>
          <p className="mt-3 text-xs text-muted-foreground">
            yt-dlp can be updated between app releases (see Updates, above). The
            other tools come with app updates.
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>About</CardTitle>
          <CardDescription>
            Crude's Edit Toolkit 0.1.0. Runs entirely on this computer, with no
            account and no tracking.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <p className="text-xs text-muted-foreground">
            Only use it on content you own or have permission to use.
          </p>
          <Button
            variant="outline"
            size="sm"
            className="self-start"
            onClick={() => void update({ firstRunDone: false })}
          >
            Show the setup again
          </Button>
          <Licenses />
        </CardContent>
      </Card>
    </div>
  );
}
