import { describe, expect, it } from "vitest";
import type {
  DocSummary,
  EngineState,
  NodeDetail,
  PrototypeHotspot,
  Rect,
  Viewport,
} from "./contract";
import { initialEngineState } from "./contract";
import {
  ACTION_TYPES,
  TRIGGER_TYPES,
  actionLabel,
  buildAction,
  buildTrigger,
  canPresent,
  containerOptions,
  emptyInteractionForm,
  frameNameFor,
  framesFromSummary,
  hotspotAtScreenPoint,
  hotspotScreenRect,
  interactionCount,
  presentTargetId,
  screenHotspots,
  triggerLabel,
  validateInteractionForm,
} from "./prototype";

const VIEWPORT: Viewport = { x: 10, y: 20, zoom: 2, width: 800, height: 600, dpr: 1 };

function treeNode(id: string, kind: NodeDetail["kind"], name = id): DocSummary["nodes"][number] {
  return {
    id,
    name,
    kind,
    depth: 0,
    parent: null,
    visible: true,
    locked: false,
    child_count: 0,
    is_component: false,
    is_instance: false,
    frame: { x: 0, y: 0, w: 10, h: 10 },
  };
}

function summary(nodes: DocSummary["nodes"], pages = [{ id: "page_1", name: "Page 1" }]): DocSummary {
  return {
    pages,
    active_page: pages[0]?.id ?? "",
    nodes,
    styles: { paints: [], texts: [], effects: [] },
    components: [],
    variables: [],
  };
}

function nodeDetail(id: string, kind: NodeDetail["kind"]): NodeDetail {
  return {
    id,
    name: id,
    kind,
    parent: null,
    transform: { x: 0, y: 0, w: 10, h: 10, rotation: 0, flip_x: false, flip_y: false },
    visible: true,
    locked: false,
    opacity: 1,
    blend: "normal",
    fills: [],
    strokes: [],
    effects: [],
    corner_radii: { tl: 0, tr: 0, br: 0, bl: 0 },
    fill_rule: "nonzero",
    text: null,
    layout: null,
    constraints: null,
    component: null,
    is_inside_instance: false,
    absolute_bounds: { x: 0, y: 0, w: 10, h: 10 },
    resolved: null,
    prototype: null,
  };
}

function engineWith(partial: Partial<EngineState>): EngineState {
  return { ...initialEngineState(VIEWPORT), ...partial };
}

const RECT: Rect = { x: 15, y: 25, w: 100, h: 50 };

function hotspot(nodeId: string, rect: Rect): PrototypeHotspot {
  return { node_id: nodeId, rect, label: `${nodeId} label` };
}

describe("buildTrigger", () => {
  it("builds click triggers with zero delay", () => {
    expect(buildTrigger({ ...emptyInteractionForm(), triggerType: "click", delayMs: 999 })).toEqual({
      type: "click",
      delay_ms: 0,
    });
  });

  it("builds press and hover triggers without delay fields", () => {
    expect(buildTrigger({ ...emptyInteractionForm(), triggerType: "press" })).toEqual({ type: "press" });
    expect(buildTrigger({ ...emptyInteractionForm(), triggerType: "hover" })).toEqual({ type: "hover" });
  });

  it("clamps and rounds after-delay milliseconds", () => {
    expect(buildTrigger({ ...emptyInteractionForm(), triggerType: "after_delay", delayMs: -12 })).toEqual({
      type: "after_delay",
      delay_ms: 0,
    });
    expect(buildTrigger({ ...emptyInteractionForm(), triggerType: "after_delay", delayMs: 450.6 })).toEqual({
      type: "after_delay",
      delay_ms: 451,
    });
  });
});

describe("buildAction", () => {
  it("requires a frame for navigation and overlay actions", () => {
    const form = emptyInteractionForm();
    expect(buildAction({ ...form, actionType: "navigate_to" })).toBeNull();
    expect(buildAction({ ...form, actionType: "navigate_to", frameId: "frame_2" })).toEqual({
      type: "navigate_to",
      frame_id: "frame_2",
    });
    expect(buildAction({ ...form, actionType: "open_overlay", frameId: "frame_3" })).toEqual({
      type: "open_overlay",
      frame_id: "frame_3",
    });
  });

  it("builds delay-free actions and requires a scroll target", () => {
    const form = emptyInteractionForm();
    expect(buildAction({ ...form, actionType: "close_overlay" })).toEqual({ type: "close_overlay" });
    expect(buildAction({ ...form, actionType: "back" })).toEqual({ type: "back" });
    expect(buildAction({ ...form, actionType: "scroll_to" })).toBeNull();
    expect(buildAction({ ...form, actionType: "scroll_to", target: "node_9" })).toEqual({
      type: "scroll_to",
      target: "node_9",
    });
  });
});

describe("validateInteractionForm", () => {
  it("reports missing frame and target requirements", () => {
    const form = emptyInteractionForm();
    expect(validateInteractionForm({ ...form, actionType: "navigate_to" })).toBe("frameRequired");
    expect(validateInteractionForm({ ...form, actionType: "open_overlay" })).toBe("frameRequired");
    expect(validateInteractionForm({ ...form, actionType: "scroll_to" })).toBe("targetRequired");
    expect(validateInteractionForm({ ...form, actionType: "back" })).toBeNull();
    expect(validateInteractionForm({ ...form, actionType: "navigate_to", frameId: "f" })).toBeNull();
  });
});

