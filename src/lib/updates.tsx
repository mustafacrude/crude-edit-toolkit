import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { isTauri, onUpdatesFound, type UpdatesFound } from "@/lib/tauri";

interface UpdatesState {
  /** What the last look for updates found, or null if nothing newer (or not looked yet). */
  found: UpdatesFound | null;
  setFound: (found: UpdatesFound | null) => void;
}

const UpdatesContext = createContext<UpdatesState | null>(null);

/** Holds what the check at launch (or the Check button) found, for the top bar and Settings. */
export function UpdatesProvider({ children }: { children: ReactNode }) {
  const [found, setFound] = useState<UpdatesFound | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    void onUpdatesFound(setFound).then((unlisten) =>
      cancelled ? unlisten() : (stop = unlisten),
    );
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  return (
    <UpdatesContext.Provider value={{ found, setFound }}>
      {children}
    </UpdatesContext.Provider>
  );
}

export function useUpdates() {
  const ctx = useContext(UpdatesContext);
  if (!ctx) throw new Error("useUpdates must be used inside UpdatesProvider");
  return ctx;
}

/** True when there's anything to install. */
export const hasUpdate = (found: UpdatesFound | null) =>
  !!found &&
  (!!found.app || !!found.ytDlp?.available || !!found.untrunc?.available);

/** What's left to install once untrunc has been updated; null when nothing is. */
export const withoutUntrunc = (found: UpdatesFound | null): UpdatesFound | null =>
  found && (found.app || found.ytDlp) ? { ...found, untrunc: null } : null;

/** The same for yt-dlp. */
export const withoutYtDlp = (found: UpdatesFound | null): UpdatesFound | null =>
  found && (found.app || found.untrunc) ? { ...found, ytDlp: null } : null;
