// Status details: the overall state, every app's figures, N8 Tray's own
// cost, recent warnings, and Copy status.
import { useEffect, useState } from "preact/hooks";
import type { HostSnapshot } from "../bindings/HostSnapshot";
import type { WarningView } from "../bindings/WarningView";
import { openHost, recentWarnings, statusText } from "../api";
import { Glyph } from "../components/Glyph";
import { memory, percent } from "../format";
import { levelWords, since } from "../model";

export function Details({ snapshot }: { snapshot: HostSnapshot }) {
  const [warnings, setWarnings] = useState<WarningView[]>([]);
  const [copied, setCopied] = useState<string | null>(null);
  useEffect(() => {
    recentWarnings().then(setWarnings).catch(() => {});
  }, [snapshot]);
  const t = snapshot.tray;
  const m = snapshot.memory;
  const copy = async () => {
    const text = await statusText();
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied("Copied");
    } catch {
      setCopied("Could not copy");
    }
    setTimeout(() => setCopied(null), 2500);
  };
  return (
    <div class="flyout-inner">
      <section class="section" aria-labelledby="d-all">
        <h2 id="d-all">
          Overall
          <span class="h-links">
            <button onClick={copy}>{copied ?? "Copy status"}</button>
          </span>
        </h2>
        <dl class="kv">
          <dt>Status</dt>
          <dd>
            {levelWords(snapshot.level)}
            {snapshot.reason ? `: ${snapshot.reason}` : ""}
          </dd>
          <dt>Apps</dt>
          <dd>
            {snapshot.running} of {snapshot.total} running
          </dd>
          <dt>{m.system}</dt>
          <dd>
            {m.freeGb !== null && m.totalGb !== null ? `${m.freeGb.toFixed(1)} of ${m.totalGb.toFixed(1)} GB free` : "unknown"}
            {m.level !== "ok" && m.level !== "off" ? ` · ${levelWords(m.level).toLowerCase()}` : ""}
          </dd>
          <dt>Pressure sent</dt>
          <dd>{levelWords(snapshot.pressure)}</dd>
          <dt>N8 in all</dt>
          <dd>{memory(m.n8Mb)}</dd>
          <dt>This app</dt>
          <dd>
            {memory(t.rssMb)} in use, {t.privateMb !== null ? memory(t.privateMb) : "-"} private
            {t.cpuPercent !== null ? ` · ${percent(t.cpuPercent)} CPU` : ""}
          </dd>
          {t.windowMb > 0 && (
            <>
              <dt>This window</dt>
              <dd>
                {memory(t.windowMb)} in use, {t.windowPrivateMb !== null ? memory(t.windowPrivateMb) : "-"} private
              </dd>
            </>
          )}
        </dl>
      </section>

      {snapshot.apps.length > 0 && (
        <section class="section" aria-labelledby="d-apps">
          <h2 id="d-apps">Apps</h2>
          <table>
            <thead>
              <tr>
                <th>App</th>
                <th>State</th>
                <th class="num">Memory</th>
                <th class="num">CPU</th>
                <th>Since</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.apps.map((a) => {
                const main = a.processes.find((p) => p.verdict === "verified");
                const reported = (a.memoryMb ?? 0) === 0 && (a.namespacedMb ?? 0) > 0;
                return (
                  <tr key={a.id}>
                    <td>{a.name}</td>
                    <td>{a.state}</td>
                    <td class="num">{reported ? `${memory(a.namespacedMb)}*` : a.memoryMb !== null ? memory(a.memoryMb) : "-"}</td>
                    <td class="num">{percent(a.cpuPercent)}</td>
                    <td>{a.running ? since(main?.startedAt) : "-"}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {snapshot.apps.some((a) => (a.memoryMb ?? 0) === 0 && (a.namespacedMb ?? 0) > 0) && (
            <p class="say" style={{ paddingTop: "4px", paddingBottom: "8px", margin: 0 }}>* reported by the app, inside WSL</p>
          )}
        </section>
      )}

      {snapshot.problems.length > 0 && (
        <section class="section" aria-labelledby="d-prob">
          <h2 id="d-prob">Problems</h2>
          <ul class="warnings">
            {snapshot.problems.map((p) => (
              <li key={p.file}>
                <span />
                <span class="error wrap">
                  {p.id ?? p.file}: {p.message}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section class="section" aria-labelledby="d-warn">
        <h2 id="d-warn">Recent warnings</h2>
        {warnings.length === 0 ? (
          <p class="say">None since N8 Tray started.</p>
        ) : (
          <ul class="warnings">
            {warnings.map((w, i) => (
              <li key={i}>
                <time>{w.at}</time>
                <span class={`wrap${w.level === "error" ? " error" : ""}`}>
                  {w.message}
                  {w.count > 1 ? ` (${w.count} times)` : ""}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section class="section" aria-label="Files">
        <div class="opens">
          <button class="link" onClick={() => openHost("log")}>
            <Glyph name="file" /> <span>Open tray log</span>
          </button>
          <button class="link" onClick={() => openHost("folder")}>
            <Glyph name="folder" /> <span>Open tray folder</span>
          </button>
        </div>
      </section>
    </div>
  );
}
