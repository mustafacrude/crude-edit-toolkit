// Renders any tool's settings form from the schema it describes in Rust, so a
// new module gets a working form without screen code.
import { ArrowDown, ArrowUp, X } from "lucide-react";
import { fileName } from "@/lib/format";
import { PREMIERE_SAFE, type EncodeSettings, type Field } from "@/lib/tauri";
import { EncodePanel } from "@/components/encode-panel";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

export type FormValues = Record<string, unknown>;
export type Trim = { start: string; end: string };

/** The starting values for a schema. */
export function defaults(schema: Field[]): FormValues {
  const values: FormValues = {};
  for (const f of schema) {
    if (f.kind === "select" || f.kind === "number" || f.kind === "toggle") values[f.key] = f.default;
    else if (f.kind === "lines") values[f.key] = "";
    else if (f.kind === "encode") values[f.key] = PREMIERE_SAFE;
    else if (f.kind === "trimList") values[f.key] = {};
  }
  return values;
}

const memoryKey = (toolId: string) => `crude.tool.${toolId}`;

/** The starting values, with whatever was used last time laid over them. Per-file trims are never remembered. */
export function remembered(toolId: string, schema: Field[]): FormValues {
  const values = defaults(schema);
  try {
    const saved = JSON.parse(localStorage.getItem(memoryKey(toolId)) ?? "{}") as FormValues;
    for (const f of schema) {
      if (f.kind !== "trimList" && f.key in saved && typeof saved[f.key] === typeof values[f.key]) values[f.key] = saved[f.key];
    }
  } catch {
    // Nothing remembered, or storage is unavailable: the defaults are fine.
  }
  return values;
}

export function remember(toolId: string, schema: Field[], values: FormValues) {
  try {
    const keep: FormValues = {};
    for (const f of schema) if (f.kind !== "trimList") keep[f.key] = values[f.key];
    localStorage.setItem(memoryKey(toolId), JSON.stringify(keep));
  } catch {
    // Not remembering is harmless.
  }
}

/** The options object the module receives: lines become arrays, trims follow the file order. */
export function toOptions(schema: Field[], values: FormValues, files: string[]): Record<string, unknown> {
  const options: Record<string, unknown> = {};
  for (const f of schema) {
    const v = values[f.key];
    if (f.kind === "lines") options[f.key] = String(v ?? "").split("\n").filter((l) => l.trim());
    else if (f.kind === "trimList") {
      const trims = (v ?? {}) as Record<string, Trim>;
      options[f.key] = files.map((path) => trims[path] ?? { start: "", end: "" });
    } else options[f.key] = v;
  }
  return options;
}

interface ToolFormProps {
  schema: Field[];
  values: FormValues;
  onChange: (values: FormValues) => void;
  files: string[];
  onFilesChange: (files: string[]) => void;
}

