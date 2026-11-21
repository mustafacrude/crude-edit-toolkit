import { getVersion } from "@tauri-apps/api/app";
import { Check, LoaderCircle, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { ACTIVE, useJobs } from "@/lib/jobs";
import { useSettings } from "@/lib/settings";
import { useTools } from "@/lib/tools";
import { useUpdates, withoutUntrunc, withoutYtDlp } from "@/lib/updates";
import {
  checkForUpdates,
  installAppUpdate,
  isTauri,
  goBackToBundledYtDlp,
  updateUntrunc,
  updateYtDlp,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";

type Busy = null | "checking" | "app" | "yt-dlp" | "untrunc";

/** Updates and notifications. Nothing installs unless a button here is pressed. */
export function UpdateSettings() {
  const { settings, update } = useSettings();
  const { found, setFound } = useUpdates();
  const { tools, refresh } = useTools();
  const { jobs } = useJobs();
  const [version, setVersion] = useState("");
  const [busy, setBusy] = useState<Busy>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checked, setChecked] = useState(false);
  const ytDlp = tools.find((t) => t.id === "yt-dlp");
  // Only once it has been downloaded (Video tools > Repair damaged video).
  const untrunc = tools.find((t) => t.id === "untrunc");
  const working = jobs.some((j) => ACTIVE.has(j.status));

  useEffect(() => {
    if (isTauri())
      void getVersion()
        .then(setVersion)
        .catch(() => {});
  }, []);

  const check = async () => {
    setBusy("checking");
    setError(null);
    setMessage(null);
    try {
      const result = await checkForUpdates();
      setFound(result.app || result.ytDlp || result.untrunc ? result : null);
      setChecked(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const updateTool = async () => {
    setBusy("yt-dlp");
    setError(null);
    setMessage(null);
    try {
      const done = await updateYtDlp();
      setMessage(
        done
          ? `yt-dlp is now ${done}.`
          : "yt-dlp is already the newest version.",
      );
      setFound(withoutYtDlp(found));
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const updateRepairTool = async () => {
    setBusy("untrunc");
    setError(null);
    setMessage(null);
    try {
      const done = await updateUntrunc();
      setMessage(
        done
          ? `untrunc is now build ${done}.`
          : "untrunc is already the newest build.",
      );
      setFound(withoutUntrunc(found));
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const goBack = async () => {
    setError(null);
    setMessage(null);
    try {
      await goBackToBundledYtDlp();
      setMessage("Back to the yt-dlp that came with the app.");
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const installApp = async () => {
    setBusy("app");
    setError(null);
    try {
      await installAppUpdate(); // the app restarts into the new version
    } catch (e) {
      setError(String(e));
      setBusy(null);
    }
  };

  return (
    <Card id="settings-updates" className="scroll-mt-4">
      <CardHeader>
        <CardTitle>Updates and notifications</CardTitle>
        <CardDescription>
          The app, yt-dlp and untrunc (if you use it) can be updated from here. Nothing installs without
          you pressing a button.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex min-w-0 flex-[1_1_260px] flex-col gap-0.5">
            <span className="text-[13px] font-medium">
              Crude's Edit Toolkit {version}
            </span>
            <span className="text-xs text-muted-foreground">
              {found?.app
                ? `Version ${found.app.version} is available.`
                : checked
                  ? "You have the newest version."
                  : "Looks for a new version of the app."}
            </span>
          </div>
          {found?.app ? (
            <Button
              size="sm"
              onClick={installApp}
              disabled={busy !== null || working}
            >
              {busy === "app" && (
                <LoaderCircle className="animate-spin motion-reduce:animate-none" />
              )}
              Install and restart
            </Button>
          ) : (
            <Button
              variant="outline"
              size="sm"
              onClick={check}
              disabled={busy !== null}
            >
              {busy === "checking" ? (
                <LoaderCircle className="animate-spin motion-reduce:animate-none" />
              ) : (
                <RefreshCw />
              )}
              Check for updates
            </Button>
          )}
        </div>
        {found?.app && working && (
          <p className="-mt-2 text-xs text-status-warn">
            Jobs are running. Installing would stop them, so wait for the queue
            to finish.
          </p>
        )}
        {found?.app?.notes && (
          <p className="-mt-2 max-w-[70ch] text-xs whitespace-pre-line text-muted-foreground">
            {found.app.notes}
          </p>
        )}

        <div className="flex flex-wrap items-center gap-3 border-t pt-4">
          <div className="flex min-w-0 flex-[1_1_260px] flex-col gap-0.5">
            <span className="text-[13px] font-medium">
              yt-dlp {ytDlp?.version ?? ""}
            </span>
            <span className="text-xs text-muted-foreground">
              {found?.ytDlp?.available
                ? `${found.ytDlp.latest} is available. Sites change often, and a newer yt-dlp usually fixes failed downloads.`
                : checked
                  ? "yt-dlp is the newest version."
                  : "The program that downloads from YouTube, Twitch and other sites."}
            </span>
          </div>
          <div className="flex gap-2">
            {found?.ytDlp?.downloaded && (
              <Button
                variant="ghost"
                size="sm"
                onClick={goBack}
                disabled={busy !== null}
              >
                Use the built-in one
              </Button>
            )}
            <Button
              variant={found?.ytDlp?.available ? "default" : "outline"}
              size="sm"
              onClick={updateTool}
              disabled={busy !== null || working}
              title={working ? "Wait for the queue to finish" : undefined}
            >
              {busy === "yt-dlp" ? (
                <LoaderCircle className="animate-spin motion-reduce:animate-none" />
              ) : (
                <RefreshCw />
              )}
              {found?.ytDlp?.available
                ? `Update to ${found.ytDlp.latest}`
                : "Update yt-dlp"}
            </Button>
          </div>
        </div>

        {untrunc && !untrunc.missing && (
          <div className="flex flex-wrap items-center gap-3 border-t pt-4">
            <div className="flex min-w-0 flex-[1_1_260px] flex-col gap-0.5">
              <span className="text-[13px] font-medium">
                untrunc {untrunc.version ?? ""}
              </span>
              <span className="text-xs text-muted-foreground">
                {found?.untrunc?.available
                  ? `A newer build (${found.untrunc.latest}${found.untrunc.released ? `, ${found.untrunc.released}` : ""}) is available.`
                  : checked
                    ? "untrunc is the newest build."
                    : "Repairs cut-off recordings. Its author publishes new builds now and then."}
              </span>
            </div>
            <Button
              variant={found?.untrunc?.available ? "default" : "outline"}
              size="sm"
              onClick={updateRepairTool}
              disabled={busy !== null || working}
              title={working ? "Wait for the queue to finish" : undefined}
            >
              {busy === "untrunc" ? (
                <LoaderCircle className="animate-spin motion-reduce:animate-none" />
              ) : (
                <RefreshCw />
              )}
              {found?.untrunc?.available
                ? `Update to ${found.untrunc.latest}`
                : "Update untrunc"}
            </Button>
          </div>
        )}

        {message && (
          <p className="flex items-center gap-1.5 text-[13px] text-status-ok">
            <Check className="size-4" aria-hidden />
            {message}
          </p>
        )}
        {error && (
          <p className="text-[13px] break-words text-status-error">{error}</p>
        )}

        <div className="flex flex-col gap-2 border-t pt-4">
          <Toggle
            id="set-check-updates"
            label="Look for updates when the app opens (once a day)"
            checked={settings.checkForUpdates}
            onChange={(v) => void update({ checkForUpdates: v })}
          />
          <Toggle
            id="set-notify"
            label="Show a notification when the queue finishes while the app is in the background"
            checked={settings.notifyWhenDone}
            onChange={(v) => void update({ notifyWhenDone: v })}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function Toggle({
  id,
  label,
  checked,
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <Checkbox
        id={id}
        checked={checked}
        onCheckedChange={(v) => onChange(v === true)}
      />
      <Label htmlFor={id} className="text-[13px] font-normal">
        {label}
      </Label>
    </div>
  );
}
