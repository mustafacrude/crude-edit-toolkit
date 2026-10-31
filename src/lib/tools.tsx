import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { toolVersions, type ToolInfo } from "@/lib/tauri";

interface ToolsState {
  tools: ToolInfo[];
  loading: boolean;
  /** Set when the check itself failed, as opposed to one tool being broken. */
  error: string | null;
  refresh: () => void;
}

const ToolsContext = createContext<ToolsState | null>(null);

export function ToolsProvider({ children }: { children: ReactNode }) {
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    toolVersions()
      .then((t) => {
        setTools(t);
        setError(null);
      })
      // Keep the last good list on screen if a re-check fails.
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(refresh, [refresh]);

  return <ToolsContext.Provider value={{ tools, loading, error, refresh }}>{children}</ToolsContext.Provider>;
}

export function useTools() {
  const ctx = useContext(ToolsContext);
  if (!ctx) throw new Error("useTools must be used inside ToolsProvider");
  return ctx;
}