export function ToolForm({ schema, values, onChange, files, onFilesChange }: ToolFormProps) {
  const set = (key: string, v: unknown) => onChange({ ...values, [key]: v });

  return (
    <div className="flex flex-col gap-5">
      {schema.map((f) => {
        if (f.kind === "encode") {
          return (
            <EncodePanel key={f.key} value={(values[f.key] as EncodeSettings) ?? PREMIERE_SAFE} onChange={(v) => set(f.key, v)} />
          );
        }
        if (f.kind === "trimList") {
          return (
            <TrimList
              key={f.key}
              files={files}
              onFilesChange={onFilesChange}
              trims={(values[f.key] as Record<string, Trim>) ?? {}}
              onTrimsChange={(t) => set(f.key, t)}
            />
          );
        }
        if (f.kind === "number" && f.showWhen && String(values[f.showWhen.key]) !== f.showWhen.value) return null;
        if (f.kind === "toggle") {
          return (
            <div key={f.key} className="grid grid-cols-[130px_1fr] items-start gap-x-5">
              <span />
              <div className="flex flex-col gap-1">
                <div className="flex items-center gap-2">
                  <Checkbox
                    id={`field-${f.key}`}
                    checked={Boolean(values[f.key] ?? f.default)}
                    onCheckedChange={(v) => set(f.key, v === true)}
                  />
                  <Label htmlFor={`field-${f.key}`} className="text-[13px] font-normal">
                    {f.label}
                  </Label>
                </div>
                {f.help && <p className="max-w-[62ch] pl-6 text-xs text-muted-foreground">{f.help}</p>}
              </div>
            </div>
          );
        }
        return (
          <div key={f.key} className="grid grid-cols-[130px_1fr] items-start gap-x-5">
            <Label htmlFor={`field-${f.key}`} className="pt-1.5 text-muted-foreground">
              {f.label}
            </Label>
            <div className="flex flex-col gap-1.5">
              {f.kind === "select" && (
                <ToggleGroup
                  type="single"
                  variant="outline"
                  size="sm"
                  spacing={0}
                  value={String(values[f.key] ?? f.default)}
                  onValueChange={(v) => v && set(f.key, v)}
                  aria-label={f.label}
                  className="flex-wrap"
                >
                  {f.options.map((o) => (
                    <ToggleGroupItem key={o.value} value={o.value}>
                      {o.label}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              )}
              {f.kind === "number" && f.slider && (
                <div className="flex max-w-sm items-center gap-3">
                  <input
                    id={`field-${f.key}`}
                    type="range"
                    min={f.min}
                    max={f.max}
                    step={f.step}
                    value={Number(values[f.key] ?? f.default)}
                    onChange={(e) => set(f.key, Number(e.target.value))}
                    className="h-1.5 flex-1 cursor-pointer accent-primary"
                  />
                  <span className="w-8 text-right font-mono text-[13px] tabular-nums">
                    {Number(values[f.key] ?? f.default)}
                  </span>
                </div>
              )}
              {f.kind === "number" && !f.slider && (
                <div className="flex items-center gap-2">
                  <Input
                    id={`field-${f.key}`}
                    type="number"
                    min={f.min}
                    max={f.max}
                    step={f.step}
                    value={Number(values[f.key] ?? f.default)}
                    onChange={(e) => set(f.key, Number(e.target.value))}
                    className="h-8 w-28 font-mono text-[13px]"
                  />
                  <span className="text-[13px] text-muted-foreground">{f.unit}</span>
                </div>
              )}
              {f.kind === "lines" && (
                <Textarea
                  id={`field-${f.key}`}
                  value={String(values[f.key] ?? "")}
                  onChange={(e) => set(f.key, e.target.value)}
                  placeholder={f.placeholder}
                  rows={3}
                  className="max-w-md font-mono text-[13px]"
                />
              )}
              {f.help && <p className="max-w-[62ch] text-xs text-muted-foreground">{f.help}</p>}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** Trim & merge: the files in order, each with an optional start and end. */
function TrimList({
  files,
  onFilesChange,
  trims,
  onTrimsChange,
}: {
  files: string[];
  onFilesChange: (f: string[]) => void;
  trims: Record<string, Trim>;
  onTrimsChange: (t: Record<string, Trim>) => void;
}) {
  const move = (i: number, by: number) => {
    const next = [...files];
    const [f] = next.splice(i, 1);
    next.splice(i + by, 0, f);
    onFilesChange(next);
  };
  const setTrim = (path: string, patch: Partial<Trim>) =>
    onTrimsChange({ ...trims, [path]: { ...(trims[path] ?? { start: "", end: "" }), ...patch } });

  if (files.length === 0) return null;
  return (
    <div className="flex flex-col gap-2">
      <span className="text-sm text-muted-foreground">Order and trims</span>
      <ol className="flex flex-col divide-y rounded-lg border">
        {files.map((path, i) => (
          <li key={path} className="flex flex-wrap items-center gap-2 px-3 py-2">
            <span className="w-5 font-mono text-xs text-muted-foreground tabular-nums">{i + 1}</span>
            <span className="min-w-0 flex-1 truncate font-mono text-[13px]" title={path}>
              {fileName(path)}
            </span>
            <Input
              aria-label={`Start of ${fileName(path)}`}
              placeholder="start"
              value={trims[path]?.start ?? ""}
              onChange={(e) => setTrim(path, { start: e.target.value })}
              className="h-7 w-24 font-mono text-[13px]"
            />
            <Input
              aria-label={`End of ${fileName(path)}`}
              placeholder="end"
              value={trims[path]?.end ?? ""}
              onChange={(e) => setTrim(path, { end: e.target.value })}
              className="h-7 w-24 font-mono text-[13px]"
            />
            <Button variant="ghost" size="icon-xs" aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>
              <ArrowUp />
            </Button>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Move down"
              disabled={i === files.length - 1}
              onClick={() => move(i, 1)}
            >
              <ArrowDown />
            </Button>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={`Remove ${fileName(path)}`}
              onClick={() => onFilesChange(files.filter((f) => f !== path))}
            >
              <X />
            </Button>
          </li>
        ))}
      </ol>
      <p className="text-xs text-muted-foreground">
        Leave start or end empty to keep the whole clip. Times like 1:02:10. Matching files join losslessly (cuts land on
        keyframes); mixed ones are re-encoded to match the first.
      </p>
    </div>
  );
}
