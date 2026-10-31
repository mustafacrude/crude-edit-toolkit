import { FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { useSettings } from "@/lib/settings";
import {
  BROWSERS,
  downloadArchiveCount,
  forgetDownloadArchive,
  pickCookiesFile,
  pickFolder,
  type SignIn,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

/** Sample values for the naming preview. */
const SAMPLE: Record<string, string> = {
  title: "Big Buck Bunny",
  uploader: "Blender",
  upload_date: "20141110",
  id: "aqz-KE-bpKQ",
  site: "Youtube",
};

const preview = (template: string) => template.replace(/\{(\w+)\}/g, (m, f: string) => SAMPLE[f] ?? m) + ".mp4";
const titleCase = (s: string) => s[0].toUpperCase() + s.slice(1);

/** The download archive: videos playlist downloads skip because they saved them before. */
function PlaylistMemory() {
  const [count, setCount] = useState<number | null>(null);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    downloadArchiveCount()
      .then(setCount)
      .catch((e) => setError(String(e)));
  }, []);

  // Asks inline: the dialog plugin swaps window.confirm for an async version
  // that resolves truthy at once, and the UI has no dialog permission anyway.
  const forget = async () => {
    setAsking(false);
    try {
      await forgetDownloadArchive();
      setCount(0);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs text-muted-foreground">Playlist memory</span>
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-[13px] tabular-nums">
          {count == null ? "…" : count === 0 ? "Nothing remembered yet" : `${count} download${count === 1 ? "" : "s"} remembered`}
        </span>
        {count != null && count > 0 && !asking && (
          <Button variant="outline" size="sm" onClick={() => setAsking(true)}>
            Forget them
          </Button>
        )}
        {asking && (
          <>
            <span className="text-[13px] text-muted-foreground">The next playlist download gets every video again.</span>
            <Button variant="destructive" size="sm" onClick={forget}>
              Forget {count}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setAsking(false)}>
              Keep
            </Button>
          </>
        )}
      </div>
      <p className="max-w-[62ch] text-xs text-muted-foreground">
        Playlist, channel and set downloads skip what they saved before, even if you've since moved or deleted the files.
      </p>
      {error && <p className="text-[13px] text-status-error">{error}</p>}
    </div>
  );
}

export function DownloadSettings() {
  const { settings, update } = useSettings();
  const [naming, setNaming] = useState(settings.namingTemplate);
  const [error, setError] = useState<string | null>(null);

  const save = async (patch: Parameters<typeof update>[0]) => {
    setError(await update(patch));
  };

  const chooseFolder = async () => {
    const dir = await pickFolder();
    if (dir) await save({ downloadDir: dir });
  };

  const chooseCookies = async () => {
    try {
      const file = await pickCookiesFile();
      if (file) await save({ cookiesFile: file, signIn: "file" });
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <>
      <Card id="settings-downloads" className="scroll-mt-4">
        <CardHeader>
          <CardTitle>Downloads</CardTitle>
          <CardDescription>Where downloads go and what they're called.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <span className="text-xs text-muted-foreground">Folder</span>
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate rounded-md border bg-background px-2.5 py-1.5 font-mono text-xs selectable" title={settings.downloadDir}>
                {settings.downloadDir}
              </span>
              <Button variant="outline" size="sm" onClick={chooseFolder}>
                <FolderOpen />
                Change
              </Button>
            </div>
          </div>
          <div className="flex flex-col gap-1.5">
            <label htmlFor="naming-template" className="text-xs text-muted-foreground">
              File names
            </label>
            <Input
              id="naming-template"
              value={naming}
              className="font-mono text-[13px]"
              onChange={(e) => setNaming(e.target.value)}
              onBlur={() => naming !== settings.namingTemplate && void save({ namingTemplate: naming })}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            />
            <p className="text-xs text-muted-foreground">
              Use {"{title}"}, {"{uploader}"}, {"{upload_date}"}, {"{id}"} and {"{site}"}. Example:{" "}
              <span className="font-mono text-foreground/80">{preview(naming)}</span>
            </p>
          </div>
          <PlaylistMemory />
        </CardContent>
      </Card>

      <Card id="settings-signin" className="scroll-mt-4">
        <CardHeader>
          <CardTitle>Sign-in</CardTitle>
          <CardDescription>
            Lets downloads use your account for members-only and age-restricted videos, and SoundCloud Go+ quality. You still turn
            it on per download.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            spacing={0}
            value={settings.signIn}
            onValueChange={(v) => v && void save({ signIn: v as SignIn })}
            aria-label="Sign-in method"
          >
            <ToggleGroupItem value="none">Off</ToggleGroupItem>
            <ToggleGroupItem value="browser">From my browser</ToggleGroupItem>
            <ToggleGroupItem value="file">From a cookies.txt file</ToggleGroupItem>
          </ToggleGroup>

          {settings.signIn === "browser" && (
            <div className="flex flex-col gap-1.5">
              <NativeSelect
                id="sign-in-browser"
                value={settings.browser}
                onChange={(e) => void save({ browser: e.target.value })}
                className="w-48"
              >
                {BROWSERS.map((b) => (
                  <NativeSelectOption key={b} value={b}>
                    {titleCase(b)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <p className="max-w-[62ch] text-xs text-muted-foreground">
                Firefox works best. On Windows, Chrome and Edge lock their sign-ins so other apps usually can't read
                them; if that happens, use a cookies.txt file instead.
              </p>
            </div>
          )}

          {settings.signIn === "file" && (
            <div className="flex flex-col gap-1.5">
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate rounded-md border bg-background px-2.5 py-1.5 font-mono text-xs selectable" title={settings.cookiesFile ?? ""}>
                  {settings.cookiesFile ?? "No file chosen"}
                </span>
                <Button variant="outline" size="sm" onClick={chooseCookies}>
                  Choose file
                </Button>
              </div>
              <p className="max-w-[62ch] text-xs text-muted-foreground">
                Treat this file like a password. The toolkit only remembers where it is: it never copies it into the
                app, never changes it, and hides it from logs and commands.
              </p>
            </div>
          )}
          {error && <p className="text-[13px] text-status-error">{error}</p>}
        </CardContent>
      </Card>
    </>
  );
}
