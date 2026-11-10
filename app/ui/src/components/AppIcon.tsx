// An app's own icon (spec 3.11), read by Rust and shown only as an image,
// with the app's level as a dot on its corner. Without a usable icon: the
// generic app glyph.
import { useEffect, useState } from "preact/hooks";
import type { LevelName } from "../bindings/LevelName";
import { appIcon } from "../api";
import n8tray from "../assets/n8tray.svg";

// One read per app and icon path while the window lives.
const cache = new Map<string, Promise<string | null>>();

function load(id: string, path: string): Promise<string | null> {
  const key = `${id}\n${path}`;
  let hit = cache.get(key);
  if (!hit) {
    hit = appIcon(id).catch(() => null);
    cache.set(key, hit);
  }
  return hit;
}

export function GenericIcon() {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" focusable="false">
      <rect width="32" height="32" rx="7" fill="var(--off-bg)" />
      <g fill="none" stroke="var(--muted)" stroke-width="1.8">
        <rect x="8" y="8" width="7" height="7" rx="1.6" />
        <rect x="17" y="8" width="7" height="7" rx="1.6" />
        <rect x="8" y="17" width="7" height="7" rx="1.6" />
        <rect x="17" y="17" width="7" height="7" rx="1.6" />
      </g>
    </svg>
  );
}

export function AppIcon({
  id,
  path,
  level,
  small,
}: {
  id: string;
  path: string | null;
  level?: LevelName;
  small?: boolean;
}) {
  const [src, setSrc] = useState<string | null>(id === "n8tray" ? n8tray : null);
  useEffect(() => {
    if (id === "n8tray") return;
    let live = true;
    if (!path) {
      setSrc(null);
      return;
    }
    load(id, path).then((url) => live && setSrc(url));
    return () => {
      live = false;
    };
  }, [id, path]);
  return (
    <span class={`appicon${small ? " small" : ""}`} aria-hidden="true">
      {src ? <img src={src} alt="" draggable={false} onError={() => setSrc(null)} /> : <GenericIcon />}
      {level && <span class={`dot ${level}`} />}
    </span>
  );
}
