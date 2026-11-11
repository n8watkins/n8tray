// The hub: status card, one row per app with its one action, and N8 Tray
// itself. Every figure is the host's; this only lays them out.
import type { AppView } from "../bindings/AppView";
import type { HostSnapshot } from "../bindings/HostSnapshot";
import { useState } from "preact/hooks";
import { hideWindow, openHost, setSetting } from "../api";
import { UpdateBanner } from "../components/UpdateBanner";
import { AppIcon, GenericIcon } from "../components/AppIcon";
import { Spark } from "../components/Charts";
import { Glyph } from "../components/Glyph";
import { ActionButton } from "../components/ActionButton";
import { memory } from "../format";
import {
  hasHistory,
  hubApps,
  levelWords,
  memoryMeter,
  rowAction,
  rowFigures,
  rowLine,
  sections,
  statusCard,
} from "../model";
import type { Go } from "../main";

export function HubHead({ snapshot, go }: { snapshot: HostSnapshot; go: Go }) {
  return (
    <header class="head">
      <AppIcon id="n8tray" path={null} small />
      <h1 class="title" id="title">
        N8 Tray
      </h1>
      <span class={`pill ${snapshot.level}`}>{levelWords(snapshot.level)}</span>
      <span class="grow" />
      <button class="iconbtn" aria-label="Settings" title="Settings" onClick={() => go({ name: "settings" })}>
        <Glyph name="gear" />
      </button>
      <CloseButton />
    </header>
  );
}

export function CloseButton() {
  return (
    <button class="iconbtn" aria-label="Hide N8 Tray" title="Hide" onClick={() => hideWindow()}>
      <Glyph name="close" size={12} />
    </button>
  );
}

export function Hub({ snapshot, go }: { snapshot: HostSnapshot; go: Go }) {
  const apps = hubApps(snapshot);
  const card = statusCard(snapshot);
  const meter = memoryMeter(snapshot);
  let row = 0;
  return (
    <div class="flyout-inner">
      {snapshot.legacyTray && <WatchNote />}
      <UpdateBanner update={snapshot.update} />
      <button class="status" onClick={() => go({ name: "details" })}>
        <span>
          <span class="big" style={{ display: "block" }}>
            {card.big}
          </span>
          <span class="small" style={{ display: "block" }}>
            {card.small}
          </span>
        </span>
        <span class="more">Details</span>
        {meter && (
          <span class="meters" aria-hidden="true">
            <span class="meter">
              <span class="label">
                <span>{meter.label}</span>
                <span>{meter.text}</span>
              </span>
              <span class={`bar ${meter.level}`} style={{ display: "block" }}>
                <i style={{ width: `${Math.round(meter.used * 100)}%` }} />
              </span>
            </span>
          </span>
        )}
      </button>
      <div class="list" role="group" aria-label="Apps">
        {apps.length === 0 && <p class="empty">No apps yet. An app appears here once it installs its manifest.</p>}
        {sections(apps).map((g) => (
          <div key={g.name ?? "-"}>
            {g.name && (
              <div class="group" aria-hidden="true">
                {g.name}
              </div>
            )}
            {g.apps.map((app) => (
              <Row key={app.id} app={app} index={row++} go={go} watching={snapshot.legacyTray} />
            ))}
          </div>
        ))}
        {snapshot.problems.map((p) => (
          <div class="app" key={p.file}>
            <div class="app-main static" role="note">
              <span class="appicon" aria-hidden="true">
                <GenericIcon />
                <span class="dot problem" />
              </span>
              <span class="text">
                <span class="name" style={{ display: "block" }}>
                  {p.id ?? p.file}
                </span>
                <span class="line problem wrap" style={{ display: "block" }}>
                  {p.message}
                </span>
              </span>
              <span />
            </div>
          </div>
        ))}
        <div class="divider" aria-hidden="true" />
        <TrayRow snapshot={snapshot} index={row++} go={go} />
      </div>
    </div>
  );
}

/**
 * The one line shown while the PowerShell N8 Tray runs: who manages the
 * apps, and that nothing here starts or stops them. "Learn more" opens the
 * why in place.
 */
