import { useState } from "react";
import { useSettings } from "@/lib/settings";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";

const SPEEDS = [0, 2, 5, 10, 25];

/** Keeps the toolkit from using so much of the computer or the connection that other things suffer. */
export function PerformanceSettings() {
  const { settings, update } = useSettings();
  const [error, setError] = useState<string | null>(null);
  const save = async (patch: Parameters<typeof update>[0]) => setError(await update(patch));
  // A limit set some other way still shows up as itself.
  const speeds = SPEEDS.includes(settings.speedLimitMb) ? SPEEDS : [...SPEEDS, settings.speedLimitMb].sort((a, b) => a - b);

  return (
    <Card id="settings-performance" className="scroll-mt-4">
      <CardHeader>
        <CardTitle>Performance</CardTitle>
        <CardDescription>
          Keeps the toolkit from slowing down the rest of your computer. Tools always run at low priority and share a
          memory limit, so editing in Premiere stays smooth while jobs run.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid grid-cols-[170px_1fr] items-center gap-x-4 gap-y-3">
        <label htmlFor="perf-downloads" className="text-xs text-muted-foreground">
          Downloads at once
        </label>
        <div className="flex flex-col gap-1">
          <NativeSelect
            id="perf-downloads"
            size="sm"
            value={settings.downloadConcurrency}
            onChange={(e) => void save({ downloadConcurrency: Number(e.target.value) })}
            className="w-28"
          >
            {[1, 2, 3, 4, 5, 6].map((n) => (
              <NativeSelectOption key={n} value={n}>
                {n}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <p className="text-xs text-muted-foreground">Converting video always runs one at a time.</p>
        </div>

        <label htmlFor="perf-speed" className="text-xs text-muted-foreground">
          Speed limit per download
        </label>
        <div className="flex flex-col gap-1">
          <NativeSelect
            id="perf-speed"
            size="sm"
            value={settings.speedLimitMb}
            onChange={(e) => void save({ speedLimitMb: Number(e.target.value) })}
            className="w-40"
          >
            {speeds.map((n) => (
              <NativeSelectOption key={n} value={n}>
                {n === 0 ? "No limit" : `${n} MB/s`}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <p className="text-xs text-muted-foreground">Each download stays under this, so several at once add up. Applies to new downloads.</p>
        </div>

        <label htmlFor="perf-cpu" className="text-xs text-muted-foreground">
          Most of the CPU to use
        </label>
        <div className="flex flex-col gap-1">
          <NativeSelect
            id="perf-cpu"
            size="sm"
            value={settings.cpuLimit}
            onChange={(e) => void save({ cpuLimit: Number(e.target.value) })}
            className="w-40"
          >
            <NativeSelectOption value={100}>All of it</NativeSelectOption>
            <NativeSelectOption value={75}>75%</NativeSelectOption>
            <NativeSelectOption value={50}>50%</NativeSelectOption>
          </NativeSelect>
          <p className="text-xs text-muted-foreground">
            Lower this to keep fans quiet or to work while converting. Conversions take longer. Windows only; a
            graphics card encoder uses very little CPU either way.
          </p>
        </div>
        {error && <p className="col-span-2 text-[13px] text-status-error">{error}</p>}
      </CardContent>
    </Card>
  );
}
