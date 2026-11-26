// Writes every snapshot in preview/fixtures.ts to native/fixtures/<name>.json,
// the copies the native window and the Rust model test read. Run it with
// `npm run fixtures:json` (Node strips the types itself) after changing a
// fixture, and commit the files it rewrites.
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { fixtures } from "../preview/fixtures.ts";

const dir = fileURLToPath(new URL("../../../native/fixtures/", import.meta.url));
mkdirSync(dir, { recursive: true });
for (const [name, snapshot] of Object.entries(fixtures)) {
  writeFileSync(`${dir}${name}.json`, `${JSON.stringify(snapshot, null, 2)}\n`);
}
console.log(`wrote ${Object.keys(fixtures).length} fixtures to native/fixtures/`);
