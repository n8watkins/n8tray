import { render } from "preact";
import { useEffect, useState } from "preact/hooks";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { AppView } from "./bindings/AppView";
import { getSnapshot, hideWindow, moduleAction, onSnapshot, openItem } from "./api";
import { cpu, memory } from "./format";
import "./style.css";

// A plain status view of the host's snapshot. The hub (Phase 4) replaces it.
function App() {
  const [snapshot, setSnapshot] = useState<HostSnapshot | null>(null);

  useEffect(() => {
    let stop: (() => void) | undefined;
    getSnapshot().then((s) => s && setSnapshot(s));
    onSnapshot(setSnapshot).then((unlisten) => (stop = unlisten));
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hideWindow();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      stop?.();
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  if (!snapshot) return <main class="empty">Starting</main>;
  const apps = snapshot.apps.filter((a) => a.hosted && a.level !== "hidden");
  return (
    <main>
      <header class={`status ${snapshot.level}`}>
        <span class="dot" aria-hidden="true" />
        <div>
          <h1>{snapshot.headline}</h1>
          <p>{snapshot.reason ?? `${snapshot.running} of ${snapshot.total} running`}</p>
          <p class="muted">{snapshot.memory.text}</p>
        </div>
        <button class="close" aria-label="Hide" onClick={() => hideWindow()}>
          x
        </button>
      </header>
      <ul class="apps" aria-label="Apps">
        {apps.map((app) => (
          <Row key={app.id} app={app} />
        ))}
      </ul>
      {snapshot.problems.map((p) => (
        <p class="problem" key={p.file}>
          {p.message}
        </p>
      ))}
      <footer class="muted">
        N8 Tray {snapshot.version} · this app {memory(snapshot.tray.rssMb)}
        {snapshot.tray.cpuPercent !== null ? ` · ${cpu(snapshot.tray.cpuPercent)}` : ""}
        {snapshot.tray.windowMb > 0 ? ` · window ${memory(snapshot.tray.windowMb)}` : ""}
      </footer>
    </main>
  );
}

function Row({ app }: { app: AppView }) {
  const figures = [memory(app.memoryMb), cpu(app.cpuPercent)].filter(Boolean).join(" · ");
  // Start while stopped; Stop and Restart while running.
  const buttons = app.actions.filter((a) =>
    a.kind === "start" ? !app.running : a.kind === "stop" || a.kind === "restart" ? app.running : false,
  );
  return (
    <li class={`app ${app.level}`}>
      <span class="dot" aria-hidden="true" />
      <div class="text">
        <strong>{app.name}</strong>
        <span>
          {app.pending ?? app.summary}
          {figures ? ` · ${figures}` : ""}
        </span>
        {app.alert && <span class="outcome">{app.alert}</span>}
        {app.lastOutcome && app.lastOutcome.outcome !== "ok" && (
          <span class="outcome">{app.lastOutcome.message ?? app.lastOutcome.outcome}</span>
        )}
      </div>
      <div class="buttons">
        {buttons.map((a) => (
          <button key={a.id} disabled={!a.enabled} onClick={() => moduleAction(app.id, a.id)}>
            {a.label}
          </button>
        ))}
        {app.open
          .filter((o) => o.key === "logs")
          .map((o) => (
            <button key={o.key} disabled={!o.exists} onClick={() => openItem(app.id, "open", o.key)}>
              Log
            </button>
          ))}
      </div>
    </li>
  );
}

render(<App />, document.getElementById("app")!);