describe("frame options", () => {
  it("lists frames topmost-first and containers for scroll targets", () => {
    const doc = summary([
      treeNode("frame_1", "frame", "Home"),
      treeNode("rect_1", "rectangle"),
      treeNode("frame_2", "frame", "Detail"),
      treeNode("component_1", "component"),
      treeNode("group_1", "group"),
      treeNode("text_1", "text"),
    ]);
    expect(framesFromSummary(doc)).toEqual([
      { id: "frame_1", name: "Home" },
      { id: "frame_2", name: "Detail" },
    ]);
    expect(containerOptions(doc).map((option) => option.id)).toEqual([
      "frame_1",
      "frame_2",
      "component_1",
      "group_1",
    ]);
    expect(framesFromSummary(null)).toEqual([]);
    expect(frameNameFor(doc, "frame_2")).toBe("Detail");
    expect(frameNameFor(doc, "missing")).toBe("Frame");
  });
});

describe("presentTargetId", () => {
  it("prefers a selected frame, then the first frame", () => {
    const doc = summary([treeNode("frame_1", "frame"), treeNode("frame_2", "frame")]);
    const withSelection = engineWith({
      summary: doc,
      selection_details: [nodeDetail("frame_2", "frame")],
    });
    expect(presentTargetId(withSelection)).toEqual({ ok: true, frameId: "frame_2" });

    const withoutSelection = engineWith({ summary: doc });
    expect(presentTargetId(withoutSelection)).toEqual({ ok: true, frameId: "frame_1" });
  });

  it("fails honestly when the document has no frames", () => {
    const doc = summary([treeNode("rect_1", "rectangle")]);
    expect(presentTargetId(engineWith({ summary: doc }))).toEqual({ ok: false });
    expect(canPresent(engineWith({ summary: doc }), true)).toBe(false);
  });

  it("requires the engine capability", () => {
    const doc = summary([treeNode("frame_1", "frame")]);
    expect(canPresent(engineWith({ summary: doc }), true)).toBe(true);
    expect(canPresent(engineWith({ summary: doc }), undefined)).toBe(false);
  });
});

describe("presentation geometry", () => {
  it("projects window rectangles into screen space", () => {
    expect(hotspotScreenRect(RECT, VIEWPORT)).toEqual({ x: 10, y: 10, w: 200, h: 100 });
    expect(screenHotspots([hotspot("a", RECT)], VIEWPORT)).toEqual([
      { nodeId: "a", label: "a label", rect: { x: 10, y: 10, w: 200, h: 100 } },
    ]);
    expect(screenHotspots(undefined, VIEWPORT)).toEqual([]);
  });

  it("hit-tests topmost hotspots first", () => {
    const lower = hotspot("lower", { x: 10, y: 20, w: 100, h: 100 });
    const upper = hotspot("upper", { x: 20, y: 30, w: 50, h: 50 });
    const center = { x: 40, y: 60 };
    expect(hotspotAtScreenPoint([lower, upper], VIEWPORT, center)?.node_id).toBe("upper");
    expect(hotspotAtScreenPoint([lower, upper], VIEWPORT, { x: -80, y: -80 })).toBeNull();
    expect(hotspotAtScreenPoint(undefined, VIEWPORT, center)).toBeNull();
  });
});

describe("labels", () => {
  it("formats trigger and action labels through injected translations", () => {
    const triggerLabels = {
      click: "On click",
      hover: "On hover",
      press: "On press",
      afterDelay: (ms: number) => `After ${ms} ms`,
    };
    expect(triggerLabel({ type: "click", delay_ms: 0 }, triggerLabels)).toBe("On click");
    expect(triggerLabel({ type: "after_delay", delay_ms: 250 }, triggerLabels)).toBe("After 250 ms");
    expect(triggerLabel({ type: "key", key: "Enter" }, triggerLabels)).toBe("Enter");

    const actionLabels = {
      navigateTo: (frame: string) => `Go to ${frame}`,
      openOverlay: (frame: string) => `Open ${frame}`,
      closeOverlay: "Close overlay",
      back: "Back",
      scrollTo: (target: string) => `Scroll to ${target}`,
      setVariable: (variable: string) => `Set ${variable}`,
    };
    expect(actionLabel({ type: "navigate_to", frame_id: "Detail" }, actionLabels)).toBe("Go to Detail");
    expect(actionLabel({ type: "back" }, actionLabels)).toBe("Back");
    expect(actionLabel({ type: "set_variable", variable_id: "v1", value: { type: "boolean", value: true } }, actionLabels)).toBe(
      "Set v1",
    );
  });

  it("counts interactions and exposes stable option lists", () => {
    expect(interactionCount(null)).toBe(0);
    expect(interactionCount({ interactions: [] })).toBe(0);
    expect(interactionCount({ interactions: [{ id: "i1", trigger: { type: "click", delay_ms: 0 }, actions: [] }] })).toBe(1);
    expect(TRIGGER_TYPES).toEqual(["click", "hover", "press", "after_delay"]);
    expect(ACTION_TYPES).toContain("navigate_to");
  });
});
