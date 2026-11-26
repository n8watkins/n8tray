// Until the switch, nothing users get may contain the native shell (D33).
// This checks what actually decides that:
//
// - app/src-tauri/tauri.conf.json: the installer bundles no native file
//   (`bundle.externalBin`, `bundle.resources`) and the build step before the
//   bundle is still the window's own (`build.beforeBuildCommand`);
// - .github/workflows/app-release.yml, its commands only (comments are
//   ignored): it runs `tauri build`, and nothing that builds the native shell
//   (a root `npm run build`, a whole-workspace `cargo build`, anything naming
//   the native crate or folder).
//
//   node native/scripts/check-release.mjs              check the repo
//   node native/scripts/check-release.mjs --self-test  check that bad inputs fail
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const BEFORE_BUILD = "npm --prefix ui run build";

/** Problems in a parsed tauri.conf.json. */
export function checkTauriConf(conf) {
  const problems = [];
  const bundle = conf.bundle ?? {};
  for (const key of ["externalBin", "resources"]) {
    const value = bundle[key];
    if (value === undefined) continue;
    const text = JSON.stringify(value);
    if (/native/i.test(text)) problems.push(`tauri.conf.json bundle.${key} names the native shell: ${text}`);
  }
  if (conf.build?.beforeBuildCommand !== BEFORE_BUILD) {
    problems.push(`tauri.conf.json build.beforeBuildCommand is ${JSON.stringify(conf.build?.beforeBuildCommand)}, not ${JSON.stringify(BEFORE_BUILD)}`);
  }
  return problems;
}

/** A workflow's lines without comments (whole-line and trailing `# ...`). */
function commandLines(yaml) {
  return yaml
    .split("\n")
    .map((line) => line.replace(/(^|\s)#.*$/, "").trimEnd())
    .filter((line) => line.trim() !== "");
}

/** Problems in app-release.yml's text. */
export function checkReleaseWorkflow(yaml) {
  const problems = [];
  const lines = commandLines(yaml);
  const bad = [
    [/n8tray-native|(^|[\s/"'=])native\//, "names the native crate or folder"],
    [/--workspace\b/, "builds the whole workspace"],
    [/\bcargo\s+(build|install|run|rustc)\b/, "runs cargo to build something other than the Tauri app"],
    [/\bnpm\s+(run\s+)?build\b(?!.*--prefix\s+(app\/)?ui)/, "runs a root npm build"],
    [/--manifest-path/, "points cargo at another manifest"],
  ];
  for (const line of lines) {
    for (const [pattern, why] of bad) {
      if (pattern.test(line)) problems.push(`app-release.yml ${why}: ${line.trim()}`);
    }
  }
  if (!lines.some((line) => /\btauri\s+build\b/.test(line))) {
    problems.push("app-release.yml no longer runs tauri build (outside comments); update this check");
  }
  return problems;
}

function selfTest() {
  const goodConf = { build: { beforeBuildCommand: BEFORE_BUILD }, bundle: { active: true } };
  const goodYaml = "steps:\n  - run: npm ci --prefix app/ui\n  - run: |\n      ../ui/node_modules/.bin/tauri build --ci --bundles nsis\n";
  const cases = [
    ["a clean config", checkTauriConf(goodConf), 0],
    ["a clean workflow", checkReleaseWorkflow(goodYaml), 0],
    ["a native external binary", checkTauriConf({ ...goodConf, bundle: { externalBin: ["../../native/target/release/n8tray-native"] } }), 1],
    ["a native resource", checkTauriConf({ ...goodConf, bundle: { resources: { "../../native/target/release/n8tray-native.exe": "n8tray-native.exe" } } }), 1],
    ["a changed build step", checkTauriConf({ ...goodConf, build: { beforeBuildCommand: "npm run build && cargo build --manifest-path ../../native/Cargo.toml" } }), 1],
    ["a root npm build", checkReleaseWorkflow(goodYaml + "  - run: npm run build\n"), 1],
    ["a workspace build", checkReleaseWorkflow(goodYaml + "  - run: cargo build --release --workspace\n"), 2],
    ["the native crate by name", checkReleaseWorkflow(goodYaml + "  - run: cargo build -p n8tray-native\n"), 2],
    ["the native manifest", checkReleaseWorkflow(goodYaml + "  - run: cargo build --manifest-path native/Cargo.toml\n"), 3],
    ["tauri build only in a comment", checkReleaseWorkflow("steps:\n  # tauri build used to run here\n  - run: echo nothing\n"), 1],
    ["a native name only in a comment", checkReleaseWorkflow(goodYaml + "  # n8tray-native is never built here\n"), 0],
  ];
  let failed = 0;
  for (const [name, problems, expected] of cases) {
    const ok = problems.length === expected;
    if (!ok) failed++;
    console.log(`${ok ? "ok  " : "FAIL"} ${name}: ${problems.length} problem(s), expected ${expected}${ok ? "" : `\n     ${problems.join("\n     ")}`}`);
  }
  if (failed) {
    console.error(`The release check's self-test failed (${failed} case(s)).`);
    process.exit(1);
  }
  console.log("The release check catches every bad sample.");
}

if (process.argv.includes("--self-test")) {
  selfTest();
} else {
  const conf = JSON.parse(readFileSync(join(root, "app/src-tauri/tauri.conf.json"), "utf8"));
  const yaml = readFileSync(join(root, ".github/workflows/app-release.yml"), "utf8");
  const problems = [...checkTauriConf(conf), ...checkReleaseWorkflow(yaml)];
  for (const problem of problems) console.error(`::error::${problem}`);
  if (problems.length) process.exit(1);
  console.log("The installer and App release build the Tauri app only; the native shell stays out (D33).");
}
