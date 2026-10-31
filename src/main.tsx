import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/App";
import { JobsProvider } from "@/lib/jobs";
import { ModulesProvider } from "@/lib/modules";
import { SettingsProvider } from "@/lib/settings";
import { ToolsProvider } from "@/lib/tools";
import { UpdatesProvider } from "@/lib/updates";
import { ErrorBoundary } from "@/components/error-boundary";
import { TooltipProvider } from "@/components/ui/tooltip";
import "@/index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <TooltipProvider delayDuration={400}>
        <SettingsProvider>
          <ToolsProvider>
            <ModulesProvider>
              <JobsProvider>
                <UpdatesProvider>
                  <App />
                </UpdatesProvider>
              </JobsProvider>
            </ModulesProvider>
          </ToolsProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
