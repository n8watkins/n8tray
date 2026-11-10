#!/usr/bin/env node
// Writes the update feed Tauri's updater reads (latest.json): the version,
// its notes and, per platform, the download URL and the minisign signature
// the build wrote next to the installer (.sig).
//
//   node scripts/updater-feed.mjs --version 0.2.0 --out latest.json \
//     --platform windows-x86_64 --url https://.../N8.Tray_0.2.0_x64-setup.exe \
//     --sig "target/release/bundle/nsis/N8 Tray_0.2.0_x64-setup.exe.sig" \
//     [--notes-file notes.md] [--allow-http]
//
// --platform, --url and --sig repeat together, once per platform. Plain HTTP
// URLs are refused unless --allow-http (the end-to-end test serves the
// update from this computer). No dependencies.
import { readFileSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const one = (name) => {
  const at = args.indexOf(`--${name}`);
  return at === -1 ? undefined : args[at + 1];
};
const all = (name) => args.flatMap((a, i) => (a === `--${name}` ? [args[i + 1]] : []));
const fail = (message) => {
  console.error(`updater-feed: ${message}`);
  process.exit(1);
};

const version = (one("version") ?? "").replace(/^v/, "");
if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z.-]+)?$/.test(version)) fail(`"${version}" is not a version like 1.2.3`);
const out = one("out") ?? fail("--out is required");
const platforms = all("platform");
const urls = all("url");
const sigs = all("sig");
if (!platforms.length || platforms.length !== urls.length || urls.length !== sigs.length) {
  fail("give --platform, --url and --sig once each per platform");
}
const allowHttp = args.includes("--allow-http");

const feed = {
  version,
  notes: one("notes-file") ? readFileSync(one("notes-file"), "utf8").trim() : "",
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
  platforms: {},
};
platforms.forEach((platform, i) => {
  if (!/^(windows|darwin|linux)-(x86_64|aarch64|i686|armv7)(-[a-z]+)?$/.test(platform)) fail(`"${platform}" is not an updater platform like windows-x86_64`);
  let url;
  try {
    url = new URL(urls[i]);
  } catch {
    fail(`"${urls[i]}" is not a URL`);
  }
  if (url.protocol !== "https:" && !(allowHttp && url.protocol === "http:")) fail(`${url} must be HTTPS`);
  const signature = readFileSync(sigs[i], "utf8").trim();
  // A Tauri signature is the base64 of a minisign signature file.
  const decoded = Buffer.from(signature, "base64").toString("utf8");
  if (!decoded.startsWith("untrusted comment:")) fail(`${sigs[i]} is not a Tauri updater signature`);
  feed.platforms[platform] = { signature, url: url.href };
});

writeFileSync(out, `${JSON.stringify(feed, null, 2)}\n`);
console.log(`updater-feed: ${out}: ${version} for ${platforms.join(", ")}`);
