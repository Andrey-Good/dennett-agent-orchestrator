import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";

import { isTauriRuntime } from "./api/bridge";
import { App } from "./App";
import "./styles/tokens.css";
import "./styles/desktop.css";

// Native Tauri window keeps HTML/body transparent for Windows 11 Mica with zero backdrop-filter;
// plain browser dev/review preview gets a dark Mica-toned ambient backdrop.
const native = isTauriRuntime();
document.documentElement.classList.toggle("native-shell", native);
document.documentElement.classList.toggle("browser-mica-preview", !native);
document.documentElement.dataset.surface = native ? "mica" : "browser-preview";

if (native) {
  void invoke<boolean>("is_mica_available")
    .then((available) => {
      document.documentElement.classList.toggle("native-mica-unavailable", !available);
      if (!available) {
        document.documentElement.dataset.surface = "solid-fallback";
      }
    })
    .catch(() => {
      // Keep default native shell styling if IPC is unavailable in tests.
    });
}

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Missing #root mount node");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
