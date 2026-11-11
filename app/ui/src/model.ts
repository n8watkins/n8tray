// Pure functions from the host's snapshot to what the window shows. No
// decisions live here: which actions are possible, levels and texts come from
// Rust; this only picks and words them for one place on screen.
import type { ActionView } from "./bindings/ActionView";
import type { AppView } from "./bindings/AppView";
import type { ComponentView } from "./bindings/ComponentView";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { LevelName } from "./bindings/LevelName";
import type { UpdateView } from "./bindings/UpdateView";
import { cpu, memory } from "./format";

/** The apps the hub shows: hosted and not hidden, in the snapshot's order. */
export function hubApps(snapshot: HostSnapshot): AppView[] {
  return snapshot.apps.filter((a) => a.hosted && a.level !== "hidden");
}

/**
 * The hub's list: one list in the snapshot's order (what runs first, then
 * what needs a look, what is stopped and what is not installed), with no
 * headings. `byState` instead puts a heading over each of those parts.
 */
export function sections(apps: AppView[], byState = false): { name: string | null; apps: AppView[] }[] {
  if (!byState) return apps.length ? [{ name: null, apps }] : [];
  const out: { name: string | null; apps: AppView[] }[] = [];
  for (const app of apps) {
    const name = sectionName(app);
    const last = out[out.length - 1];
    if (last && last.name === name) last.apps.push(app);
    else out.push({ name, apps: [app] });
  }
  return out;
}

function sectionName(app: AppView): string {
  if (app.running || app.pending || app.state === "starting") return "Running";
  if (app.state === "failed") return "Needs a look";
  if (app.state === "not-installed" || app.state === "needs-setup" || app.state === "unsupported") return "Not set up";
  return "Stopped";
}

/** The row's one action: Stop while running, Start while not. */
export function rowAction(app: AppView): ActionView | null {
  const kind = app.running ? "stop" : "start";
  return app.actions.find((a) => a.kind === kind) ?? null;
}

/** Restart, offered while the app runs (the panel and the menu). */
export function restartAction(app: AppView): ActionView | null {
  if (!app.running) return null;
  return app.actions.find((a) => a.kind === "restart") ?? null;
}

/** The app's own actions that are not per component (the panel). */
export function appActions(app: AppView): ActionView[] {
  return app.actions.filter((a) => a.kind === "app" && !a.perComponent);
}

/** Per-component actions with the component's name in their label. */
export function componentActions(app: AppView, component: ComponentView): { action: ActionView; label: string }[] {
  return app.actions
    .filter((a) => a.perComponent)
    .map((action) => ({ action, label: action.label.replace("{component}", component.name) }));
}

export type Tone = "normal" | "attention" | "problem";

/** The row's second line: what happens now, an alert, a failed action, or the summary. */
export function rowLine(app: AppView): { text: string; tone: Tone } {
  if (app.pending) return { text: app.pending, tone: "normal" };
  const failed = app.lastOutcome && (app.lastOutcome.outcome === "failed" || app.lastOutcome.outcome === "refused");
  if (failed) {
    const o = app.lastOutcome!;
    return { text: o.message ?? `${o.action} ${o.outcome}`, tone: "problem" };
  }
  if (app.alert) return { text: app.alert, tone: app.level === "problem" ? "problem" : "attention" };
  const tone: Tone = app.level === "problem" ? "problem" : app.level === "attention" ? "attention" : "normal";
  return { text: app.summary, tone };
}

/** The figures at the right of a row: memory, then CPU or where it runs. */
export function rowFigures(app: AppView): { memory: string; sub: string } | null {
  const measured = app.memoryMb ?? 0;
  const reported = app.namespacedMb ?? 0;
  if (app.memoryMb === null && app.namespacedMb === null) return null;
  if (!app.running && measured + reported === 0) return null;
  return {
    memory: memory(measured + reported),
    // Most of it inside WSL, where the app reports it itself.
    sub: reported > measured ? "in WSL" : cpu(app.cpuPercent),
  };
}

