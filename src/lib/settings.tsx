import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { getSettings, isTauri, setSettings, type Settings } from "@/lib/tauri";

const DEFAULTS: Settings = {
  mode: "simple",
  downloadConcurrency: 3,
  downloadDir: "",
  namingTemplate: "{uploader}_{upload_date}_{title}",
  signIn: "none",
  browser: "firefox",
  cookiesFile: null,
  encoder: "auto",
  cpuLimit: 100,
  speedLimitMb: 0,
  firstRunDone: true, // a plain browser has no tools to set up
  notifyWhenDone: true,
  checkForUpdates: true,
};

interface SettingsState {
  settings: Settings;
  /** Saves a change. Resolves to an error message if Rust rejected it (and it was undone). */
  update: (patch: Partial<Settings>) => Promise<string | null>;
}

const SettingsContext = createContext<SettingsState | null>(null);

/** Settings live in Rust (settings.json); this mirrors them and saves changes back. */
export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setLocal] = useState<Settings | null>(isTauri() ? null : DEFAULTS);
  const current = useRef(settings);
  current.current = settings;

  useEffect(() => {
    if (!isTauri()) return;
    getSettings()
      .then(setLocal)
      .catch(() => setLocal(DEFAULTS));
  }, []);

  const update = useCallback(async (patch: Partial<Settings>) => {
    const before = current.current ?? DEFAULTS;
    const next = { ...before, ...patch };
    setLocal(next); // optimistic, so toggles feel instant
    if (!isTauri()) return null;
    try {
      setLocal(await setSettings(next));
      return null;
    } catch (e) {
      setLocal(before);
      return String(e);
    }
  }, []);

  // Wait for the saved mode so Pro users don't see Simple flash first.
  if (!settings) return null;
  return <SettingsContext.Provider value={{ settings, update }}>{children}</SettingsContext.Provider>;
}

export function useSettings() {
  const ctx = useContext(SettingsContext);
  if (!ctx) throw new Error("useSettings must be used inside SettingsProvider");
  return ctx;
}
