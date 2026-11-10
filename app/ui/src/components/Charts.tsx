// The sparkline in a row and the chart in a panel. Screen readers get the
// numbers from the surrounding text, not the drawing.
import { points, runStart } from "../model";

export function Spark({ values }: { values: number[] }) {
  const w = 44;
  const h = 20;
  const from = runStart(values);
  if (values.length - from < 2) return <span class="spark" aria-hidden="true" />;
  const line = points(values, w, h, undefined, 2, from);
  const x0 = ((from * w) / (values.length - 1)).toFixed(1);
  return (
    <svg class="spark" viewBox={`0 0 ${w} ${h}`} aria-hidden="true" focusable="false">
      <polygon points={`${x0},${h} ${line} ${w},${h}`} fill="var(--spark-fill)" />
      <polyline points={line} fill="none" stroke="var(--spark)" stroke-width="1.3" stroke-linejoin="round" />
    </svg>
  );
}

/** Memory (filled) and CPU (dashed, against 100% of the machine scaled up). */
export function Chart({ memory, cpu, label }: { memory: number[]; cpu: number[]; label: string }) {
  const w = 336;
  const h = 64;
  const mem = points(memory, w, h, undefined, 4);
  const cpuMax = Math.max(10, ...cpu);
  const c = points(cpu, w, h, cpuMax, 4);
  return (
    <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label={label}>
      <polygon points={`0,${h} ${mem} ${w},${h}`} fill="var(--spark-fill)" />
      <polyline points={mem} fill="none" stroke="var(--spark)" stroke-width="1.5" vector-effect="non-scaling-stroke" />
      <polyline
        points={c}
        fill="none"
        stroke="var(--cpu)"
        stroke-width="1"
        stroke-dasharray="2 2"
        opacity="0.85"
        vector-effect="non-scaling-stroke"
      />
    </svg>
  );
}
