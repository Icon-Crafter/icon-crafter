import { describe, expect, it } from "vitest";

import {
  addCapture,
  CAPTURE_STORAGE_KEY,
  formatColorCss,
  formatDiffCounts,
  formatFillCss,
  formatMeasurements,
  loadCaptures,
  MAX_CAPTURES,
  nextCaptureLabel,
  removeCapture,
  saveCaptures,
  summarizeDiffForUi,
  type RevisionCapture,
} from "./devMode";
import type {
  DiffCounts,
  InspectSnapshot,
  NodeChange,
  NodeDiff,
  NodeInspect,
  PaintInfo,
  RevisionDiff,
} from "./contract";

class MemoryStorage {
  private items = new Map<string, string>();

  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.items.set(key, value);
  }

  removeItem(key: string): void {
    this.items.delete(key);
  }
}

const storage = (): Storage => new MemoryStorage() as unknown as Storage;

function paint(overrides: Partial<PaintInfo> = {}): PaintInfo {
  return {
    kind: "solid",
    color: { r: 1, g: 0.42, b: 0.29, a: 1 },
    hex: "#FF6B4A",
    stops: [],
    opacity: 1,
    blend: "normal",
    style_id: null,
    variable_id: null,
    variable_name: null,
    ...overrides,
  };
}

function node(overrides: Partial<NodeInspect> = {}): NodeInspect {
  return {
    id: "n1",
    name: "Card",
    kind: "rectangle",
    page: "Page 1",
    parent: null,
    children: [],
    bounds: [10, 20, 120, 64],
    size: [120, 64],
    local_position: [10, 20],
    rotation: 0,
    flip: [false, false],
    opacity: 1,
    blend: "normal",
    visible: true,
    locked: false,
    matrix: [1, 0, 0, 1, 10, 20],
    fills: [paint()],
    strokes: [],
    effects: [],
    corner_radii: [0, 0, 0, 0],
    layout_direction: null,
    layout_gap: null,
    layout_padding: null,
    layout_sizing: null,
    constraints: null,
    text: null,
    component: null,
    variable_refs: [],
    style_refs: [],
    ...overrides,
  };
}

function snapshot(nodes: NodeInspect[], warnings: string[] = []): InspectSnapshot {
  return { nodes, bounds: nodes.length > 0 ? [0, 0, 200, 200] : null, total_size: [200, 200], warnings };
}

function capture(overrides: Partial<RevisionCapture> = {}): RevisionCapture {
  return {
    id: "cap_1",
    label: "Revision 3",
    revision: 3,
    captured_at: "2026-10-24T00:00:00.000Z",
    snapshot: '{"nodes":[]}',
    ...overrides,
  };
}

function diff(overrides: Partial<RevisionDiff> = {}): RevisionDiff {
  const counts: DiffCounts = { added: 0, removed: 0, changed: 0 };
  return { added: [], removed: [], changed: [], counts, ...overrides };
}

function nodeDiff(id: string, name = id): NodeDiff {
  return { id, name, kind: "rectangle" };
}

describe("formatColorCss", () => {
  it("formats opaque colours as uppercase hex", () => {
    expect(formatColorCss({ r: 1, g: 0.42, b: 0.29, a: 1 })).toBe("#FF6B4A");
  });

  it("uses rgba when alpha is below one", () => {
    expect(formatColorCss({ r: 0, g: 0.5, b: 1, a: 0.5 })).toBe("rgba(0, 128, 255, 0.5)");
  });

  it("clamps out-of-range channels", () => {
    expect(formatColorCss({ r: 2, g: -1, b: 0.5, a: 1 })).toBe("#FF0080");
  });
});

describe("formatFillCss", () => {
  it("returns the solid colour", () => {
    expect(formatFillCss(paint())).toBe("#FF6B4A");
  });

  it("uses the first gradient stop otherwise", () => {
    const gradient = paint({
      kind: "linear",
      color: null,
      hex: null,
      stops: [{ position: 0, color: { r: 0, g: 0, b: 0, a: 1 }, hex: "#000000" }],
    });
    expect(formatFillCss(gradient)).toBe("#000000");
  });

  it("returns null for images and missing paints", () => {
    expect(formatFillCss(paint({ kind: "image", color: null, stops: [] }))).toBeNull();
    expect(formatFillCss(null)).toBeNull();
    expect(formatFillCss(undefined)).toBeNull();
  });
});

