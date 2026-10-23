import { beforeEach, describe, expect, it } from "vitest";
import type { DocSummary } from "./contract";
import type { PaletteGroup, PaletteItem } from "./palette";
import {
  buildLayerItems,
  buildRecentItems,
  buildStyleItems,
  buildToolItems,
  filterPalette,
  groupPaletteItems,
  PALETTE_GROUP_ORDER,
} from "./palette";
import { defaultBindings } from "./shortcuts";

function paletteItem(
  id: string,
  group: PaletteGroup,
  title: string,
  keywords: string[] = [],
  subtitle?: string,
): PaletteItem {
  return { id, group, title, keywords, subtitle, run: () => undefined };
}

const actions = [
  paletteItem("undo", "actions", "Undo"),
  paletteItem("group", "actions", "Group selection", ["ungroup"]),
];
const tools = [paletteItem("rectangle", "tools", "Rectangle")];

beforeEach(() => {
  localStorage.clear();
});

describe("filterPalette", () => {
  it("keeps all rows in catalogue order for an empty query", () => {
    const items = [...actions, ...tools];
    const ranked = filterPalette(items, "  ");
    expect(ranked.map((item) => item.id)).toEqual(["undo", "group", "rectangle"]);
    expect(ranked.every((item) => item.score === 0 && item.titleIndices.length === 0)).toBe(true);
  });

  it("matches titles and reports highlight indices", () => {
    const ranked = filterPalette([...actions, ...tools], "rectang");
    expect(ranked.map((item) => item.id)).toEqual(["rectangle"]);
    expect(ranked[0]?.titleIndices.length).toBeGreaterThan(0);
  });

  it("matches keywords and subtitles as well as titles", () => {
    expect(filterPalette(actions, "ungroup").map((item) => item.id)).toEqual(["group"]);
    const withSubtitle = [paletteItem("layer", "layers", "Login Card", [], "Frame")];
    expect(filterPalette(withSubtitle, "frame").map((item) => item.id)).toEqual(["layer"]);
  });

  it("ranks title matches above keyword-only matches", () => {
    const titleMatch = paletteItem("a", "actions", "Zoom in");
    const keywordMatch = paletteItem("b", "tools", "Else", ["zoom in"]);
    const ranked = filterPalette([keywordMatch, titleMatch], "zoomin");
    expect(ranked.map((item) => item.id)).toEqual(["a", "b"]);
    expect(ranked[0]?.score ?? 0).toBeGreaterThan(ranked[1]?.score ?? 0);
  });

  it("returns no rows when nothing matches", () => {
    expect(filterPalette([...actions, ...tools], "zzzz")).toEqual([]);
  });
});

describe("groupPaletteItems", () => {
  it("groups ranked rows in section order and skips empty groups", () => {
    const ranked = filterPalette([...actions, ...tools], "");
    const sections = groupPaletteItems(ranked);
    expect(sections.map((section) => section.group)).toEqual(["actions", "tools"]);
    expect(PALETTE_GROUP_ORDER).toEqual(["actions", "tools", "layers", "styles", "recent"]);
    expect(sections[0]?.items.length).toBe(actions.length);
  });
});

describe("buildRecentItems", () => {
  it("replays recent ids in recency order and skips unknown ones", () => {
    const items = [...actions, ...tools];
    const recent = buildRecentItems(items, ["rectangle", "missing", "undo"]);
    expect(recent.map((item) => item.id)).toEqual(["rectangle", "undo"]);
    expect(recent.every((item) => item.group === "recent")).toBe(true);
  });
});

describe("buildToolItems", () => {
  it("lists every tool with its shortcut", () => {
    const items = buildToolItems(defaultBindings());
    expect(items).toHaveLength(15);
    expect(items.every((item) => item.group === "tools")).toBe(true);
    expect(items.find((item) => item.id === "tool.move")?.shortcut).toBe("V");
    expect(items.find((item) => item.id === "tool.slice")?.shortcut).toBeNull();
  });
});

describe("buildStyleItems / buildLayerItems", () => {
  const summary: DocSummary = {
    pages: [{ id: "p1", name: "Page 1" }],
    active_page: "p1",
    nodes: [
      {
        id: "n1",
        name: "Login Card",
        kind: "frame",
        depth: 0,
        parent: null,
        visible: true,
        locked: false,
        child_count: 0,
        is_component: false,
        is_instance: false,
        frame: { x: 0, y: 0, w: 320, h: 240 },
      },
    ],
    styles: {
      paints: [{ id: "paint1", name: "Coral" }],
      texts: [{ id: "text1", name: "Heading" }],
      effects: [],
    },
    components: [],
    variables: [],
  };

  it("returns nothing without a document", () => {
    expect(buildStyleItems(null)).toEqual([]);
    expect(buildLayerItems(null, [], () => "Unknown")).toEqual([]);
  });

  it("builds style and layer rows from the document summary", () => {
    expect(buildStyleItems(summary).map((item) => item.title)).toEqual(["Coral", "Heading"]);

    const layers = buildLayerItems(summary, ["n1"], () => "Frame");
    expect(layers).toHaveLength(1);
    expect(layers[0]).toMatchObject({ id: "layer.n1", group: "layers", title: "Login Card", disabled: true });
  });
});
