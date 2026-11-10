// The real window in the real app, through tauri-driver (WebDriver): N8 Tray
// starts in its own folders with two fake apps, opens its window, shows a
// row per app with the app's own icon, starts and stops an app from its row,
// opens a panel and comes back with Escape. Windows (msedgedriver) and Linux
// (WebKitWebDriver); tauri-driver has no macOS support.
//
//   node tests/e2e/webdriver.mjs --app <n8tray> --fake <n8tray-fake-module>
//        [--driver tauri-driver] [--native-driver <msedgedriver>] [--out <dir>]
//
// No dependencies: WebDriver is HTTP and JSON.
import { spawn } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const opts = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []),
);
const app = resolve(opts.app);
const fake = resolve(opts.fake);
const out = resolve(opts.out ?? join(here, "../../screens"));
const DRIVER = "http://127.0.0.1:4444";
const ELEMENT = "element-6066-11e4-a52f-4a5d6dc9c3b2";
// W3C drivers key an element by ELEMENT; older ones by "ELEMENT".
const idOf = (e) => {
  const id = e?.[ELEMENT] ?? e?.ELEMENT ?? Object.values(e ?? {})[0];
  if (typeof id !== "string") throw new Error(`no element id in ${JSON.stringify(e)}`);
  return id;
};

function freePort() {
  return new Promise((ok) => {
    const s = createServer().listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => ok(port));
    });
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function until(what, fn, ms = 30000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    try {
      const v = await fn();
      if (v) return v;
    } catch (e) {
      last = e;
    }
    await sleep(200);
  }
  throw new Error(`Timed out waiting for ${what}${last ? `: ${last.message}` : ""}`);
}

async function wd(method, path, body) {
  const res = await fetch(`${DRIVER}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  const json = await res.json();
  if (!res.ok || json.value?.error) throw new Error(`${method} ${path}: ${json.value?.message ?? res.status}`);
  return json.value;
}

// The fake apps live in their own folder with an icon, like installed apps.
const home = mkdtempSync(join(tmpdir(), "n8tray-e2e-"));
const install = join(home, "apps", "fake");
mkdirSync(install, { recursive: true });
mkdirSync(join(home, "modules"), { recursive: true });
const exe = join(install, basename(fake));
copyFileSync(fake, exe);
copyFileSync(join(here, "../../../../spec/examples/icons/voicedock.svg"), join(install, "one.svg"));
const port = await freePort();
const manifest = (id, name, status, args, extra = {}) => ({
  contract: 2,
  id,
  name,
  version: "1.0.0",
  group: "Test",
  installDir: install,
  status,
  run: { program: basename(fake), args: [...args, "--id", id, "--exit-after-seconds", "240"], stop: { type: status.type === "http" ? "http" : "terminate", graceSeconds: 5 } },
  ...extra,
});
writeFileSync(
  join(home, "modules", "fakeone.json"),
  JSON.stringify(
    manifest("fakeone", "Fake one", { type: "http", url: `http://127.0.0.1:${port}/n8/v2/status`, intervalSeconds: 1, timeoutMs: 1000 }, ["--status", "http", "--port", String(port), "--interval-seconds", "1"], {
      icon: "one.svg",
      autoStartDefault: true,
    }),
  ),
);
writeFileSync(
  join(home, "modules", "faketwo.json"),
  JSON.stringify(manifest("faketwo", "Fake two", { type: "file", intervalSeconds: 1 }, ["--status", "file", "--interval-seconds", "1"])),
);

const driverArgs = opts["native-driver"] ? ["--native-driver", opts["native-driver"]] : [];
const driver = spawn(opts.driver ?? "tauri-driver", driverArgs, {
  env: { ...process.env, N8TRAY_HOME: home },
  stdio: ["ignore", "inherit", "inherit"],
});
let session;
let failed = false;

const css = async (selector) => (await wd("POST", `/session/${session}/elements`, { using: "css selector", value: selector })).map(idOf);
const click = async (id) => wd("POST", `/session/${session}/element/${id}/click`, {});
const keys = async (value) => wd("POST", `/session/${session}/actions`, { actions: [{ type: "key", id: "k", actions: [{ type: "keyDown", value }, { type: "keyUp", value }] }] });
const shot = async (name) => {
  const png = await wd("GET", `/session/${session}/screenshot`);
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, `webdriver-${process.platform}-${name}.png`), Buffer.from(png, "base64"));
};
// Found in the page itself, so every driver matches text the same way.
async function find(script, ...args) {
  const found = await wd("POST", `/session/${session}/execute/sync`, { script, args });
  return found ? idOf(found) : null;
}
const asElement = (id) => ({ [ELEMENT]: id, ELEMENT: id });
const js = (script, ...args) => wd("POST", `/session/${session}/execute/sync`, { script, args });
const heading = () => js("return document.querySelector('h1')?.textContent.trim() ?? null;");
async function rowFor(name) {
  return find("return [...document.querySelectorAll('.app')].find((e) => e.textContent.includes(arguments[0])) || null;", name);
}
async function buttonIn(row, label) {
  return find("return [...arguments[0].querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === arguments[1]) || null;", asElement(row), label);
}

