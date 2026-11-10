// The window outside Tauri, for the screenshot tests and for review: the
// real UI against a recorded snapshot, through Tauri's IPC mock. Query:
// ?fixture=busy|healthy|trouble|empty&view=hub|details|settings|history|panel:<id>
import { mockIPC } from "@tauri-apps/api/mocks";

import { fixtures, series } from "./fixtures";
import { busy } from "./fixtures";

const icons = import.meta.glob("../../../spec/examples/icons/*.svg", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

const params = new URLSearchParams(location.search);
const snapshot = fixtures[params.get("fixture") ?? "busy"] ?? busy;
const view = params.get("view") ?? "hub";

declare global {
  interface Window {
    __preview: { calls: { cmd: string; args: unknown }[]; height: number | null };
  }
}
window.__preview = { calls: [], height: null };

mockIPC(
  (cmd, args) => {
    window.__preview.calls.push({ cmd, args });
    const a = (args ?? {}) as Record<string, string>;
    switch (cmd) {
      case "get_snapshot":
        return snapshot;
      case "initial_view":
        return view.startsWith("panel:") ? "hub" : view;
      case "app_icon": {
        const app = snapshot.apps.find((x) => x.id === a.id);
        const file = app?.icon?.split("/").pop();
        const svg = Object.entries(icons).find(([k]) => k.endsWith(`/${file}`))?.[1];
        return svg ? `data:image/svg+xml;base64,${btoa(svg)}` : null;
      }
      case "history": {
        const n = a.range === "day" ? 288 : 168;
        const seed = a.id.length;
        return { intervalSeconds: a.range === "day" ? 300 : 3600, memoryMb: series(200 + seed * 40, 80, seed, n), cpuPercent: series(4, 6, seed + 1, n) };
      }
      case "recent_warnings":
        return [
          { at: "18:02", level: "warn", message: "leadscraper: Over its 1 GB warning", count: 1 },
          { at: "17:41", level: "warn", message: "voicedock: could not start: bin/voicedock-hub.exe is missing", count: 3 },
        ];
      case "status_text":
        return "N8 Tray 0.2.0: Needs attention";
      case "fit_window":
        window.__preview.height = Number((args as { height: number }).height);
        return null;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

await import("../src/main");
if (view.startsWith("panel:")) {
  // Open the panel the way a user does: click the app's row.
  const id = view.slice("panel:".length);
  const name = snapshot.apps.find((x) => x.id === id)?.name ?? id;
  const until = Date.now() + 3000;
  while (Date.now() < until) {
    const row = Array.from(document.querySelectorAll<HTMLButtonElement>(".app-main")).find((b) => b.textContent?.includes(name));
    if (row) {
      row.click();
      break;
    }
    await new Promise((r) => setTimeout(r, 20));
  }
}
