// One action button. Impossible actions are dimmed (the host says which);
// an action whose manifest asks for confirmation takes a second click
// within 4 s. The host re-checks every action when it arrives.
import { useEffect, useState } from "preact/hooks";
import type { ActionView } from "../bindings/ActionView";
import type { AppView } from "../bindings/AppView";
import { moduleAction } from "../api";
import { Glyph, type GlyphName } from "./Glyph";

const glyphs: Record<string, GlyphName> = { start: "play", stop: "stop", restart: "restart" };

export function ActionButton({
  app,
  action,
  row,
  component,
  label,
  text,
}: {
  app: AppView;
  action: ActionView;
  row?: number;
  component?: string;
  label?: string;
  /** A text button (panel) instead of an icon button (row). */
  text?: boolean;
}) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(t);
  }, [armed]);
  const busy = app.pending !== null;
  const words = label ?? action.label;
  const name = armed ? `Confirm: ${words} ${component ? "" : app.name}`.trim() : `${words}${component ? "" : ` ${app.name}`}`;
  const run = () => {
    if (action.confirm && !armed) {
      setArmed(true);
      return;
    }
    setArmed(false);
    moduleAction(app.id, action.id, component);
  };
  const disabled = !action.enabled || busy;
  const title = busy ? app.pending! : !action.enabled ? `${words}: not possible now` : armed ? "Click again to confirm" : words;
  if (text) {
    return (
      <button class={`small-btn${armed ? " confirm" : ""}`} aria-label={name} title={title} disabled={disabled} onClick={run}>
        {armed ? "Confirm" : words}
      </button>
    );
  }
  return (
    <button
      class={`iconbtn act${armed ? " confirm" : ""}`}
      data-row={row}
      data-part="action"
      aria-label={name}
      title={title}
      disabled={disabled}
      onClick={run}
    >
      <Glyph name={glyphs[action.kind] ?? "play"} size={action.kind === "stop" ? 13 : 14} />
    </button>
  );
}
