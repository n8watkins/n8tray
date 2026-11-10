// N8 Tray's window: a view of the host's snapshot. It keeps only the
// latest snapshot and which view it shows; every decision is Rust's.
import { render } from "preact";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import { fitWindow, getSnapshot, hideWindow, initialView, onFit, onNavigate, onSnapshot, type ViewName } from "./api";
import { moveFocus } from "./keyboard";
import { Details } from "./views/Details";
import { History } from "./views/History";
import { CloseButton, Hub, HubFoot, HubHead } from "./views/Hub";
import { Panel, PanelHead } from "./views/Panel";
import { Settings } from "./views/Settings";
import { Glyph } from "./components/Glyph";
import "./style.css";

export type View = { name: ViewName } | { name: "panel"; id: string } | { name: "history"; id?: string };
export type Go = (view: View) => void;

const titles: Record<string, string> = { details: "Status details", settings: "Settings", history: "Resource history" };

function App() {
  const [snapshot, setSnapshot] = useState<HostSnapshot | null>(null);
  const [view, setView] = useState<View>({ name: "hub" });
  const [clipped, setClipped] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);

  const go: Go = useCallback((next: View) => {
    setView(next);
    // Keyboard and screen reader users land at the top of the new view.
    requestAnimationFrame(() => root.current?.focus());
  }, []);

  useEffect(() => {
    const stops: (() => void)[] = [];
    // The host may not have published yet when a new window asks: ask again
    // until it has (events then carry every change).
    let first = true;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const ask = () =>
      getSnapshot()
        .then((s) => {
          if (!first) return;
          if (s) {
            first = false;
            setSnapshot(s);
          } else retry = setTimeout(ask, 300);
        })
        .catch(() => first && (retry = setTimeout(ask, 1000)));
    ask();
    stops.push(() => {
      first = false;
      clearTimeout(retry);
    });
    initialView().then((v) => setView({ name: v }));
    onSnapshot((s) => {
      first = false;
      setSnapshot(s);
    }).then((u) => stops.push(u));
    onNavigate((v) => go({ name: v })).then((u) => stops.push(u));
    onFit(setClipped).then((u) => stops.push(u));
    return () => stops.forEach((s) => s());
  }, [go]);

  // The window's height follows the content; Rust sizes, places and (the
  // first time) shows it once it has drawn, so it appears in one piece.
  useLayoutEffect(() => {
    const el = root.current;
    const body = inner.current;
    if (!el || !body || !snapshot) return;
    let frame = 0;
    let last = -1;
    const report = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const head = el.querySelector<HTMLElement>(".head")?.offsetHeight ?? 0;
        const foot = el.querySelector<HTMLElement>(".foot")?.offsetHeight ?? 0;
        const height = Math.ceil(head + body.scrollHeight + foot);
        if (height !== last) {
          last = height;
          fitWindow(height);
        }
      });
    };
    const observer = new ResizeObserver(report);
    observer.observe(body);
    report();
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [snapshot !== null, view]);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (view.name === "hub") hideWindow();
      else go({ name: "hub" });
      return;
    }
    if (root.current && moveFocus(e, root.current)) e.preventDefault();
  };

  if (!snapshot) {
    return (
      <div class="flyout" ref={root} tabIndex={-1} role="dialog" aria-label="N8 Tray">
        <div class="flyout-body">
          <div ref={inner}>
            <p class="empty">Starting</p>
          </div>
        </div>
      </div>
    );
  }

  const app = view.name === "panel" ? snapshot.apps.find((a) => a.id === view.id) : undefined;
  const back = () => go({ name: "hub" });
  let head;
  let body;
  if (view.name === "panel" && app) {
    head = <PanelHead app={app} back={back} />;
    body = <Panel app={app} go={go} />;
  } else if (view.name === "hub" || view.name === "panel") {
    head = <HubHead snapshot={snapshot} go={go} />;
    body = <Hub snapshot={snapshot} go={go} />;
  } else {
    head = (
      <header class="head crumb">
        <button class="iconbtn" aria-label="Back" title="Back" onClick={back}>
          <Glyph name="back" />
        </button>
        <h1 class="title" id="title">
          {titles[view.name]}
        </h1>
        <span class="grow" />
        <CloseButton />
      </header>
    );
    body =
      view.name === "details" ? (
        <Details snapshot={snapshot} />
      ) : view.name === "settings" ? (
        <Settings snapshot={snapshot} />
      ) : (
        <History snapshot={snapshot} only={"id" in view ? view.id : undefined} />
      );
  }
  const isHub = view.name === "hub" || (view.name === "panel" && !app);
  return (
    <div class="flyout" ref={root} tabIndex={-1} role="dialog" aria-labelledby="title" onKeyDown={onKey}>
      {head}
      <div class={`flyout-body${clipped ? " clipped" : ""}`}>
        <div ref={inner}>{body}</div>
      </div>
      {isHub && <HubFoot snapshot={snapshot} go={go} />}
    </div>
  );
}

render(<App />, document.getElementById("app")!);
