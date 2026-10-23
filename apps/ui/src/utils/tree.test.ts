import { describe, expect, it } from "vitest";
import type { CornerRadii, NodeDetail, Rect, TreeNode } from "../engine/contract";
import {
  findNode,
  findNodeIndex,
  flattenVisibleNodes,
  nodeAncestors,
  subtreeIds,
  unionBounds,
} from "./tree";

const rect = (x: number, y: number, w: number, h: number): Rect => ({ x, y, w, h });

function treeNode(
  partial: Pick<TreeNode, "id" | "name" | "parent" | "depth"> & Partial<TreeNode>,
): TreeNode {
  return {
    kind: "rectangle",
    visible: true,
    locked: false,
    child_count: 0,
    is_component: false,
    is_instance: false,
    frame: rect(0, 0, 100, 100),
    ...partial,
  };
}

/** Page structure: frame f > (a, b > c). */
const nodes: TreeNode[] = [
  treeNode({ id: "f", name: "Home", parent: null, depth: 0, child_count: 2, frame: rect(0, 0, 800, 600) }),
  treeNode({ id: "a", name: "Hero", parent: "f", depth: 1, frame: rect(0, 0, 800, 200) }),
  treeNode({ id: "b", name: "Login Card", parent: "f", depth: 1, child_count: 1, frame: rect(20, 220, 320, 240) }),
  treeNode({ id: "c", name: "Submit", parent: "b", depth: 2, frame: rect(40, 400, 120, 40) }),
];

function detail(id: string, bounds: Rect): NodeDetail {
  const cornerRadii: CornerRadii = { tl: 0, tr: 0, br: 0, bl: 0 };
  return {
    id,
    name: id,
    kind: "rectangle",
    parent: null,
    transform: {
      x: bounds.x,
      y: bounds.y,
      w: bounds.w,
      h: bounds.h,
      rotation: 0,
      flip_x: false,
      flip_y: false,
    },
    visible: true,
    locked: false,
    opacity: 1,
    blend: "normal",
    fills: [],
    strokes: [],
    effects: [],
    corner_radii: cornerRadii,
    fill_rule: "nonzero",
    text: null,
    layout: null,
    constraints: null,
    component: null,
    is_inside_instance: false,
    absolute_bounds: bounds,
    resolved: null,
    prototype: null,
  };
}

describe("flattenVisibleNodes", () => {
  it("returns every node in pre-order by default", () => {
    expect(flattenVisibleNodes(nodes, new Set()).map((node) => node.id)).toEqual(["f", "a", "b", "c"]);
  });

  it("hides descendants of collapsed nodes", () => {
    expect(flattenVisibleNodes(nodes, new Set(["b"])).map((node) => node.id)).toEqual(["f", "a", "b"]);
    expect(flattenVisibleNodes(nodes, new Set(["f"])).map((node) => node.id)).toEqual(["f"]);
  });

  it("keeps matching nodes and their ancestors while searching, ignoring collapse", () => {
    const matches = flattenVisibleNodes(nodes, new Set(["f"]), "submit");
    expect(matches.map((node) => node.id)).toEqual(["f", "b", "c"]);
  });

  it("is case-insensitive while filtering", () => {
    expect(flattenVisibleNodes(nodes, new Set(), "HERO").map((node) => node.id)).toEqual(["f", "a"]);
  });
});

describe("lookups", () => {
  it("finds a node and its index", () => {
    expect(findNode(nodes, "c")?.name).toBe("Submit");
    expect(findNode(nodes, "missing")).toBeNull();
    expect(findNodeIndex(nodes, "b")).toBe(2);
    expect(findNodeIndex(nodes, "missing")).toBe(-1);
  });

  it("returns ancestors nearest-first", () => {
    expect(nodeAncestors(nodes, "c").map((node) => node.id)).toEqual(["b", "f"]);
  });

  it("collects a subtree from the flat pre-order list", () => {
    expect(subtreeIds(nodes, "f")).toEqual(["f", "a", "b", "c"]);
    expect(subtreeIds(nodes, "b")).toEqual(["b", "c"]);
    expect(subtreeIds(nodes, "missing")).toEqual([]);
  });
});

describe("unionBounds", () => {
  it("is null for an empty selection", () => {
    expect(unionBounds([])).toBeNull();
  });

  it("unions world-space AABBs", () => {
    expect(unionBounds([detail("one", rect(10, 20, 30, 40)), detail("two", rect(100, 50, 10, 10))])).toEqual({
      x: 10,
      y: 20,
      w: 100,
      h: 40,
    });
  });
});
