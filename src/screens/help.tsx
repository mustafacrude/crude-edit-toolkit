import { isValidElement, useRef, type ReactNode } from "react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
// The same file the repo shows, so the in-app handbook never drifts from docs/.
import rawHandbook from "../../docs/handbook.md?raw";

// The in-file contents list duplicates the sidebar here, so it's left out.
const handbook = rawHandbook
  .replace(/\*\*Contents\*\*[\s\S]*?\r?\n---\r?\n/, "")
  // The pointer to the developer docs is for the repo page, not for people using the app.
  .replace(/\s*\(Developers:[^)]*\)/, "");

/** GitHub-style heading ids, which the handbook's own "#9-safety-and-limits" links use. */
const slug = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .trim()
    .replace(/\s/g, "-");

function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement<{ children?: ReactNode }>(node)) return textOf(node.props.children);
  return "";
}

const SECTIONS = [...handbook.matchAll(/^## (.+)$/gm)].map((m) => ({ title: m[1], id: slug(m[1]) }));

function jumpTo(id: string) {
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  document.getElementById(id)?.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" });
}

const COMPONENTS: Components = {
  h1: ({ children }) => <h1 className="text-[26px] font-semibold tracking-tight">{children}</h1>,
  h2: ({ children }) => (
    <h2 id={slug(textOf(children))} className="mt-10 scroll-mt-4 border-b pb-1.5 text-xl font-semibold tracking-tight">
      {children}
    </h2>
  ),
  h3: ({ children }) => (
    <h3 id={slug(textOf(children))} className="mt-6 scroll-mt-4 text-base font-semibold">
      {children}
    </h3>
  ),
  p: ({ children }) => <p className="max-w-[54ch] leading-relaxed text-pretty">{children}</p>,
  ul: ({ children }) => <ul className="flex max-w-[54ch] list-disc flex-col gap-1.5 pl-5 leading-relaxed marker:text-muted-foreground">{children}</ul>,
  ol: ({ children }) => <ol className="flex max-w-[54ch] list-decimal flex-col gap-1.5 pl-5 leading-relaxed marker:text-muted-foreground">{children}</ol>,
  hr: () => <hr className="my-2 border-border" />,
  strong: ({ children }) => <strong className="font-semibold text-foreground">{children}</strong>,
  code: ({ children }) => (
    <code className="rounded bg-muted px-1 py-px font-mono text-[12.5px] text-foreground">{children}</code>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto rounded-lg border">
      <table className="w-full border-collapse text-[13px]">{children}</table>
    </div>
  ),
  thead: ({ children }) => <thead className="bg-muted/50 text-left">{children}</thead>,
  th: ({ children }) => <th className="px-3 py-2 font-medium">{children}</th>,
  td: ({ children }) => <td className="border-t px-3 py-2 align-top">{children}</td>,
  blockquote: ({ children }) => (
    <blockquote className="rounded-lg border bg-card px-4 py-2 text-muted-foreground">{children}</blockquote>
  ),
  // Only jumps inside the handbook. Links to repo files have nothing to open in the app, so they read as text.
  a: ({ href, children }) =>
    href?.startsWith("#") ? (
      <a
        href={href}
        onClick={(e) => {
          e.preventDefault();
          jumpTo(href.slice(1));
        }}
        className="text-primary-ink underline underline-offset-2 hover:text-foreground"
      >
        {children}
      </a>
    ) : (
      <span>{children}</span>
    ),
};

export function Help() {
  const top = useRef<HTMLDivElement>(null);

  return (
    <div className="mx-auto flex w-full max-w-[960px] gap-8">
      <nav aria-label="Handbook contents" className="sticky top-0 hidden h-fit w-52 shrink-0 flex-col gap-0.5 self-start lg:flex">
        <button
          type="button"
          onClick={() => top.current?.scrollIntoView({ block: "start" })}
          className="mb-1 rounded-md px-2 py-1 text-left text-xs font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          Handbook
        </button>
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => jumpTo(s.id)}
            className="rounded-md px-2 py-1 text-left text-[13px] leading-snug text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
          >
            {s.title.replace(/^\d+\.\s*/, "")}
          </button>
        ))}
      </nav>

      <article ref={top} className="flex min-w-0 max-w-3xl flex-1 flex-col gap-3 pb-16 text-[14px] text-pretty">
        <Markdown remarkPlugins={[remarkGfm]} components={COMPONENTS}>
          {handbook}
        </Markdown>
      </article>
    </div>
  );
}