function WatchNote() {
  const [open, setOpen] = useState(false);
  return (
    <div class="banner info watch" role="note">
      <Glyph name="info" />
      <span class="grow">
        Your PowerShell N8 Tray is managing these apps. This app is watching only; nothing here starts or stops apps.{" "}
        <button class="inline-link" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? "Show less" : "Learn more"}
        </button>
        {open && (
          <span class="more-text">
            Both trays read the same apps. Until this one replaces the PowerShell tray, it only shows their state and memory, so the two never fight over
            an app. Exit the PowerShell N8 Tray and this one manages them.
          </span>
        )}
      </span>
    </div>
  );
}

function Row({ app, index, go, watching }: { app: AppView; index: number; go: Go; watching: boolean }) {
  // While watching, nothing here starts or stops apps: no dimmed buttons.
  const action = watching ? null : rowAction(app);
  const line = rowLine(app);
  const figures = rowFigures(app);
  const history = app.history?.memoryMb;
  return (
    <div class="app">
      <button
        class="app-main"
        data-row={index}
        data-part="main"
        title={app.detail ?? undefined}
        onClick={() => go({ name: "panel", id: app.id })}
      >
        <AppIcon id={app.id} path={app.icon} level={app.level} />
        <span class="text">
          <span class="name" style={{ display: "block" }}>
            {app.name}
            {(app.pending ?? app.words) !== line.text && <span class="sr-only">, {app.pending ?? app.words},</span>}
          </span>
          <span class={`line ${line.tone}`} style={{ display: "block" }}>
            {line.text}
          </span>
        </span>
        <span class="right">
          {app.running && hasHistory(history) && <Spark values={history!} />}
          {figures && (
            <span class="fig">
              <b>{figures.memory}</b>
              {figures.sub && (
                <>
                  <br />
                  {figures.sub}
                </>
              )}
            </span>
          )}
        </span>
      </button>
      {action ? (
        <ActionButton app={app} action={action} row={index} />
      ) : (
        <span class="act" style={{ width: "26px" }} />
      )}
    </div>
  );
}

function TrayRow({ snapshot, index, go }: { snapshot: HostSnapshot; index: number; go: Go }) {
  const t = snapshot.tray;
  return (
    <div class="app">
      <button class="app-main" data-row={index} data-part="main" onClick={() => go({ name: "details" })}>
        <AppIcon id="n8tray" path={null} level="ok" />
        <span class="text">
          <span class="name" style={{ display: "block" }}>
            N8 Tray
          </span>
          <span class="line" style={{ display: "block" }}>
            This app
          </span>
        </span>
        <span class="right">
          {hasHistory(t.history?.memoryMb) && <Spark values={t.history!.memoryMb} />}
          <span class="fig">
            <b>{memory(t.privateMb ?? t.rssMb)}</b>
            {t.windowMb > 0 && (
              <>
                <br />
                window {memory(t.windowPrivateMb ?? t.windowMb)}
              </>
            )}
          </span>
        </span>
      </button>
      <span class="act" style={{ width: "26px" }} />
    </div>
  );
}

export function HubFoot({ snapshot, go }: { snapshot: HostSnapshot; go: Go }) {
  const s = snapshot.settings;
  const locked = s.signInForeign || s.error !== null;
  return (
    <footer class="foot">
      <button
        class="link"
        role="switch"
        aria-checked={s.launchAtSignIn}
        disabled={locked}
        title={s.signInForeign ? "Another program made the sign-in entry; N8 Tray leaves it alone" : undefined}
        onClick={() => setSetting("launchAtSignIn", !s.launchAtSignIn)}
      >
        <Glyph name={s.launchAtSignIn ? "check" : "box"} /> Start at sign-in
      </button>
      <span class="grow" />
      <button class="link" onClick={() => go({ name: "history" })}>
        <Glyph name="history" /> History
      </button>
      <button class="link" onClick={() => openHost("log")}>
        <Glyph name="file" /> Log
      </button>
    </footer>
  );
}
