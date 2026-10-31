import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

interface State {
  error: Error | null;
}

/** Catches a crash anywhere in the UI so the window shows a way out instead of going blank. */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
        <h1 className="text-lg font-semibold">Something went wrong</h1>
        <p className="max-w-[50ch] text-[13px] text-muted-foreground">
          The toolkit hit a problem it didn't expect. Jobs already running keep going in the background. Reload to get
          back to where you were.
        </p>
        <Button onClick={() => location.reload()}>Reload</Button>
        <pre className="max-w-full overflow-auto font-mono text-[11px] text-muted-foreground/70 selectable">
          {this.state.error.message}
        </pre>
      </div>
    );
  }
}
