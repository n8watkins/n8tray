// Recorded host snapshots for the preview and the screenshot tests: the same
// shape the host sends (the generated bindings), so a change to the Rust
// types breaks this file's type check rather than the pictures.
import type { AppView } from "../src/bindings/AppView";
import type { HistoryView } from "../src/bindings/HistoryView";
import type { HostSnapshot } from "../src/bindings/HostSnapshot";
import type { ProcessView } from "../src/bindings/ProcessView";

/** A deterministic 15-minute series (90 steps of 10 s). */
export function series(base: number, wobble: number, seed: number, steps = 90, ramp = 0): number[] {
  let x = seed;
  const out: number[] = [];
  for (let i = 0; i < steps; i++) {
    x = (x * 9301 + 49297) % 233280;
    out.push(Math.max(0, Math.round((base + (ramp * i) / steps + (x / 233280 - 0.5) * wobble) * 10) / 10));
  }
  return out;
}

function hist(mem: number[], cpu: number[]): HistoryView {
  return { intervalSeconds: 10, memoryMb: mem, cpuPercent: cpu };
}

const started = "2026-11-09T16:01:40.000Z";

function proc(pid: number, name: string, rss: number, cpu: number, extra: Partial<ProcessView> = {}): ProcessView {
  return { pid, role: null, component: null, name, rssMb: rss, privateMb: Math.round(rss * 0.9), cpuPercent: cpu, startedAt: started, verdict: "verified", note: null, ...extra };
}

const base: Omit<AppView, "id" | "name"> = {
  group: null,
  description: null,
  icon: null,
  version: "1.0.0",
  level: "ok",
  state: "ready",
  words: "Ready",
  summary: "Ready",
  detail: null,
  running: true,
  shouldRun: true,
  enabled: true,
  hosted: true,
  autoStart: true,
  pending: null,
  lastOutcome: null,
  actions: [
    { id: "start", label: "Start", kind: "start", enabled: false, perComponent: false, confirm: false },
    { id: "stop", label: "Stop", kind: "stop", enabled: true, perComponent: false, confirm: false },
    { id: "restart", label: "Restart", kind: "restart", enabled: true, perComponent: false, confirm: false },
  ],
  components: [],
  open: [],
  menu: [],
  memoryMb: null,
  privateMb: null,
  cpuPercent: null,
  gpu: null,
  namespacedMb: null,
  processes: [],
  alert: null,
  history: null,
  notes: [],
};

const stopped = {
  running: false,
  shouldRun: false,
  state: "stopped",
  words: "Stopped",
  level: "off" as const,
  actions: [
    { id: "start", label: "Start", kind: "start" as const, enabled: true, perComponent: false, confirm: false },
    { id: "stop", label: "Stop", kind: "stop" as const, enabled: false, perComponent: false, confirm: false },
    { id: "restart", label: "Restart", kind: "restart" as const, enabled: false, perComponent: false, confirm: false },
  ],
};

const voicedock: AppView = {
  ...base,
  id: "voicedock",
  name: "VoiceDock",
  group: "Audio",
  description: "Local voices for the VoiceDock extension.",
  icon: "icons/voicedock.svg",
  version: "1.3.0",
  summary: "Kokoro ready · Piper idle",
  memoryMb: 643,
  privateMb: 627,
  cpuPercent: 12,
  history: hist(series(620, 60, 7, 90, 30), series(10, 10, 21)),
  actions: [
    ...base.actions,
    { id: "engine.start", label: "Start {component}", kind: "app", enabled: true, perComponent: true, confirm: false },
    { id: "engine.stop", label: "Stop {component}", kind: "app", enabled: true, perComponent: true, confirm: false },
  ],
  components: [
    { id: "kokoro", name: "Kokoro", state: "ready", words: "Ready", level: "ok", detail: "Model loaded, unloads in 7 min", memoryMb: 598 },
    { id: "piper", name: "Piper", state: "idle", words: "Idle", level: "ok", detail: "8 voices, no process while idle", memoryMb: 0 },
    { id: "pocket", name: "Pocket TTS", state: "stopped", words: "Stopped", level: "off", detail: "Not set up · about 240 MB · needs uv", memoryMb: null },
  ],
  open: [
    { key: "settings", label: "Settings", target: "http://127.0.0.1:7477/settings", exists: true },
    { key: "logs", label: "Log", target: "C:\\apps\\voicedock\\logs", exists: true },
  ],
  menu: [
    { label: "Engine history", kind: "open-url", target: "http://127.0.0.1:7477/report", exists: true },
    { label: "Open voices folder", kind: "open-path", target: "C:\\apps\\voicedock\\voices", exists: true },
  ],
  processes: [
    proc(14820, "voicedock-hub.exe", 31, 0.1, { role: "hub" }),
    proc(22016, "python.exe", 612, 11.8, { role: "engine", component: "kokoro" }),
    proc(22020, "conhost.exe", 7, 0, { verdict: "unreported", note: "Started by python.exe; the app did not report it" }),
  ],
};

