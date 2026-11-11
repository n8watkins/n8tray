// An available update: what it is, its progress while it installs, and the
// one button that installs it (or tries again). "What's new" opens its
// release page, which after a failure is the way to get it by hand. The host and the updater decide everything; this words it.
import type { UpdateView } from "../bindings/UpdateView";
import { installUpdate, openUpdate } from "../api";
import { updateLine } from "../model";
import { Glyph } from "./Glyph";

export function UpdateBanner({ update }: { update: UpdateView }) {
  const line = updateLine(update);
  if (!line) return null;
  return (
    <div class={`banner update ${line.problem ? "problem" : "info"}`} role={line.problem ? "alert" : "status"}>
      <Glyph name="update" />
      <span class="grow wrap">
        <span style={{ display: "block" }}>{line.text}</span>
        {line.detail && (
          <span class="line wrap" style={{ display: "block" }}>
            {line.detail}
          </span>
        )}
        {line.progress !== null && (
          <span
            class="bar"
            style={{ display: "block" }}
            role="progressbar"
            aria-label="Update download"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(line.progress * 100)}
          >
            <i style={{ width: `${Math.round(line.progress * 100)}%` }} />
          </span>
        )}
      </span>
      <span class="banner-actions">
        {update.url && (
          <button class="small-btn" onClick={() => openUpdate()}>
            {line.problem ? "Release page" : "What's new"}
          </button>
        )}
        {line.action && (
          <button class="small-btn primary" onClick={() => installUpdate()}>
            {line.action === "retry" ? "Try again" : "Install"}
          </button>
        )}
      </span>
    </div>
  );
}