async function step(name, fn) {
  const started = Date.now();
  try {
    await fn();
    console.log(`ok - ${name} (${Date.now() - started} ms)`);
  } catch (e) {
    failed = true;
    console.log(`not ok - ${name}: ${e.message}`);
    await shot(`failed-${name.replace(/\W+/g, "-")}`).catch(() => {});
    throw e;
  }
}

try {
  await until("tauri-driver", () => fetch(`${DRIVER}/status`).then((r) => r.ok), 20000);
  session = (await wd("POST", "/session", { capabilities: { alwaysMatch: { "tauri:options": { application: app, args: ["--show"] } } } })).sessionId;

  await step("the window shows a row per app, the app's own icon and N8 Tray itself", async () => {
    await until("both rows", async () => (await rowFor("Fake one")) && (await rowFor("Fake two"))).catch(async (e) => {
      // Why not: ask the host through the window's own IPC.
      const probe = await wd("POST", `/session/${session}/execute/async`, {
        script:
          "const done = arguments[arguments.length - 1]; window.__TAURI_INTERNALS__.invoke('get_snapshot').then((s) => done(s ? 'snapshot: ' + s.apps.length + ' apps, ' + s.headline : 'no snapshot yet'), (err) => done('invoke failed: ' + err));",
        args: [],
      }).catch((err) => `probe failed: ${err.message}`);
      throw new Error(`${e.message} (${probe})`);
    });
    const icons = await css(".app img");
    if (icons.length < 2) throw new Error(`expected the app's icon and N8 Tray's, found ${icons.length} images`);
    if (!(await rowFor("N8 Tray"))) throw new Error("no row for N8 Tray itself");
    await shot("hub");
  });

  await step("an app that starts with N8 Tray runs and offers Stop", async () => {
    const row = await rowFor("Fake one");
    await until("Fake one to run", async () => buttonIn(row, "Stop Fake one"), 45000);
  });

  await step("Start in a row starts the app; Stop stops it", async () => {
    let row = await rowFor("Fake two");
    await click(await until("Start Fake two", () => buttonIn(row, "Start Fake two")));
    row = await rowFor("Fake two");
    const stop = await until("Fake two to run", async () => buttonIn((await rowFor("Fake two")), "Stop Fake two"), 45000);
    await until("Stop enabled", async () => (await js("return arguments[0].disabled;", asElement(stop))) === false);
    await click(stop);
    await until("Fake two to stop", async () => buttonIn((await rowFor("Fake two")), "Start Fake two"), 45000);
  });

  await step("a row opens its panel; Escape comes back", async () => {
    const row = await rowFor("Fake one");
    const main = idOf(await wd("POST", `/session/${session}/element/${row}/element`, { using: "css selector", value: ".app-main" }));
    await click(main);
    await until("the panel", async () => (await heading()) === "Fake one");
    await until("its processes", async () => (await css("table")).length > 0, 20000);
    await shot("panel");
    await keys(""); // Escape
    await until("the hub", async () => (await heading()) === "N8 Tray");
  });

  await step("status details open from the status card", async () => {
    await click((await css(".status"))[0]);
    await until("the details", async () => (await heading()) === "Status details");
    await shot("details");
    await keys("");
  });

  await step("the window is 380 CSS pixels wide", async () => {
    const width = await wd("POST", `/session/${session}/execute/sync`, { script: "return window.innerWidth", args: [] });
    if (Math.abs(width - 380) > 1) throw new Error(`width ${width}`);
  });
} catch (e) {
  failed = true;
  if (!String(e.message).startsWith("Timed out")) console.error(e);
} finally {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
  driver.kill();
  // The fake apps were started by N8 Tray and keep running: end them.
  spawn(app, ["--exit"], { env: { ...process.env, N8TRAY_HOME: home }, stdio: "ignore" });
  await sleep(1500);
}
process.exit(failed ? 1 : 0);
