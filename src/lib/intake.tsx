// Everything the user drops or pastes lands here: files are inspected, links
// are held for the downloader. Drag-and-drop and Ctrl+V work on every screen.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  expandPaths,
  inspectFile,
  isTauri,
  onFilesOpened,
  skippedNotice,
  takeOpenedFiles,
  type Inspection,
} from "@/lib/tauri";

export type IntakeItem =
  | { key: string; type: "file"; path: string; status: "loading" }
  | {
      key: string;
      type: "file";
      path: string;
      status: "ready";
      inspection: Inspection;
    }
  | { key: string; type: "file"; path: string; status: "error"; error: string }
  | { key: string; type: "link"; url: string }
  | { key: string; type: "image"; image: Blob };

/** What was dropped or pasted, handed to a flow screen that asked for it. */
export interface Drop {
  urls: string[];
  paths: string[];
  /** Pictures pasted from the clipboard (a browser's Copy image). */
  images: Blob[];
}

interface IntakeState {
  /** Newest first. */
  items: IntakeItem[];
  /** True while files are being dragged over the window. */
  dragging: boolean;
  /** Something the person should know about the last drop (a folder too big to show in full). */
  notice: string | null;
  addPaths: (paths: string[], options?: { toHome?: boolean }) => Promise<void>;
  /** A screen that wants drops and pastes itself, instead of Home's cards. Null to stop. */
  setSink: (sink: ((drop: Drop) => void) | null) => void;
  remove: (key: string) => void;
  clear: () => void;
}

const IntakeContext = createContext<IntakeState | null>(null);

let counter = 0;
const newKey = () => `item-${++counter}`;

/** Explorer's "Copy as path" wraps paths in quotes. */
const unquote = (s: string) => s.replace(/^"(.*)"$/, "$1");
const isUrl = (s: string) => /^https?:\/\/\S+$/i.test(s);
const isPath = (s: string) => /^[a-zA-Z]:[\\/]|^\\\\|^\//.test(s);

export function IntakeProvider({
  children,
  onArrive,
}: {
  children: ReactNode;
  onArrive: () => void;
}) {
  const [items, setItems] = useState<IntakeItem[]>([]);
  const [dragging, setDragging] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const arrive = useRef(onArrive);
  arrive.current = onArrive;
  const sink = useRef<((drop: Drop) => void) | null>(null);
  const setSink = useCallback((next: ((drop: Drop) => void) | null) => {
    sink.current = next;
  }, []);

  const update = (key: string, next: IntakeItem) =>
    setItems((prev) => prev.map((item) => (item.key === key ? next : item)));

  const addPaths = useCallback(
    async (raw: string[], options?: { toHome?: boolean }) => {
      if (raw.length === 0) return;
      if (sink.current && !options?.toHome) {
        sink.current({ urls: [], paths: raw, images: [] });
        return;
      }
      const expanded = await expandPaths(raw).catch(() => ({
        files: raw,
        skipped: 0,
      }));
      const paths = expanded.files;
      setNotice(skippedNotice(paths.length, expanded.skipped));
      if (paths.length === 0) return;
      const fresh = paths.map((path) => ({
        key: newKey(),
        type: "file" as const,
        path,
        status: "loading" as const,
      }));
      setItems((prev) => [
        ...fresh,
        // Dropping the same file again replaces its old card.
        ...prev.filter(
          (item) => item.type !== "file" || !paths.includes(item.path),
        ),
      ]);
      arrive.current();

      // Inspect a few at a time so a big folder doesn't start dozens of ffprobes.
      const queue = [...fresh];
      const worker = async () => {
        for (let item = queue.shift(); item; item = queue.shift()) {
          const { key, path } = item;
          await inspectFile(path)
            .then((inspection) =>
              update(key, {
                key,
                type: "file",
                path,
                status: "ready",
                inspection,
              }),
            )
            .catch((e) =>
              update(key, {
                key,
                type: "file",
                path,
                status: "error",
                error: String(e),
              }),
            );
        }
      };
      await Promise.all([worker(), worker(), worker()]);
    },
    [],
  );

  const addText = useCallback(
    (text: string): boolean => {
      const lines = text
        .split(/\r?\n/)
        .map((l) => unquote(l.trim()))
        .filter(Boolean);
      const urls = lines.filter(isUrl);
      const paths = lines.filter(isPath);
      if (urls.length === 0 && paths.length === 0) return false;
      if (sink.current) {
        sink.current({ urls, paths, images: [] });
        return true;
      }
      if (urls.length > 0) {
        setItems((prev) => [
          ...urls.map((url) => ({ key: newKey(), type: "link" as const, url })),
          ...prev.filter(
            (item) => item.type !== "link" || !urls.includes(item.url),
          ),
        ]);
        arrive.current();
      }
      void addPaths(paths);
      return true;
    },
    [addPaths],
  );

  // Native drag-and-drop from Explorer/Finder. Tauri hands us real file paths.
  useEffect(() => {
    if (!isTauri()) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === "enter" || payload.type === "over")
          setDragging(true);
        else if (payload.type === "leave") setDragging(false);
        else if (payload.type === "drop") {
          setDragging(false);
          void addPaths(payload.paths);
        }
      })
      .then((unlisten) => (cancelled ? unlisten() : (stop = unlisten)));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [addPaths]);

  // Files sent to the app from Explorer ("Send to Crude's Toolkit"), at launch or while it runs.
  useEffect(() => {
    if (!isTauri()) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    // The waiting list is the source of truth; the event only says "look again".
    const collect = () =>
      void takeOpenedFiles().then(
        (paths) => !cancelled && void addPaths(paths, { toHome: true }),
      );
    collect();
    void onFilesOpened(collect).then((unlisten) =>
      cancelled ? unlisten() : (stop = unlisten),
    );
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [addPaths]);

  // Ctrl+V anywhere that isn't a text field: links, copied file paths and copied pictures.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable='true']")) return;
      const text = e.clipboardData?.getData("text") ?? "";
      if (addText(text)) {
        e.preventDefault();
        return;
      }
      const images = [...(e.clipboardData?.files ?? [])].filter((f) =>
        f.type.startsWith("image/"),
      );
      if (images.length === 0) return;
      e.preventDefault();
      if (sink.current) {
        sink.current({ urls: [], paths: [], images });
        return;
      }
      setItems((prev) => [
        ...images.map((image) => ({
          key: newKey(),
          type: "image" as const,
          image,
        })),
        ...prev,
      ]);
      arrive.current();
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, [addText]);

  const remove = useCallback(
    (key: string) => setItems((prev) => prev.filter((i) => i.key !== key)),
    [],
  );
  const clear = useCallback(() => {
    setItems([]);
    setNotice(null);
  }, []);

  return (
    <IntakeContext.Provider
      value={{ items, dragging, notice, addPaths, setSink, remove, clear }}
    >
      {children}
    </IntakeContext.Provider>
  );
}

export function useIntake() {
  const ctx = useContext(IntakeContext);
  if (!ctx) throw new Error("useIntake must be used inside IntakeProvider");
  return ctx;
}

/** While mounted, drops and pastes go to `onDrop` (this screen's own flow) instead of becoming cards on Home. */
export function useIntakeSink(onDrop: (drop: Drop) => void) {
  const { setSink } = useIntake();
  const latest = useRef(onDrop);
  latest.current = onDrop;
  useEffect(() => {
    setSink((drop) => latest.current(drop));
    return () => setSink(null);
  }, [setSink]);
}
