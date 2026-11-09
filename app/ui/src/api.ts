// The only ways the window talks to the host: the commands the Tauri
// capability allows, and the snapshot event. The window holds no state of
// its own beyond the latest snapshot.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { HostSnapshot } from "./bindings/HostSnapshot";

export function getSnapshot(): Promise<HostSnapshot | null> {
  return invoke<HostSnapshot | null>("get_snapshot");
}

export function onSnapshot(handler: (snapshot: HostSnapshot) => void): Promise<UnlistenFn> {
  return listen<HostSnapshot>("snapshot", (event) => handler(event.payload));
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
