import { Cpu, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useModules } from "@/lib/modules";
import { useSettings } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";

const FAMILIES: Record<string, string> = {
  nvenc: "NVIDIA (NVENC)",
  amf: "AMD (AMF)",
  qsv: "Intel (Quick Sync)",
  videotoolbox: "Apple (VideoToolbox)",
};

export function EncodingSettings() {
  const { settings, update } = useSettings();
  const { encoder, retest } = useModules();
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const software = encoder !== null && encoder.hardware.length === 0;

  const runTest = async () => {
    setTesting(true);
    await retest();
    setTesting(false);
  };

  return (
    <Card id="settings-encoding" className="scroll-mt-4">
      <CardHeader>
        <CardTitle>Encoding</CardTitle>
        <CardDescription>Which part of the computer converts video. A graphics card is many times faster than the CPU.</CardDescription>
        <CardAction>
          <Button variant="outline" size="sm" onClick={runTest} disabled={testing || encoder === null}>
            <RefreshCw className={cn(testing && "animate-spin motion-reduce:animate-none")} />
            Test again
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center gap-2 text-[13px]">
          <Cpu className={cn("size-4", software ? "text-status-warn" : "text-status-ok")} aria-hidden />
          {encoder === null
            ? "Testing your graphics card…"
            : software
              ? "No graphics card encoder works here, so video converts on the CPU. It works, just slower."
              : `Using ${encoder.label}.`}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor="encoder-choice" className="text-xs text-muted-foreground">
            Use
          </label>
          <NativeSelect
            id="encoder-choice"
            size="sm"
            value={settings.encoder}
            onChange={async (e) => setError(await update({ encoder: e.target.value }))}
          >
            <NativeSelectOption value="auto">Automatic (fastest that works)</NativeSelectOption>
            {(encoder?.hardware ?? []).map((f) => (
              <NativeSelectOption key={f} value={f}>
                {FAMILIES[f] ?? f}
              </NativeSelectOption>
            ))}
            <NativeSelectOption value="software">CPU only (slowest, most compatible)</NativeSelectOption>
          </NativeSelect>
        </div>
        <p className="text-xs text-muted-foreground">
          Compress to size always uses the CPU: two careful passes hit the size limit far more reliably.
        </p>
        {error && <p className="text-[13px] text-status-error">{error}</p>}
      </CardContent>
    </Card>
  );
}
