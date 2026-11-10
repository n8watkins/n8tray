// One app's panel: its state, components, actions, the last 15 minutes,
// its processes, what it lets you open, and its settings.
import type { AppView } from "../bindings/AppView";
import { openItem, setSetting } from "../api";
import { ActionButton } from "../components/ActionButton";
import { AppIcon } from "../components/AppIcon";
import { Chart } from "../components/Charts";
import { Glyph, type GlyphName } from "../components/Glyph";
import { cpu, memory, percent } from "../format";
import { appActions, componentActions, hasHistory, restartAction, rowAction, rowLine } from "../model";
import { CloseButton } from "./Hub";
import { Toggle } from "./Settings";
import type { Go } from "../main";

export function PanelHead({ app, back }: { app: AppView; back: () => void }) {
  const main = rowAction(app);
  const restart = restartAction(app);
  return (
    <header class="head crumb">
      <button class="iconbtn" aria-label="Back" title="Back" onClick={back}>
        <Glyph name="back" />
      </button>
      <AppIcon id={app.id} path={app.icon} level={app.level} small />
      <h1 class="title panel-title" id="title">
        {app.name}
      </h1>
      <span class={`pill ${app.level}`}>{app.pending ?? app.words}</span>
      <span class="grow" />
      {main && <ActionButton app={app} action={main} />}
      {restart && <ActionButton app={app} action={restart} />}
      <CloseButton />
    </header>
  );
}

const openGlyph: Record<string, GlyphName> = { settings: "gear", logs: "file", folder: "folder", readme: "file" };

export function Panel({ app, go }: { app: AppView; go: Go }) {
  const line = rowLine(app);
  const others = appActions(app);
  const history = app.history;
  const memMax = history ? Math.max(...history.memoryMb) : 0;
  const cpuMax = history ? Math.max(...history.cpuPercent) : 0;
  const figures = [memory(app.memoryMb), cpu(app.cpuPercent)].filter(Boolean).join(" · ");
  return (
    <div class="flyout-inner">
      <section class="section" aria-labelledby="s-state">
        <h2 id="s-state">State</h2>
        <div class="say">
          <p class={`line wrap ${line.tone}`}>{line.text}</p>
          {app.detail && app.detail !== line.text && <p class="wrap">{app.detail}</p>}
          {figures && <p>{figures}{app.namespacedMb ? ` · ${memory(app.namespacedMb)} reported inside WSL` : ""}</p>}
          {app.gpu && (app.gpu.memMb !== null || app.gpu.percent !== null) && (
            <p>
              GPU {memory(app.gpu.memMb)}
              {app.gpu.percent !== null ? ` · ${percent(app.gpu.percent)}` : ""} ({app.gpu.source})
            </p>
          )}
          {app.description && <p class="wrap">{app.description}</p>}
          {app.notes.map((n) => (
            <p class="wrap line attention" key={n}>
              {n}
            </p>
          ))}
        </div>
        {others.length > 0 && (
          <div class="actions-row">
            {others.map((a) => (
              <ActionButton key={a.id} app={app} action={a} text />
            ))}
          </div>
        )}
      </section>

      {app.components.length > 0 && (
        <section class="section" aria-labelledby="s-comp">
          <h2 id="s-comp">Components</h2>
          {app.components.map((c) => (
            <div class="comp" key={c.id}>
              <div class="text">
                <div class="name">{c.name}</div>
                <div class={`line wrap ${c.level === "problem" ? "problem" : c.level === "attention" ? "attention" : ""}`}>
                  {c.detail ?? c.words}
                </div>
              </div>
              <div class="fig">{c.memoryMb !== null ? <b>{memory(c.memoryMb)}</b> : null}</div>
              <div class="btns">
                {componentActions(app, c).map(({ action, label }) => (
                  <ActionButton key={action.id} app={app} action={action} component={c.id} label={label} text />
                ))}
              </div>
            </div>
          ))}
        </section>
      )}

      {history && hasHistory(history.memoryMb) && (
        <section class="section" aria-labelledby="s-hist">
          <h2 id="s-hist">
            Last 15 minutes
            <span class="h-links">
              <button onClick={() => go({ name: "history", id: app.id })}>24 hours, 7 days</button>
            </span>
          </h2>
          <div class="chart">
            <Chart
              memory={history.memoryMb}
              cpu={history.cpuPercent}
              label={`Memory over the last 15 minutes, at most ${memory(memMax)}; CPU at most ${percent(cpuMax)}`}
            />
            <div class="legend">
              <span>Memory, max {memory(memMax)}</span>
              <span class="cpu-key">CPU, dashed, max {percent(cpuMax)}</span>
            </div>
          </div>
        </section>
      )}

      {app.processes.length > 0 && (
        <section class="section" aria-labelledby="s-proc">
          <h2 id="s-proc">Processes</h2>
          <table>
            <thead>
              <tr>
                <th>Process</th>
                <th class="num">Memory</th>
                <th class="num">Private</th>
                <th class="num">CPU</th>
                <th>
                  <span class="sr-only">Verdict</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {app.processes.map((p) => (
                <tr key={`${p.pid}-${p.startedAt}`} title={p.note ?? undefined}>
                  <td>
                    {p.name} <small>{[p.pid, p.component ?? p.role].filter(Boolean).join(" · ")}</small>
                  </td>
                  <td class="num">{p.rssMb !== null ? memory(p.rssMb) : "-"}</td>
                  <td class="num">{p.privateMb !== null ? memory(p.privateMb) : "-"}</td>
                  <td class="num">{percent(p.cpuPercent)}</td>
                  <td>
                    <span class={`tag ${p.verdict}`}>{p.verdict}</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div class="table-end" />
        </section>
      )}

      {(app.open.length > 0 || app.menu.length > 0) && (
        <section class="section" aria-label="Open">
          <div class="opens">
            {app.open.map((o) => (
              <button
                class="link"
                key={o.key}
                disabled={!o.exists}
                title={o.exists ? o.target : `${o.label}: not there yet`}
                onClick={() => openItem(app.id, "open", o.key)}
              >
                <Glyph name={openGlyph[o.key] ?? "file"} /> <span>{o.label}</span>
              </button>
            ))}
            {app.menu.map((m, i) => (
              <button
                class="link"
                key={`m${i}`}
                disabled={!m.exists}
                onClick={() => openItem(app.id, "menu", String(i))}
              >
                <Glyph name={m.kind === "open-url" ? "web" : m.kind === "action" ? "play" : "folder"} /> <span>{m.label}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      <section class="section" aria-labelledby="s-set">
        <h2 id="s-set">In N8 Tray</h2>
        <Toggle
          label="Start with N8 Tray"
          hint="When N8 Tray starts, or when the app is installed"
          on={app.autoStart}
          onChange={(on) => setSetting("autoStart", on, app.id)}
        />
        <Toggle
          label="Show in N8 Tray"
          hint="Off: the app uses its own tray icon"
          on={app.hosted}
          onChange={(on) => setSetting("hosted", on, app.id)}
        />
        <Toggle
          label="Turned on"
          hint="Off: N8 Tray never starts it"
          on={app.enabled}
          onChange={(on) => setSetting("enabled", on, app.id)}
        />
      </section>
      <p class="say" style={{ margin: "4px 10px 0", padding: 0 }}>
        {app.name} {app.version}
      </p>
    </div>
  );
}
