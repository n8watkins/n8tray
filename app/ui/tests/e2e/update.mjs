// An update end to end on Windows, the way a user gets one: the older N8 Tray
// is installed from its NSIS installer, finds the newer version in its feed,
// and Install in its window downloads it, checks its signature, runs the
// installer and starts N8 Tray again on the new version.
//
//   node tests/e2e/update.mjs --old <old setup.exe> --new <new setup.exe>
//        --new-version 0.1.1 --native-driver <msedgedriver> [--out <dir>]
//
// Both installers must be signed with one throwaway key whose public half
// both builds carry, and built with dangerousInsecureTransportProtocol, since
// the feed is served from this computer over plain HTTP. No dependencies.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../../..");
const opts = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []),
);
const oldSetup = resolve(opts.old);
const newSetup = resolve(opts.new);
const newVersion = opts["new-version"];
const out = resolve(opts.out ?? join(here, "../../screens"));
const DRIVER = "http://127.0.0.1:4444";
const ELEMENT = "element-6066-11e4-a52f-4a5d6dc9c3b2";
const idOf = (e) => {
  const id = e?.[ELEMENT] ?? e?.ELEMENT ?? Object.values(e ?? {})[0];
  if (typeof id !== "string") throw new Error(`no element id in ${JSON.stringify(e)}`);
  return id;
};
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
    await sleep(250);
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

// The new version's installer and its feed, served from this computer.
const served = mkdtempSync(join(tmpdir(), "n8tray-update-feed-"));
const setupName = "n8tray-setup.exe";
copyFileSync(newSetup, join(served, setupName));
const server = createServer((req, res) => {
  const file = join(served, basename(new URL(req.url, "http://x").pathname));
  if (!existsSync(file)) {
    res.writeHead(404).end();
    return;
  }
  res.writeHead(200, { "content-type": file.endsWith(".json") ? "application/json" : "application/octet-stream" });
  res.end(readFileSync(file));
});
await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
const base = `http://127.0.0.1:${server.address().port}`;
execFileSync(
  process.execPath,
  [join(repo, "scripts/updater-feed.mjs"), "--version", newVersion, "--out", join(served, "latest.json"), "--platform", "windows-x86_64", "--url", `${base}/${setupName}`, "--sig", `${newSetup}.sig`, "--allow-http"],
  { stdio: "inherit" },
);

// N8 Tray runs in its own folders and reads the local feed. The installer
// N8 Tray starts, and the N8 Tray it starts again, inherit both.
const home = mkdtempSync(join(tmpdir(), "n8tray-update-"));
const env = { ...process.env, N8TRAY_HOME: home, N8TRAY_UPDATE_FEED: `${base}/latest.json` };
const installed = join(process.env.LOCALAPPDATA, "N8 Tray", "n8tray.exe");
const status = () => {
  try {
    return JSON.parse(readFileSync(join(home, "status.json"), "utf8"));
  } catch {
    return null;
  }
};

let failed = false;
let session;
let driver;
async function step(name, fn) {
  const started = Date.now();
  try {
    await fn();
    console.log(`ok - ${name} (${Date.now() - started} ms)`);
  } catch (e) {
    failed = true;
    console.log(`not ok - ${name}: ${e.message}`);
    if (session) {
      const png = await wd("GET", `/session/${session}/screenshot`).catch(() => null);
      if (png) {
        mkdirSync(out, { recursive: true });
        writeFileSync(join(out, `update-failed-${name.replace(/\W+/g, "-")}.png`), Buffer.from(png, "base64"));
      }
    }
    throw e;
  }
}

try {
  await step("the older N8 Tray installs from its installer, per user", async () => {
    execFileSync(oldSetup, ["/S"], { stdio: "inherit" });
    await until("n8tray.exe in %LOCALAPPDATA%\\N8 Tray", () => existsSync(installed), 60000);
  });

  driver = spawn("tauri-driver", ["--native-driver", opts["native-driver"]], { env, stdio: ["ignore", "inherit", "inherit"] });
  await until("tauri-driver", () => fetch(`${DRIVER}/status`).then((r) => r.ok), 20000);
  session = (await wd("POST", "/session", { capabilities: { alwaysMatch: { "tauri:options": { application: installed, args: ["--show"] } } } })).sessionId;
  const js = (script, ...args) => wd("POST", `/session/${session}/execute/sync`, { script, args });
  const oldVersion = await until("the host's status file", () => status()?.version, 30000);

  await step(`the window offers N8 Tray ${newVersion} after the first check`, async () => {
    if (oldVersion === newVersion) throw new Error(`the installed N8 Tray is already ${newVersion}`);
    // The first check runs a minute after start.
    await until("the update banner", () => js("return [...document.querySelectorAll('.banner.update button')].some((b) => b.textContent.trim() === 'Install');"), 120000);
    const text = await js("return document.querySelector('.banner.update')?.textContent ?? '';");
    if (!text.includes(newVersion)) throw new Error(`the banner says "${text}"`);
    const png = await wd("GET", `/session/${session}/screenshot`);
    mkdirSync(out, { recursive: true });
    writeFileSync(join(out, "update-available.png"), Buffer.from(png, "base64"));
  });

  await step("Install downloads and verifies it, and N8 Tray starts again on the new version", async () => {
    await js("[...document.querySelectorAll('.banner.update button')].find((b) => b.textContent.trim() === 'Install').click();");
    await until(`N8 Tray ${newVersion} in the status file`, () => status()?.version === newVersion, 240000);
  });

  await step("the installed program is the new version", async () => {
    const fileVersion = execFileSync("powershell", ["-NoProfile", "-Command", `(Get-Item '${installed}').VersionInfo.ProductVersion`], { encoding: "utf8" }).trim();
    if (!fileVersion.startsWith(newVersion)) throw new Error(`n8tray.exe is ${fileVersion}`);
  });
} catch (e) {
  failed = true;
  if (!String(e.message).startsWith("Timed out")) console.error(e);
  // The tray's own log says how far the update got.
  for (const name of existsSync(join(home, "logs")) ? readdirSync(join(home, "logs")) : []) {
    console.log(`--- ${name}\n${readFileSync(join(home, "logs", name), "utf8").split("\n").slice(-60).join("\n")}`);
  }
} finally {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
  driver?.kill();
  // End whichever N8 Tray runs now, the one the installer started included.
  if (existsSync(installed)) spawn(installed, ["--exit"], { env, stdio: "ignore" });
  await sleep(2000);
  server.close();
}
process.exit(failed ? 1 : 0);
