// N8 Tray's own cost against its budgets (plan 3.6, D24): idle with no
// window, with the window open, and after the window closed. The figures
// are the host's own measurement from its status file, the same ones the
// status details show. Private memory is the budget metric; the working set
// is reported next to it.
//
//   node scripts/tray-budget.mjs --app <n8tray> [--enforce] [--out budget.json]
//
// Without --enforce it reports and exits 0; with it, a figure over budget
// fails the run.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

export const BUDGET = {
  idlePrivateMb: 30,
  idleCpuPercent: 0.2,
  openPrivateMb: 150,
  closedPrivateMb: 35,
  closedWithinSeconds: 70,
};

const opts = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]?.startsWith("--") ? true : (all[i + 1] ?? true)]] : acc), []),
);
const app = resolve(opts.app);
const home = mkdtempSync(join(tmpdir(), "n8tray-budget-"));
const env = { ...process.env, N8TRAY_HOME: home };
const statusFile = join(home, "status.json");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** When the host last wrote its status file, in ms since the epoch. */
function writtenAt() {
  try {
    return statSync(statusFile).mtimeMs;
  } catch {
    return Date.now();
  }
}

function read() {
  try {
    return JSON.parse(readFileSync(statusFile, "utf8"));
  } catch {
    return null;
  }
}

async function until(what, test, seconds) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    const s = read();
    if (s && test(s)) return s;
    await sleep(500);
  }
  throw new Error(`Timed out after ${seconds} s waiting for ${what}`);
}

/** Reads the status file `count` times, `gapMs` apart (it is rewritten every 10 s). */
async function samples(count, gapMs) {
  const out = [];
  for (let i = 0; i < count; i++) {
    if (i) await sleep(gapMs);
    out.push(read().tray);
  }
  return out;
}

const max = (xs) => {
  const v = xs.filter((x) => x !== null && x !== undefined);
  return v.length ? Math.max(...v) : null;
};
// Private memory is measured on Windows; elsewhere only the working set is.
const hasPrivate = process.platform === "win32";
const avg = (xs) => {
  const v = xs.filter((x) => x !== null && x !== undefined);
  return v.length ? v.reduce((a, b) => a + b, 0) / v.length : null;
};
const run = (...args) => spawn(app, args, { env, stdio: "ignore", detached: false });

const tray = run();
const result = { platform: process.platform, budget: BUDGET, idle: {}, open: {}, closed: {}, failures: [] };
try {
  await until("the status file", () => true, 60);
  // Start-up touches far more than an idle tray keeps; it trims after 30 s.
  await sleep(40_000);
  const idle = await samples(3, 10_000);
  result.idle = {
    privateMb: max(idle.map((t) => t.privateMb)),
    workingSetMb: max(idle.map((t) => t.rssMb)),
    cpuPercent: avg(idle.map((t) => t.cpuPercent)),
  };

  run("--show");
  const opened = Date.now();
  // macOS runs WKWebView in system XPC processes, not children of the tray:
  // the window's own processes are not seen there.
  await until("the window's processes", (s) => s.tray.windowMb > 0, process.platform === "darwin" ? 15 : 60).catch((e) =>
    process.platform === "darwin" ? null : Promise.reject(e),
  );
  result.open.secondsToWindow = Math.round((Date.now() - opened) / 100) / 10;
  await sleep(12_000);
  const open = await samples(2, 10_000);
  result.open.trayPrivateMb = max(open.map((t) => t.privateMb));
  result.open.windowPrivateMb = max(open.map((t) => t.windowPrivateMb ?? 0));
  result.open.privateMb = hasPrivate ? max(open.map((t) => (t.privateMb ?? 0) + (t.windowPrivateMb ?? 0))) : null;
  result.open.workingSetMb = max(open.map((t) => t.rssMb + t.windowMb));
  result.open.cpuPercent = avg(open.map((t) => t.cpuPercent));

  // The status file is rewritten every 10 s, so a file can show the tray
  // back under budget up to 10 s after it was. The budget therefore fails
  // only on a sample written more than closedWithinSeconds after closing that
  // still shows the tray over it; each sample is timed by when it was
  // written, not when this script read it.
  run("--hide");
  const hidden = Date.now();
  const underBudget = (s) => s.tray.windowMb === 0 && (!hasPrivate || s.tray.privateMb <= BUDGET.closedPrivateMb);
  let lastOverSeconds = null;
  const closed = await until(
    `the tray back under ${BUDGET.closedPrivateMb} MB private`,
    (s) => {
      const seconds = (writtenAt() - hidden) / 1000;
      if (!underBudget(s)) lastOverSeconds = seconds;
      return underBudget(s) && Date.now() - hidden > 62_000;
    },
    BUDGET.closedWithinSeconds + 20,
  ).catch((e) => {
    result.failures.push(e.message);
    return read();
  });
  result.closed = {
    seconds: Math.round((writtenAt() - hidden) / 100) / 10,
    lastOverSeconds: lastOverSeconds === null ? null : Math.round(lastOverSeconds * 10) / 10,
    privateMb: closed.tray.privateMb,
    workingSetMb: closed.tray.rssMb,
    windowMb: closed.tray.windowMb,
  };

  const over = (name, value, limit) => {
    if (value !== null && value !== undefined && value > limit) result.failures.push(`${name} ${value.toFixed(1)} is over ${limit}`);
  };
  over("idle private MB", result.idle.privateMb, BUDGET.idlePrivateMb);
  over("idle CPU %", result.idle.cpuPercent, BUDGET.idleCpuPercent);
  over("window open private MB", result.open.privateMb, BUDGET.openPrivateMb);
  over("closed private MB", result.closed.privateMb, BUDGET.closedPrivateMb);
  over("seconds still over budget after closing", result.closed.lastOverSeconds, BUDGET.closedWithinSeconds);
} catch (e) {
  result.failures.push(e.message);
} finally {
  run("--exit");
  await sleep(2000);
  if (tray.exitCode === null) tray.kill();
}

const f = (v) => (v === null || v === undefined ? "-" : typeof v === "number" ? v.toFixed(1) : String(v));
console.log(`N8 Tray budget on ${result.platform}`);
console.log(`  idle         private ${f(result.idle.privateMb)} MB (<= ${BUDGET.idlePrivateMb}), working set ${f(result.idle.workingSetMb)} MB, CPU ${f(result.idle.cpuPercent)}% (<= ${BUDGET.idleCpuPercent})`);
console.log(`  window open  private ${f(result.open.privateMb)} MB (<= ${BUDGET.openPrivateMb}; tray ${f(result.open.trayPrivateMb)}, window ${f(result.open.windowPrivateMb)}), working set ${f(result.open.workingSetMb)} MB, CPU ${f(result.open.cpuPercent)}%, window in ${f(result.open.secondsToWindow)} s`);
console.log(`  closed       private ${f(result.closed.privateMb)} MB (<= ${BUDGET.closedPrivateMb}) in the sample written ${f(result.closed.seconds)} s after closing; last over budget at ${f(result.closed.lastOverSeconds)} s (<= ${BUDGET.closedWithinSeconds}); working set ${f(result.closed.workingSetMb)} MB`);
for (const failure of result.failures) console.log(`  OVER: ${failure}`);
if (opts.out && opts.out !== true) writeFileSync(opts.out, JSON.stringify(result, null, 2));
if (!existsSync(statusFile)) console.log("  (no status file was written)");
process.exit(opts.enforce && result.failures.length ? 1 : 0);
