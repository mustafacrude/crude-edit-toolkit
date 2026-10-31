import { ChevronRight } from "lucide-react";
import { useState } from "react";
import { useModules } from "@/lib/modules";
import { PREMIERE_SAFE, type EncodeSettings } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

const RATES = ["60", "59.94", "30", "29.97", "24"];
const QUALITIES = [
  { cq: 16, label: "Visually lossless" },
  { cq: 19, label: "High" },
  { cq: 23, label: "Balanced" },
  { cq: 27, label: "Small" },
];

/** The shared encode settings (BRIEF §8). Quality mode is the default; bitrate is there for predictable sizes. */
export function EncodePanel({ value, onChange }: { value: EncodeSettings; onChange: (v: EncodeSettings) => void }) {
  const { encoder } = useModules();
  const set = (patch: Partial<EncodeSettings>) => onChange({ ...value, ...patch });
  // Opens by itself when something in there isn't the Premiere-safe default, so it's never hidden.
  const [open, setOpen] = useState(
    value.rate.mode !== "quality" ||
      value.keyframeSecs !== PREMIERE_SAFE.keyframeSecs ||
      value.audioKbps !== PREMIERE_SAFE.audioKbps,
  );
  const fpsChoice = value.fps === null ? "source" : RATES.includes(value.fps) ? value.fps : "custom";

  return (
    <div className="grid grid-cols-[130px_1fr] items-start gap-x-5 gap-y-4">
      <Label className="pt-1.5 text-muted-foreground">Codec</Label>
      <div className="flex flex-col gap-1.5">
        <Seg
          value={value.codec}
          onChange={(codec) => set({ codec: codec as EncodeSettings["codec"] })}
          options={[
            ["h264", "H.264"],
            ["hevc", "HEVC"],
          ]}
          label="Codec"
        />
        <Help>
          {value.codec === "h264"
            ? "Plays and scrubs smoothly everywhere."
            : "About 40% smaller, heavier to edit. Stays 10-bit when the source is."}
        </Help>
      </div>

      <Label className="pt-1.5 text-muted-foreground">Frame rate</Label>
      <div className="flex flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <Seg
            value={fpsChoice}
            onChange={(c) => set({ fps: c === "source" ? null : c === "custom" ? (value.fps ?? "50") : c })}
            options={[["source", "Match source"], ...RATES.map((r) => [r, r] as [string, string]), ["custom", "Custom"]]}
            label="Frame rate"
          />
          {fpsChoice === "custom" && (
            <Input
              aria-label="Custom frame rate"
              className="h-7 w-20 font-mono text-[13px]"
              value={value.fps ?? ""}
              onChange={(e) => set({ fps: e.target.value })}
            />
          )}
        </div>
        <Help>Always constant, so audio stays in sync. Match your Premiere sequence.</Help>
      </div>

      <Label className="pt-1.5 text-muted-foreground">Quality</Label>
      <div className="flex flex-col gap-1.5">
        {value.rate.mode === "quality" ? (
          <>
            <Seg
              value={String(value.rate.cq)}
              onChange={(cq) => set({ rate: { mode: "quality", cq: Number(cq) } })}
              options={QUALITIES.map((q) => [String(q.cq), q.label] as [string, string])}
              label="Quality level"
            />
            <Help>The encoder spends bits where the footage needs them, so the size depends on what's on screen.</Help>
          </>
        ) : (
          <Help>
            A fixed bitrate: about {Math.round(((value.rate.targetMbps + value.audioKbps / 1000) * 60) / 8)} MB per
            minute of video. Change it under Advanced.
          </Help>
        )}
      </div>

      <div className="col-span-2">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex items-center gap-1 rounded-md py-1 pr-2 text-[13px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          <ChevronRight className={cn("size-3.5 transition-transform motion-reduce:transition-none", open && "rotate-90")} aria-hidden />
          Advanced
          <span className="text-xs text-faint"> · bitrate, scrubbing, audio, encoder</span>
        </button>
      </div>

      {open && (
        <>
          <Label className="pt-1.5 text-muted-foreground">Rate control</Label>
          <div className="flex flex-col gap-2">
            <Seg
              value={value.rate.mode}
              onChange={(mode) =>
                set({ rate: mode === "quality" ? { mode: "quality", cq: 19 } : { mode: "bitrate", targetMbps: 20, maxMbps: 30 } })
              }
              options={[
                ["quality", "By quality"],
                ["bitrate", "By bitrate"],
              ]}
              label="Rate control"
            />
            {value.rate.mode === "bitrate" && (
              <>
                <div className="flex flex-wrap items-center gap-3">
                  <MbpsInput
                    label="Target"
                    value={value.rate.targetMbps}
                    onChange={(targetMbps) => value.rate.mode === "bitrate" && set({ rate: { ...value.rate, targetMbps } })}
                  />
                  <MbpsInput
                    label="Max"
                    value={value.rate.maxMbps}
                    onChange={(maxMbps) => value.rate.mode === "bitrate" && set({ rate: { ...value.rate, maxMbps } })}
                  />
                </div>
                <Help>Use this when a platform asks for a set size. Otherwise By quality is better.</Help>
              </>
            )}
          </div>

          <span className="text-sm text-muted-foreground">Scrubbing</span>
          <div className="flex flex-wrap items-center gap-2">
            <Checkbox
              id="enc-keyframes"
              checked={value.keyframeSecs !== null}
              onCheckedChange={(v) => set({ keyframeSecs: v === true ? 1 : null })}
            />
            <Label htmlFor="enc-keyframes" className="text-[13px] font-normal">
              Smooth scrubbing (a keyframe every second)
            </Label>
            <Help>Easier to scrub in Premiere, slightly bigger file.</Help>
          </div>

          <Label htmlFor="enc-audio" className="pt-1.5 text-muted-foreground">
            Audio
          </Label>
          <div className="flex flex-wrap items-center gap-2">
            <NativeSelect
              id="enc-audio"
              size="sm"
              value={String(value.audioKbps)}
              onChange={(e) => set({ audioKbps: Number(e.target.value) })}
            >
              <NativeSelectOption value="192">AAC 192 kbps</NativeSelectOption>
              <NativeSelectOption value="256">AAC 256 kbps</NativeSelectOption>
              <NativeSelectOption value="320">AAC 320 kbps</NativeSelectOption>
            </NativeSelect>
            <Help>48 kHz, every track kept.</Help>
          </div>

          <span className="text-sm text-muted-foreground">Encoder</span>
          <span className="text-[13px]">
            {encoder ? encoder.label : "Testing your graphics card…"}
            <span className="text-xs text-muted-foreground"> · change in Settings</span>
          </span>
        </>
      )}
    </div>
  );
}

function Seg({
  value,
  onChange,
  options,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  options: [string, string][];
  label: string;
}) {
  return (
    <ToggleGroup
      type="single"
      variant="outline"
      size="sm"
      spacing={0}
      value={value}
      onValueChange={(v) => v && onChange(v)}
      aria-label={label}
      className="flex-wrap"
    >
      {options.map(([v, l]) => (
        <ToggleGroupItem key={v} value={v}>
          {l}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}

function MbpsInput({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  return (
    <label className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
      {label}
      <Input
        type="number"
        min={0.1}
        max={500}
        step={1}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="h-7 w-20 font-mono text-[13px]"
      />
      Mb/s
    </label>
  );
}

function Help({ children }: { children: React.ReactNode }) {
  return <p className="text-xs text-pretty text-muted-foreground">{children}</p>;
}
