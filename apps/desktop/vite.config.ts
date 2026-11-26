import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

import { validateDevDirsRequest } from "./devPickerGuard";

function dennettDevDirPickerPlugin(): Plugin {
  return {
    name: "dennett-dev-dir-picker",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/__dennett_dev_dirs", (req, res) => {
        const verdict = validateDevDirsRequest(req.method, req.headers);
        if (!verdict.ok) {
          res.statusCode = verdict.status;
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify({ error: verdict.message }));
          return;
        }
        try {
          const url = new URL(req.url ?? "/", "http://127.0.0.1");
          const rawPath = url.searchParams.get("path")?.trim() ?? "";
          const requested = rawPath || os.homedir() || process.cwd();
          const resolved = path.resolve(requested);
          const stat = fs.lstatSync(resolved);
          if (stat.isSymbolicLink() || !stat.isDirectory()) {
            res.statusCode = 400;
            res.setHeader("Content-Type", "application/json");
            res.end(JSON.stringify({ error: "Selected path is not a regular directory" }));
            return;
          }

          const entries = fs.readdirSync(resolved, { withFileTypes: true });
          const dirs: Array<{ name: string; path: string }> = [];
          for (const entry of entries) {
            if (dirs.length >= 120) {
              break;
            }
            if (entry.isSymbolicLink()) {
              continue;
            }
            const fullPath = path.join(resolved, entry.name);
            try {
              const childStat = fs.lstatSync(fullPath);
              if (childStat.isDirectory() && !childStat.isSymbolicLink()) {
                dirs.push({ name: entry.name, path: fullPath });
              }
            } catch {
              // Skip unreadable OS directories cleanly.
            }
          }
          dirs.sort((a, b) => {
            const aDot = a.name.startsWith(".");
            const bDot = b.name.startsWith(".");
            if (aDot !== bDot) {
              return aDot ? 1 : -1;
            }
            return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
          });

          const parentDir = path.dirname(resolved);
          const parent = parentDir !== resolved ? parentDir : null;

          res.statusCode = 200;
          res.setHeader("Content-Type", "application/json");
          res.end(
            JSON.stringify({
              current: resolved,
              parent,
              home: os.homedir(),
              temp: os.tmpdir(),
              dirs,
            }),
          );
        } catch (err) {
          res.statusCode = 400;
          res.setHeader("Content-Type", "application/json");
          res.end(
            JSON.stringify({
              error: err instanceof Error ? err.message : "Failed to inspect directory",
            }),
          );
        }
      });
    },
  };
}

const devHeadToken = process.env.DENNETT_HEAD_TOKEN?.trim();

export default defineConfig({
  plugins: [react(), dennettDevDirPickerPlugin()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    cors: false,
    proxy: {
      "/__dennett_dev_head": {
        target: process.env.DENNETT_HEAD_URL || "http://127.0.0.1:41771",
        changeOrigin: false,
        ...(devHeadToken ? { headers: { Authorization: `Bearer ${devHeadToken}` } } : {}),
        rewrite: (reqPath) => reqPath.replace(/^\/__dennett_dev_head/, ""),
      },
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
});
