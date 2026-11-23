import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { isTauriRuntime } from "./api/bridge";
import { App } from "./App";
import "./styles/tokens.css";
import "./styles/desktop.css";

// Native Tauri window keeps HTML/body transparent for Windows 11 Mica;
// plain browser preview gets a dark fallback canvas without fake wallpaper hacks.
document.documentElement.dataset.surface = isTauriRuntime() ? "mica" : "browser-preview";

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Missing #root mount node");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
