import { createContext, useContext, type ReactNode } from "react";
import type { Screen } from "@/lib/nav";
import type { SettingsSection } from "@/lib/tauri";

interface AppNav {
  navigate: (screen: Screen) => void;
  /** Opens the Download screen with a link filled in. */
  openDownload: (url: string) => void;
  /** The link the Download screen should start with. */
  downloadUrl: string;
  /** Opens Settings scrolled to one of its cards. */
  openSettings: (section: SettingsSection) => void;
  /** The card Settings should scroll to when it opens, once. */
  settingsSection: SettingsSection | null;
}

const AppNavContext = createContext<AppNav | null>(null);

export function AppNavProvider({ value, children }: { value: AppNav; children: ReactNode }) {
  return <AppNavContext.Provider value={value}>{children}</AppNavContext.Provider>;
}

export function useAppNav() {
  const ctx = useContext(AppNavContext);
  if (!ctx) throw new Error("useAppNav must be used inside AppNavProvider");
  return ctx;
}
