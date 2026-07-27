import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/App";
import { loadAppSettings } from "@/features/settings/app-settings";
import { applyStartupWindowPreference } from "@/features/settings/startup-window";
import { ThemeProvider } from "@/features/theme/ThemeProvider";
import "@/index.css";

void applyStartupWindowPreference(loadAppSettings()).catch(() => {
  // A window-manager refusal must not prevent the editor from opening.
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </StrictMode>,
);
