import { Check, Cpu, FolderOpen, LoaderCircle, RefreshCw, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { useModules } from "@/lib/modules";
import { useSettings } from "@/lib/settings";
import { useTools } from "@/lib/tools";
import { LogoMark } from "@/components/logo-mark";
import { pickFolder } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

const STEPS = ["Where downloads go", "Your graphics card", "Check the tools"] as const;

/** Three short steps the first time the app opens (and from Settings → About). */
export function FirstRun() {
  const { settings, update } = useSettings();
  const { encoder, retest } = useModules();
  const { tools, loading, refresh } = useTools();
  const [step, setStep] = useState(0);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const finish = () => void update({ firstRunDone: true });
  const broken = tools.filter((t) => t.error);

  const chooseFolder = async () => {
    const dir = await pickFolder().catch(() => null);
    if (dir) setError(await update({ downloadDir: dir }));
  };
  const runTest = async () => {
    setTesting(true);
    await retest();
    setTesting(false);
  };

  return (
    <div role="dialog" aria-modal="true" aria-label="Set up Crude's Toolkit" className="fixed inset-0 z-40 overflow-y-auto bg-background p-6">
      <div className="mx-auto flex min-h-full w-full max-w-xl flex-col justify-center gap-6">
        <div className="flex flex-col gap-1.5">
          <span className="flex items-center gap-2 text-[15px] font-semibold">
            <LogoMark />
            Crude's Toolkit
          </span>
          <h1 className="text-2xl font-semibold tracking-tight">Let's get you set up</h1>
          <p className="text-muted-foreground">Three quick things. You can change any of them later in Settings.</p>
        </div>

        <ol className="flex gap-2" aria-label="Setup steps">
          {STEPS.map((label, i) => (
            <li
              key={label}
              aria-current={i === step ? "step" : undefined}
              className={cn(
                "flex-1 rounded-lg border px-3 py-2 text-xs",
                i === step ? "border-primary/60 bg-primary/[0.05] font-medium text-foreground" : "text-muted-foreground",
              )}
            >
              <span className="tabular-nums">{i + 1}.</span> {label}
            </li>
          ))}
        </ol>

        <div className="flex flex-col gap-4 rounded-2xl border bg-card p-5">
          {step === 0 && (
            <>
              <h2 className="text-base font-semibold">Where should downloads go?</h2>
              <p className="text-[13px] text-muted-foreground">
                Every download lands in this one folder. Anything you convert or clean up is saved in a "Toolkit" folder next to the original
                file instead.
              </p>
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate rounded-md border bg-background px-2.5 py-2 font-mono text-xs selectable" title={settings.downloadDir}>
                  {settings.downloadDir}
                </span>
                <Button variant="outline" onClick={chooseFolder}>
                  <FolderOpen />
                  Change
                </Button>
              </div>
              {error && <p className="text-[13px] text-status-error">{error}</p>}
            </>
          )}

          {step === 1 && (
            <>
              <h2 className="text-base font-semibold">Your graphics card</h2>
              <p className="text-[13px] text-muted-foreground">
                Converting video is many times faster on a graphics card. The toolkit tries each one with a real test and uses the best that works.
              </p>
              <div className="flex items-start gap-3 rounded-lg border bg-background p-3 text-[13px]">
                {encoder === null || testing ? (
                  <>
                    <LoaderCircle className="mt-0.5 size-4 animate-spin text-muted-foreground motion-reduce:animate-none" aria-hidden />
                    <span>Testing your graphics card. This takes a few seconds…</span>
                  </>
                ) : encoder.hardware.length > 0 ? (
                  <>
                    <Cpu className="mt-0.5 size-4 text-status-ok" aria-hidden />
                    <span>
                      <span className="font-medium">Using {encoder.label}.</span> Conversions will be fast.
                    </span>
                  </>
                ) : (
                  <>
                    <TriangleAlert className="mt-0.5 size-4 text-status-warn" aria-hidden />
                    <span>
                      <span className="font-medium">No graphics card encoder works here,</span> so video will convert on the CPU. Everything still
                      works, just slower.
                    </span>
                  </>
                )}
              </div>
              <Button variant="outline" size="sm" className="self-start" onClick={runTest} disabled={testing || encoder === null}>
                <RefreshCw className={cn(testing && "animate-spin motion-reduce:animate-none")} />
                Test again
              </Button>
            </>
          )}

          {step === 2 && (
            <>
              <h2 className="text-base font-semibold">Checking the tools</h2>
              <p className="text-[13px] text-muted-foreground">These programs come with the app and do the actual work.</p>
              <ul className="divide-y rounded-lg border bg-background">
                {tools.length === 0 && loading && <li className="px-3 py-2.5 text-[13px] text-muted-foreground">Checking…</li>}
                {tools.map((tool) => (
                  <li key={tool.id} className="flex items-center justify-between gap-3 px-3 py-2">
                    <span className="flex min-w-0 flex-col">
                      <span className="text-[13px] font-medium">{tool.name}</span>
                      <span className="truncate text-xs text-muted-foreground">{tool.error ?? tool.purpose}</span>
                    </span>
                    {tool.error ? (
                      <TriangleAlert className="size-4 shrink-0 text-status-error" aria-label="Problem" />
                    ) : (
                      <Check className="size-4 shrink-0 text-status-ok" aria-label="Ready" />
                    )}
                  </li>
                ))}
              </ul>
              {!loading && tools.length > 0 && (
                <p className={cn("text-[13px]", broken.length > 0 ? "text-status-warn" : "text-status-ok")}>
                  {broken.length === 0
                    ? "Everything's ready."
                    : `${broken.length === 1 ? "One tool needs" : `${broken.length} tools need`} attention. You can still continue; whatever depends on ${broken.length === 1 ? "it" : "them"} won't work until the app is reinstalled.`}
                </p>
              )}
              <Button variant="outline" size="sm" className="self-start" onClick={refresh} disabled={loading}>
                <RefreshCw className={cn(loading && "animate-spin motion-reduce:animate-none")} />
                Check again
              </Button>
            </>
          )}
        </div>

        <div className="flex items-center justify-between">
          <Button variant="ghost" onClick={finish}>
            Skip for now
          </Button>
          <div className="flex gap-2">
            {step > 0 && (
              <Button variant="outline" size="lg" onClick={() => setStep(step - 1)}>
                Back
              </Button>
            )}
            {step < STEPS.length - 1 ? (
              <Button size="lg" onClick={() => setStep(step + 1)}>
                Next
              </Button>
            ) : (
              <Button size="lg" onClick={finish}>
                Finish
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