const hotkeys: AppView = {
  ...base,
  id: "n8hotkeys",
  name: "N8 Hotkeys",
  group: "Hotkeys",
  icon: "icons/n8hotkeys.svg",
  version: "2.0.0",
  summary: "6 hotkeys registered",
  memoryMb: 24,
  privateMb: 9,
  cpuPercent: 0,
  history: hist(series(24, 2, 3), series(0.2, 0.3, 4)),
  processes: [proc(9120, "n8hotkeys.exe", 24, 0)],
};

const commentary: AppView = {
  ...base,
  ...stopped,
  id: "n8commentary",
  name: "N8 Commentary",
  group: "Streaming",
  icon: "icons/n8commentary.svg",
  version: "0.10.0",
  summary: "Installed but not running",
  autoStart: false,
  open: [
    { key: "settings", label: "Settings", target: "C:\\apps\\n8commentary\\settings.json", exists: true },
    { key: "logs", label: "Log", target: "C:\\apps\\n8commentary\\logs\\n8commentary.log", exists: false },
  ],
};

const leadscraper: AppView = {
  ...base,
  id: "leadscraper",
  name: "Lead Scraper",
  group: "Work",
  icon: "icons/leadscraper.svg",
  version: "2.0.0",
  level: "attention",
  summary: "Sweeping pool cell 14 of 40",
  alert: "Over its 1 GB warning",
  memoryMb: 0,
  namespacedMb: 1230,
  cpuPercent: 3,
  history: hist(series(1150, 140, 11, 90, 120), series(3, 4, 12)),
  processes: [proc(30112, "powershell.exe", 0, 0.1, { rssMb: 0 }), proc(0, "node (wsl:Ubuntu-24.04)", 1230, 3, { verdict: "reported" })],
};

const wslmemory: AppView = {
  ...base,
  id: "wslmemory",
  name: "WSL Memory",
  group: "System",
  icon: "icons/wslmemory.svg",
  summary: "10.5 of 19.5 GB free · 3 dev servers",
  actions: [],
};

export const tray = (windowOpen: boolean) => ({
  pid: 9124,
  rssMb: 15.2,
  windowMb: windowOpen ? 96.4 : 0,
  privateMb: 12.8,
  windowPrivateMb: windowOpen ? 88.1 : null,
  cpuPercent: 0.1,
  history: hist(series(14, 1.5, 5), series(0.1, 0.1, 6)),
});

const settings = {
  launchAtSignIn: true,
  signInForeign: false,
  notifications: true,
  updateChannel: "stable",
  checkUpdates: true,
  error: null,
  file: "C:\\Users\\someone\\AppData\\Local\\N8Tray\\settings.json",
};

export const busy: HostSnapshot = {
  version: "0.2.0",
  level: "attention",
  headline: "Needs attention",
  reason: "Lead Scraper is over its 1 GB warning",
  running: 3,
  total: 4,
  memory: { freeGb: 4.2, totalGb: 31.1, level: "ok", n8Mb: 1950, system: "Windows", text: "Windows 4.2/31.1 GB free · N8 1.9 GB" },
  pressure: "ok",
  apps: [voicedock, hotkeys, commentary, wslmemory, leadscraper],
  problems: [],
  tray: tray(true),
  tooltip: "N8 Tray: Needs attention",
  legacyTray: false,
  settings,
  update: { channel: "stable", available: null, url: null, checkedAt: null, error: null },
};

export const healthy: HostSnapshot = {
  ...busy,
  level: "ok",
  headline: "Healthy",
  reason: null,
  running: 2,
  total: 3,
  memory: { ...busy.memory, n8Mb: 780, text: "Windows 12.6/31.1 GB free · N8 0.8 GB", freeGb: 12.6 },
  apps: [voicedock, hotkeys, commentary],
};

export const trouble: HostSnapshot = {
  ...busy,
  level: "problem",
  headline: "Problem",
  reason: "VoiceDock: Stopped: crashed",
  running: 1,
  total: 3,
  memory: { ...busy.memory, freeGb: 1.3, level: "problem", text: "Windows 1.3/31.1 GB free · N8 0.1 GB" },
  apps: [
    {
      ...voicedock,
      ...stopped,
      level: "problem",
      state: "failed",
      words: "Stopped: crashed",
      summary: "Crashed 3 times in 10 minutes; not restarted again",
      shouldRun: true,
      memoryMb: null,
      cpuPercent: null,
      history: null,
      processes: [],
      lastOutcome: { action: "start", outcome: "failed", message: "Start failed: bin/voicedock-hub.exe is missing" },
    },
    { ...hotkeys, pending: "Restarting" },
    { ...commentary, actions: commentary.actions.map((a) => (a.kind === "start" ? { ...a, confirm: true } : a)) },
  ],
  problems: [{ file: "broken.json", id: "broken", message: "Manifest invalid: /status: \"type\" is required" }],
  legacyTray: true,
  update: { channel: "stable", available: "0.3.0", url: "https://github.com/n8watkins/n8tray/releases/tag/v0.3.0", checkedAt: null, error: null },
};

export const empty: HostSnapshot = {
  ...busy,
  level: "off",
  headline: "No apps",
  reason: null,
  running: 0,
  total: 0,
  apps: [],
  memory: { ...busy.memory, n8Mb: 15, text: "Windows 12.6/31.1 GB free · N8 0.0 GB", freeGb: 12.6 },
};

export const fixtures: Record<string, HostSnapshot> = { busy, healthy, trouble, empty };
