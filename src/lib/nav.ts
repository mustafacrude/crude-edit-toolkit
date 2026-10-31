import { Download, Film, History, House, Image, type LucideIcon } from "lucide-react";

export type Screen = "home" | "download" | "video" | "images" | "history" | "settings" | "help";

export interface NavItem {
  screen: Screen;
  label: string;
  icon: LucideIcon;
}

/** Top-bar navigation in Pro mode, left to right. Settings lives in the right-hand controls. */
export const NAV: NavItem[] = [
  { screen: "home", label: "Home", icon: House },
  { screen: "download", label: "Download", icon: Download },
  { screen: "video", label: "Video", icon: Film },
  { screen: "images", label: "Images", icon: Image },
  { screen: "history", label: "History", icon: History },
];

/** Screens that keep the queue docked on the right. Elsewhere it opens from the top bar. */
export const SHOWS_QUEUE: ReadonlySet<Screen> = new Set(["home"]);
