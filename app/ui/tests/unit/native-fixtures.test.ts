import { describe, expect, it } from "vitest";
import { fixtures } from "../../preview/fixtures";

// native/fixtures/*.json holds the same snapshots for the native window; the
// Rust test in n8tray-core checks them against the model, this one checks they
// are still the snapshots the interface tests use.
const files = import.meta.glob("../../../../native/fixtures/*.json", { eager: true, import: "default" });

describe("native fixtures", () => {
  it("has one JSON file for every interface fixture", () => {
    const names = Object.keys(files).map((p) => p.replace(/^.*\/(.+)\.json$/, "$1"));
    expect(names.sort()).toEqual(Object.keys(fixtures).sort());
  });

  for (const [path, json] of Object.entries(files)) {
    const name = path.replace(/^.*\/(.+)\.json$/, "$1");
    it(`${name}.json matches the interface fixture`, () => {
      expect(json).toEqual(fixtures[name]);
    });
  }
});
