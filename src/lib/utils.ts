import type { KeyboardEvent } from "react";

export { cn } from "cn";

/** Arrow keys move along a row of radio cards, as they do in a native radio group. */
export function radioArrows(e: KeyboardEvent<HTMLElement>) {
  const steps: Record<string, number> = {
    ArrowRight: 1,
    ArrowDown: 1,
    ArrowLeft: -1,
    ArrowUp: -1,
  };
  const step = steps[e.key];
  if (!step) return;
  const radios = [
    ...e.currentTarget.querySelectorAll<HTMLButtonElement>(
      '[role="radio"]:not(:disabled)',
    ),
  ];
  const at = radios.indexOf(e.target as HTMLButtonElement);
  const next = radios[(at + step + radios.length) % radios.length];
  e.preventDefault();
  next?.focus();
  next?.click();
}
