import { describe, expect, it } from "vitest";
import { busy, empty, healthy, trouble } from "../../preview/fixtures";
import { cpu, memory, percent } from "../../src/format";
import { nextIndex } from "../../src/keyboard";
import {
  componentActions,
  groups,
  hubApps,
  memoryMeter,
  points,
  restartAction,
  rowAction,
  rowFigures,
  rowLine,
  statusCard,
} from "../../src/model";

const app = (id: string, s = busy) => s.apps.find((a) => a.id === id)!;

describe("rows", () => {
  it("offer one action: Stop while running, Start while not", () => {
    expect(rowAction(app("voicedock"))?.kind).toBe("stop");
    expect(rowAction(app("n8commentary"))?.kind).toBe("start");
    expect(rowAction(app("wslmemory"))).toBeNull();
  });

  it("keep Restart for the panel, only while running", () => {
    expect(restartAction(app("voicedock"))?.id).toBe("restart");
    expect(restartAction(app("n8commentary"))).toBeNull();
  });

  it("show memory measured plus reported, and say when most of it is in WSL", () => {
    expect(rowFigures(app("voicedock"))).toEqual({ memory: "643 MB", sub: "12% CPU" });
    expect(rowFigures(app("leadscraper"))).toEqual({ memory: "1.2 GB", sub: "in WSL" });
    expect(rowFigures(app("n8commentary"))).toBeNull();
  });

  it("put what happens now first, then failures, alerts and the summary", () => {
    expect(rowLine(app("n8hotkeys", trouble))).toEqual({ text: "Restarting", tone: "normal" });
    expect(rowLine(app("voicedock", trouble)).tone).toBe("problem");
    expect(rowLine(app("leadscraper"))).toEqual({ text: "Over its 1 GB warning", tone: "attention" });
    expect(rowLine(app("n8hotkeys")).text).toBe("6 hotkeys registered");
  });

  it("are grouped in the snapshot's order, hidden and own-icon apps left out", () => {
    const shown = hubApps({ ...busy, apps: [...busy.apps, { ...app("voicedock"), id: "x", hosted: false }] });
    expect(shown.map((a) => a.id)).not.toContain("x");
    expect(groups(hubApps(busy)).map((g) => g.name)).toEqual(["Audio", "Hotkeys", "Streaming", "System", "Work"]);
  });

  it("name the component in per-component actions", () => {
    const vd = app("voicedock");
    expect(componentActions(vd, vd.components[0]).map((c) => c.label)).toEqual(["Start Kokoro", "Stop Kokoro"]);
  });
});

describe("status card and meter", () => {
  it("lead with the reason, else with what runs", () => {
    expect(statusCard(busy).big).toBe("Lead Scraper is over its 1 GB warning");
    expect(statusCard(healthy).big).toBe("2 of 3 running");
    expect(statusCard(empty).big).toBe("No apps installed yet");
  });

  it("show used memory and the system's name", () => {
    const m = memoryMeter(busy)!;
    expect(m.label).toBe("Windows");
    expect(m.text).toBe("4.2 / 31.1 GB free");
    expect(m.used).toBeCloseTo(1 - 4.2 / 31.1);
  });
});

describe("drawing and words", () => {
  it("scale points into the box from zero", () => {
    expect(points([0, 10], 10, 10)).toBe("0.0,9.0 10.0,2.9");
    expect(points([], 10, 10)).toBe("");
  });

  it("format memory and CPU", () => {
    expect(memory(1229)).toBe("1.2 GB");
    expect(memory(42.4)).toBe("42 MB");
    expect(cpu(0)).toBe("0% CPU");
    expect(cpu(0.14)).toBe("0.1% CPU");
    expect(cpu(12.4)).toBe("12% CPU");
    expect(percent(null)).toBe("-");
  });

  it("move through lists with arrows, Home and End", () => {
    expect(nextIndex("ArrowDown", -1, 3)).toBe(0);
    expect(nextIndex("ArrowDown", 2, 3)).toBe(2);
    expect(nextIndex("ArrowUp", 0, 3)).toBe(0);
    expect(nextIndex("End", 0, 3)).toBe(2);
    expect(nextIndex("Home", 2, 3)).toBe(0);
    expect(nextIndex("a", 1, 3)).toBeNull();
  });
});
