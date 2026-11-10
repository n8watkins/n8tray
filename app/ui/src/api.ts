// The only ways the window talks to the host: the commands the Tauri
// capability allows (capabilities/hub.json), and the events Rust sends. The
// window holds no state of its own beyond the latest snapshot and which view
// it shows.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { HistoryView } from "./bindings/HistoryView";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { WarningView } from "./bindings/WarningView";

export type ViewName = "hub" | "details" | "settings" | "history";
export type HistoryRange = "day" | "week";
export type SettingKey =
  | "launchAtSignIn"
  | "notifications"
  | "checkUpdates"
  | "betaUpdates"
  | "enabled"
  | "autoStart"
  | "hosted";

export function getSnapshot(): Promise<HostSnapshot | null> {
  return invoke<HostSnapshot | null>("get_snapshot");
}

export function onSnapshot(handler: (snapshot: HostSnapshot) => void): Promise<UnlistenFn> {
  return listen<HostSnapshot>("snapshot", (event) => handler(event.payload));
}

/** Rust asks for a view when it shows the window again (`hub` or `details`). */
export function onNavigate(handler: (view: ViewName) => void): Promise<UnlistenFn> {
  return listen<ViewName>("navigate", (event) => handler(event.payload));
}

/** Rust says whether the window is shorter than its content (it scrolls). */
export function onFit(handler: (clipped: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("fit", (event) => handler(event.payload));
}

export function moduleAction(id: string, action: string, component?: string): Promise<void> {
  return invoke("module_action", { id, action, component: component ?? null });
}

export function openItem(id: string, kind: "open" | "menu", key: string): Promise<void> {
  return invoke("open_item", { id, kind, key });
}

export function hideWindow(): Promise<void> {
  return invoke("hide_window");
}

export function appIcon(id: string): Promise<string | null> {
  return invoke<string | null>("app_icon", { id });
}

export function setSetting(key: SettingKey, on: boolean, app?: string): Promise<void> {
  return invoke("set_setting", { key, on, app: app ?? null });
}

export function hostCommand(name: "reloadSettings" | "checkUpdates"): Promise<void> {
  return invoke("host_command", { name });
}

export function openHost(target: "log" | "folder" | "settings"): Promise<void> {
  return invoke("open_host", { target });
}

export function openUpdate(): Promise<void> {
  return invoke("open_update");
}

export function history(id: string, range: HistoryRange): Promise<HistoryView> {
  return invoke<HistoryView>("history", { id, range });
}

export function recentWarnings(): Promise<WarningView[]> {
  return invoke<WarningView[]>("recent_warnings");
}

export function statusText(): Promise<string | null> {
  return invoke<string | null>("status_text");
}

/** The window drew itself this tall (CSS pixels): Rust sizes, places and shows it. */
export function fitWindow(height: number): Promise<void> {
  return invoke("fit_window", { height });
}

export function initialView(): Promise<ViewName> {
  return invoke<ViewName>("initial_view");
}
