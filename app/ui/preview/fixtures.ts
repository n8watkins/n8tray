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
  pinned: true,
  inMainView: true,
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

/** The pinned headline's figures, as the host writes them (D32). */
const pinnedCounts = (running: number, total: number, apps = total) => ({
  pinnedRunning: running,
  pinnedTotal: total,
  pinnedText: apps === 0 ? "No apps" : total === 0 ? "Nothing pinned" : `${running} of ${total} running`,
});

export const busy: HostSnapshot = {
  version: "0.2.0",
  level: "attention",
  headline: "Needs attention",
  reason: "Lead Scraper is over its 1 GB warning",
  running: 3,
  total: 4,
  // From the apps below (the older `running` and `total` above are not).
  ...pinnedCounts(3, 4),
  more: null,
  memory: { n8Mb: 1950, text: "N8 1.9 GB in all" },
  pressure: "ok",
  // The host's order: what runs first, then what is stopped; by name within.
  apps: [leadscraper, hotkeys, voicedock, commentary],
  problems: [],
  tray: tray(true),
  // As the host writes it (crates/n8tray-core/tests/native_fixtures.rs checks every fixture's).
  tooltip: "N8 Tray: Needs attention\nRunning: Lead Scraper, N8 Hotkeys, VoiceDock\nN8 1.9 GB in all",
  legacyTray: false,
  settings,
  update: { channel: "stable", available: null, url: null, checkedAt: null, error: null, install: "idle", progress: null, installError: null },
};

export const healthy: HostSnapshot = {
  ...busy,
  level: "ok",
  headline: "Healthy",
  reason: null,
  running: 2,
  total: 3,
  ...pinnedCounts(2, 3),
  memory: { n8Mb: 780, text: "N8 780 MB in all" },
  apps: [hotkeys, voicedock, commentary],
  tooltip: "N8 Tray: Healthy\nRunning: N8 Hotkeys, VoiceDock\nN8 780 MB in all",
};

export const trouble: HostSnapshot = {
  ...busy,
  level: "problem",
  headline: "Problem",
  reason: "VoiceDock: Stopped: crashed",
  running: 1,
  total: 3,
  ...pinnedCounts(1, 3),
  apps: [
    { ...hotkeys, pending: "Restarting" },
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
    { ...commentary, actions: commentary.actions.map((a) => (a.kind === "start" ? { ...a, confirm: true } : a)) },
  ],
  problems: [{ file: "broken.json", id: "broken", message: "Manifest invalid: /status: \"type\" is required" }],
  tooltip: "N8 Tray: Problem\nRunning: N8 Hotkeys\nN8 1.9 GB in all",
  legacyTray: false,
  update: { channel: "stable", available: "0.3.0", url: "https://github.com/n8watkins/n8tray/releases/tag/v0.3.0", checkedAt: null, error: null, install: "idle", progress: null, installError: null },
};

export const empty: HostSnapshot = {
  ...busy,
  level: "off",
  headline: "No apps",
  reason: null,
  running: 0,
  total: 0,
  ...pinnedCounts(0, 0),
  apps: [],
  memory: { n8Mb: 15, text: "N8 15 MB in all" },
  tooltip: "N8 Tray: No apps\nN8 15 MB in all",
};

/** Healthy, while an update downloads. */
export const updating: HostSnapshot = {
  ...healthy,
  update: { ...trouble.update, install: "downloading", progress: 42 },
};

/** Healthy, after an update failed to install. */
export const updateFailed: HostSnapshot = {
  ...healthy,
  update: { ...trouble.update, install: "failed", installError: "the signature does not match N8 Tray's key" },
};

/**
 * The owner's PC while the PowerShell N8 Tray runs: this one watches, one
 * app runs and needs a look, three are stopped. Watching is not a fault, so
 * the level and its reason stay the apps' (D29).
 */
const watched = (app: AppView): AppView => ({ ...app, actions: app.actions.map((a) => ({ ...a, enabled: false })) });
export const watching: HostSnapshot = {
  ...busy,
  level: "attention",
  headline: "Needs attention",
  reason: "N8 Hotkeys: 5 of 6 hotkeys registered",
  running: 1,
  total: 4,
  ...pinnedCounts(1, 4),
  memory: { n8Mb: 117, text: "N8 117 MB in all" },
  apps: [
    watched({ ...hotkeys, level: "attention", state: "degraded", words: "Ready, with a problem", summary: "5 of 6 hotkeys registered", memoryMb: 2.3, history: null }),
    watched({ ...leadscraper, ...stopped, group: "Watchers", level: "off", alert: null, summary: "Installed but not running", namespacedMb: null, memoryMb: null, history: null, processes: [] }),
    watched({ ...commentary, group: "Commentary" }),
    watched({ ...voicedock, ...stopped, summary: "Installed but not running", memoryMb: null, cpuPercent: null, history: null, processes: [] }),
  ],
  tray: { ...tray(true), rssMb: 18.2, privateMb: 17.4, windowMb: 287.9, windowPrivateMb: 98.0 },
  tooltip: "N8 Tray: Needs attention\nRunning: N8 Hotkeys\nWatching only: the PowerShell N8 Tray manages apps\nN8 117 MB in all",
  legacyTray: true,
};

/**
 * Three apps unpinned (D32): N8 Commentary is stopped and N8 Hotkeys runs,
 * both behind the "2 more" line; Lead Scraper is degraded and over its
 * warning, so it is in the main view until 30 s after that clears. The pinned
 * headline counts the one pinned app; the level, N8's memory, `running` and
 * `total` count every app. The tooltip names what the main view shows.
 */
const unpinned = (app: AppView, inMainView = false): AppView => ({ ...app, pinned: false, inMainView });
export const pinned: HostSnapshot = {
  ...busy,
  reason: "Lead Scraper: Over its 1 GB warning",
  running: 3,
  total: 4,
  ...pinnedCounts(1, 1, 4),
  more: { count: 2, running: 1, memoryMb: 24, text: "2 more · 1 running, 24 MB" },
  memory: { n8Mb: 767.9, text: "N8 768 MB in all" },
  apps: [
    unpinned({ ...leadscraper, state: "degraded", words: "Ready, with a problem", summary: "Over its 1 GB warning" }, true),
    unpinned(hotkeys),
    voicedock,
    unpinned(commentary),
  ],
  tooltip: "N8 Tray: Needs attention\nRunning: Lead Scraper, VoiceDock · 1 more running\nN8 768 MB in all",
};

export const fixtures: Record<string, HostSnapshot> = { busy, healthy, trouble, empty, updating, updateFailed, watching, pinned };
