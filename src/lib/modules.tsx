import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { encoderStatus, isTauri, listModules, retestEncoders, type EncoderView, type ModuleInfo } from "@/lib/tauri";

interface ModulesState {
  modules: ModuleInfo[];
  /** Null until the GPU test has run (a few seconds on first launch). */
  encoder: EncoderView | null;
  retest: () => Promise<void>;
}

const ModulesContext = createContext<ModulesState | null>(null);

/** The tool list and the machine's encoder, loaded once for every screen. */
export function ModulesProvider({ children }: { children: ReactNode }) {
  const [modules, setModules] = useState<ModuleInfo[]>([]);
  const [encoder, setEncoder] = useState<EncoderView | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    listModules().then(setModules).catch(() => setModules([]));
    encoderStatus().then(setEncoder).catch(() => setEncoder(null));
  }, []);

  const retest = useCallback(async () => {
    setEncoder(null);
    setEncoder(await retestEncoders().catch(() => null));
  }, []);

  return <ModulesContext.Provider value={{ modules, encoder, retest }}>{children}</ModulesContext.Provider>;
}

export function useModules() {
  const ctx = useContext(ModulesContext);
  if (!ctx) throw new Error("useModules must be used inside ModulesProvider");
  return ctx;
}
