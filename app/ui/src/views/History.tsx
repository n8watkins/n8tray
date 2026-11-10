// Resource history: every app's memory and CPU over 24 hours or 7 days,
// from the host's day files.
import { useEffect, useState } from "preact/hooks";
import type { HistoryView } from "../bindings/HistoryView";
import type { HostSnapshot } from "../bindings/HostSnapshot";
import { history, type HistoryRange } from "../api";
import { AppIcon } from "../components/AppIcon";
import { Chart } from "../components/Charts";
import { memory, percent } from "../format";
import { hasHistory } from "../model";

export function History({ snapshot, only }: { snapshot: HostSnapshot; only?: string }) {
  const [range, setRange] = useState<HistoryRange>("day");
  const apps = [
    ...snapshot.apps.filter((a) => a.hosted && (!only || a.id === only)).map((a) => ({ id: a.id, name: a.name, icon: a.icon })),
    ...(!only || only === "n8tray" ? [{ id: "n8tray", name: "N8 Tray", icon: null }] : []),
  ];
  return (
    <div class="flyout-inner">
      <div class="actions-row" role="group" aria-label="Range" style={{ paddingTop: "2px" }}>
        {(["day", "week"] as const).map((r) => (
          <button key={r} class={`small-btn${range === r ? " primary" : ""}`} aria-pressed={range === r} onClick={() => setRange(r)}>
            {r === "day" ? "24 hours" : "7 days"}
          </button>
        ))}
      </div>
      {apps.map((a) => (
        <AppHistory key={`${a.id}-${range}`} id={a.id} name={a.name} icon={a.icon} range={range} />
      ))}
    </div>
  );
}

function AppHistory({ id, name, icon, range }: { id: string; name: string; icon: string | null; range: HistoryRange }) {
  const [data, setData] = useState<HistoryView | null>(null);
  useEffect(() => {
    history(id, range).then(setData).catch(() => setData(null));
  }, [id, range]);
  const span = range === "day" ? "24 hours" : "7 days";
  const memMax = data ? Math.max(0, ...data.memoryMb) : 0;
  const cpuMax = data ? Math.max(0, ...data.cpuPercent) : 0;
  return (
    <section class="section" aria-label={`${name}, last ${span}`}>
      <h2 style={{ textTransform: "none", letterSpacing: 0, fontSize: "12px", color: "var(--fg)", justifyContent: "flex-start", gap: "8px", alignItems: "center" }}>
        <AppIcon id={id} path={icon} small />
        {name}
      </h2>
      <div class="chart">
        {data && hasHistory(data.memoryMb) ? (
          <>
            <Chart
              memory={data.memoryMb}
              cpu={data.cpuPercent}
              label={`${name} over the last ${span}: memory at most ${memory(memMax)}, CPU at most ${percent(cpuMax)}`}
            />
            <div class="legend">
              <span>Memory, max {memory(memMax)}</span>
              <span class="cpu-key">CPU, max {percent(cpuMax)}</span>
            </div>
          </>
        ) : (
          <p class="say" style={{ padding: "2px 0 0" }}>{data ? `Nothing recorded in the last ${span}.` : "Reading"}</p>
        )}
      </div>
    </section>
  );
}
