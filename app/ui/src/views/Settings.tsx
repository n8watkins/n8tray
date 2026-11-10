// N8 Tray's settings, and each app's place in it. Every switch sends the
// change to the host, which saves it; the next snapshot shows the result.
import type { JSX } from "preact";
import type { HostSnapshot } from "../bindings/HostSnapshot";
import { hostCommand, openHost, openUpdate, setSetting } from "../api";
import { AppIcon } from "../components/AppIcon";
import { Glyph } from "../components/Glyph";

export function Toggle({
  icon,
  label,
  hint,
  on,
  disabled,
  onChange,
}: {
  icon?: JSX.Element;
  label: string;
  hint?: string;
  on: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label class="toggle">
      {icon}
      <span class="text">
        <span style={{ display: "block" }}>{label}</span>
        {hint && (
          <span class="line wrap" style={{ display: "block" }}>
            {hint}
          </span>
        )}
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={on}
        disabled={disabled}
        onChange={(e) => onChange((e.currentTarget as HTMLInputElement).checked)}
      />
    </label>
  );
}

export function Settings({ snapshot }: { snapshot: HostSnapshot }) {
  const s = snapshot.settings;
  const u = snapshot.update;
  return (
    <div class="flyout-inner">
      {s.error && (
        <div class="banner problem" role="alert">
          <span class="grow wrap">Settings file: {s.error}. Defaults are in use and nothing is saved until it is fixed.</span>
        </div>
      )}
      <section class="section" aria-labelledby="g-tray">
        <h2 id="g-tray">N8 Tray</h2>
        <Toggle
          label="Start at sign-in"
          hint={s.signInForeign ? "Another program made this sign-in entry; N8 Tray leaves it alone" : undefined}
          on={s.launchAtSignIn}
          disabled={s.signInForeign || s.error !== null}
          onChange={(on) => setSetting("launchAtSignIn", on)}
        />
        <Toggle
          label="Notifications"
          hint="Only for problems, once each"
          on={s.notifications}
          disabled={s.error !== null}
          onChange={(on) => setSetting("notifications", on)}
        />
        <Toggle
          label="Check for updates"
          hint={u.available ? `N8 Tray ${u.available} is available` : u.error ? `Last check: ${u.error}` : "At start and once a day; nothing installs without a click"}
          on={s.checkUpdates}
          disabled={s.error !== null}
          onChange={(on) => setSetting("checkUpdates", on)}
        />
        <Toggle
          label="Beta versions"
          hint="Pre-releases, for trying what comes next"
          on={s.updateChannel === "beta"}
          disabled={s.error !== null || !s.checkUpdates}
          onChange={(on) => setSetting("betaUpdates", on)}
        />
        {u.available && (
          <div class="actions-row">
            <button class="small-btn primary" onClick={() => openUpdate()}>
              Get N8 Tray {u.available}
            </button>
          </div>
        )}
      </section>

      {snapshot.apps.length > 0 && (
        <section class="section" aria-labelledby="g-apps">
          <h2 id="g-apps">Shown in N8 Tray</h2>
          {snapshot.apps.map((app) => (
            <Toggle
              key={app.id}
              icon={<AppIcon id={app.id} path={app.icon} small />}
              label={app.name}
              hint={app.hosted ? undefined : "Uses its own tray icon"}
              on={app.hosted}
              onChange={(on) => setSetting("hosted", on, app.id)}
            />
          ))}
          <p class="say" style={{ paddingTop: "6px" }}>Each app's panel has its other settings.</p>
        </section>
      )}

      <section class="section" aria-label="Files">
        <div class="opens">
          <button class="link" onClick={() => openHost("settings")}>
            <Glyph name="file" /> <span>Settings file</span>
          </button>
          <button class="link" onClick={() => hostCommand("reloadSettings")}>
            <Glyph name="restart" /> <span>Reload settings</span>
          </button>
          <button class="link" onClick={() => openHost("folder")}>
            <Glyph name="folder" /> <span>Tray folder</span>
          </button>
          <button class="link" onClick={() => openHost("log")}>
            <Glyph name="file" /> <span>Tray log</span>
          </button>
        </div>
      </section>
      <p class="say" style={{ margin: "4px 10px 0", padding: 0 }}>
        N8 Tray {snapshot.version}
      </p>
    </div>
  );
}