/** The header pill's text. */
export function levelWords(level: LevelName): string {
  switch (level) {
    case "ok":
      return "Healthy";
    case "attention":
      return "Needs attention";
    case "problem":
      return "Problem";
    default:
      return "Idle";
  }
}

/** The status card's lines. */
export function statusCard(s: HostSnapshot): { big: string; small: string } {
  const n8 = `N8 ${memory(s.memory.n8Mb)} in all`;
  if (s.total === 0) return { big: s.reason ?? "No apps installed yet", small: n8 };
  const running = `${s.running} of ${s.total} running`;
  if (s.reason) return { big: s.reason, small: `${running} · ${n8}` };
  if (s.running === s.total) return { big: "Everything is running", small: `${running} · ${n8}` };
  return { big: running, small: n8 };
}

/** The memory meter: used share of the system's memory, and its words. */
export function memoryMeter(s: HostSnapshot): { label: string; text: string; used: number; level: LevelName } | null {
  const { freeGb, totalGb } = s.memory;
  if (freeGb === null || totalGb === null || totalGb <= 0) return null;
  return {
    label: s.memory.system,
    text: `${freeGb.toFixed(1)} / ${totalGb.toFixed(1)} GB free`,
    used: Math.min(1, Math.max(0, 1 - freeGb / totalGb)),
    level: s.memory.level,
  };
}

/**
 * Points of a sparkline or chart: values scaled into a w x h box, from
 * index `from` on (earlier steps keep their place on the time axis).
 */
export function points(values: number[], w: number, h: number, max?: number, pad = 2, from = 0): string {
  if (values.length === 0 || from >= values.length) return "";
  const top = (max ?? Math.max(...values)) * 1.15 || 1;
  const step = values.length > 1 ? w / (values.length - 1) : 0;
  return values
    .slice(from)
    .map((v, i) => `${((i + from) * step).toFixed(1)},${(h - 1 - (Math.min(v, top) / top) * (h - pad - 1)).toFixed(1)}`)
    .join(" ");
}

/**
 * Where the latest run starts: steps before it are zero because the app was
 * not running, and a line through them would read as a drop to nothing.
 */
export function runStart(values: number[]): number {
  let i = values.length;
  while (i > 0 && values[i - 1] > 0) i--;
  return i;
}

/** Whether a history has anything to draw. */
export function hasHistory(values: number[] | undefined | null): boolean {
  return !!values && values.some((v) => v > 0);
}

/** A start time as the local hour and minute. */
export function since(startedAt: string | undefined): string {
  if (!startedAt) return "-";
  const d = new Date(startedAt);
  if (Number.isNaN(d.getTime())) return "-";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** What the window says about an update, and which button it offers. */
export type UpdateLine = {
  text: string;
  /** A second, smaller line: why it failed. */
  detail: string | null;
  /** `install` to start, `retry` after a failure, none while it runs. */
  action: "install" | "retry" | null;
  /** The download's share (0 to 1), while it is known. */
  progress: number | null;
  problem: boolean;
};

export function updateLine(u: UpdateView): UpdateLine | null {
  const v = u.available;
  if (!v) return null;
  switch (u.install) {
    case "downloading":
      return {
        text: u.progress === null ? `Downloading N8 Tray ${v}` : `Downloading N8 Tray ${v}: ${u.progress}%`,
        detail: null,
        action: null,
        progress: u.progress === null ? null : u.progress / 100,
        problem: false,
      };
    case "installing":
      return { text: `Installing N8 Tray ${v}`, detail: "N8 Tray starts again by itself", action: null, progress: 1, problem: false };
    case "failed":
      return { text: `Update to ${v} failed`, detail: u.installError, action: "retry", progress: null, problem: true };
    default:
      return { text: `N8 Tray ${v} is available`, detail: null, action: "install", progress: null, problem: false };
  }
}