describe("formatMeasurements", () => {
  it("explains an empty selection with its warnings", () => {
    const text = formatMeasurements(snapshot([], ["Auto layout is not simulated here."]));
    expect(text).toContain("No layers selected.");
    expect(text).toContain("Note: Auto layout is not simulated here.");
  });

  it("includes metrics, fill, stroke, text and component details", () => {
    const text = formatMeasurements(
      snapshot([
        node({
          name: "Button",
          bounds: [12.345, 20, 120, 64],
          fills: [paint()],
          strokes: [
            {
              paint: paint({
                color: { r: 0.06, g: 0.07, b: 0.09, a: 1 },
                hex: "#101318",
              }),
              weight: 2,
              align: "inside",
              cap: "butt",
              join: "miter",
              miter_limit: 4,
              dash: null,
            },
          ],
          effects: [
            {
              kind: "drop_shadow",
              color: null,
              offset: [0, 4],
              blur: 12,
              spread: 0,
              blend: "normal",
            },
          ],
          text: {
            characters: "Hello",
            font_family: "Inter",
            font_size: 20,
            font_weight: 600,
            italic: false,
            line_height: 28,
            letter_spacing: 0,
            align: "left",
            vertical_align: "top",
            color: null,
            overflow: false,
          },
          component: {
            role: "main",
            component_id: null,
            component_name: null,
            override_count: 0,
            inside_instance: false,
          },
        }),
      ]),
    );

    expect(text).toContain("Button (rectangle)");
    expect(text).toContain("Size  120 × 64");
    expect(text).toContain("Fill  #FF6B4A");
    expect(text).toContain("Stroke  #0F1217 2 px inside");
    expect(text).toContain("Text  Inter 20 px / 600");
    expect(text).toContain("Component  main");
  });
});

describe("summarizeDiffForUi", () => {
  it("re-labels engine added/removed for the capture direction", () => {
    const raw = diff({
      added: [nodeDiff("old-1")],
      removed: [nodeDiff("new-1")],
      changed: [{ id: "keep", name: "Keep", fields: ["name"] }] satisfies NodeChange[],
    });

    const summary = summarizeDiffForUi(raw);

    expect(summary.addedSinceCapture.map((item) => item.id)).toEqual(["new-1"]);
    expect(summary.removedSinceCapture.map((item) => item.id)).toEqual(["old-1"]);
    expect(summary.changed).toHaveLength(1);
    expect(summary.counts).toEqual({ added: 1, removed: 1, changed: 1 });
    expect(summary.hasChanges).toBe(true);
  });

  it("reports no structural changes when lists are empty", () => {
    const summary = summarizeDiffForUi(diff());
    expect(summary.hasChanges).toBe(false);
    expect(formatDiffCounts(summary)).toBe("No structural changes since this capture.");
  });

  it("formats counts for the result header", () => {
    const summary = summarizeDiffForUi(
      diff({ removed: [nodeDiff("a")], added: [nodeDiff("b")] }),
    );
    const text = formatDiffCounts(summary);
    expect(text).toContain("1 added");
    expect(text).toContain("1 removed");
    expect(text).toContain("0 changed");
  });
});

describe("revision capture helpers", () => {
  it("round-trips captures through storage", () => {
    const target = storage();
    saveCaptures([capture(), capture({ id: "cap_2", label: "Revision 4", revision: 4 })], target);

    const loaded = loadCaptures(target);
    expect(loaded).toHaveLength(2);
    expect(loaded[0]?.label).toBe("Revision 3");
    expect(loaded[1]?.revision).toBe(4);
  });

  it("tolerates corrupt storage", () => {
    const target = storage();
    target.setItem(CAPTURE_STORAGE_KEY, "not json");
    expect(loadCaptures(target)).toEqual([]);
    expect(loadCaptures(undefined)).toEqual([]);
  });

  it("dedupes an identical revision/snapshot capture", () => {
    const first = capture();
    const duplicate = capture({ id: "cap_dup" });
    const existing = [first, capture({ id: "cap_other", revision: 2, snapshot: "{}" })];

    const next = addCapture(existing, duplicate);

    expect(next[0]?.id).toBe("cap_dup");
    expect(next.filter((item) => item.revision === 3)).toHaveLength(1);
    expect(next).toHaveLength(2);
  });

  it("caps the list at the capture limit", () => {
    let captures: RevisionCapture[] = [];
    for (let index = 0; index < MAX_CAPTURES + 2; index += 1) {
      captures = addCapture(captures, capture({ id: `cap_${index}`, revision: index }));
    }
    expect(captures).toHaveLength(MAX_CAPTURES);
    expect(captures[0]?.id).toBe(`cap_${MAX_CAPTURES + 1}`);
  });

  it("suggests unique labels per revision", () => {
    expect(nextCaptureLabel([], 7)).toBe("Revision 7");
    expect(nextCaptureLabel([capture({ label: "Revision 7" })], 7)).toBe("Revision 7 (2)");
    expect(
      nextCaptureLabel([capture({ label: "Revision 7" }), capture({ label: "Revision 7 (2)" })], 7),
    ).toBe("Revision 7 (3)");
  });

  it("removes by id without touching others", () => {
    const captures = [capture(), capture({ id: "cap_2" })];
    expect(removeCapture(captures, "cap_1").map((item) => item.id)).toEqual(["cap_2"]);
  });
});
