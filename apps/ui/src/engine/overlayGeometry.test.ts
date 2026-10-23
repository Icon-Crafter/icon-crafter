import { describe, expect, it } from "vitest";
import type { Handle, SelectionBox, Viewport } from "./contract";
import { EMPTY_OVERLAY } from "./contract";
import {
  cursorFor,
  findHandleAtPoint,
  floatingToolbarPosition,
  handleScreenPoint,
  marqueeScreenRect,
  niceRulerStep,
  rulerTicks,
  screenCorners,
  selectionScreenBounds,
  worldPointToScreen,
} from "./overlayGeometry";

const viewport: Viewport = { x: 10, y: 20, zoom: 2, width: 800, height: 600, dpr: 1 };

const box: SelectionBox = {
  id: "n1",
  corners: [
    { x: 0, y: 0 },
    { x: 100, y: 0 },
    { x: 100, y: 100 },
    { x: 0, y: 100 },
  ],
};

const handle: Handle = { kind: "corner", pos: { x: 0, y: 0 }, cursor: "nwse-resize" };

describe("world to screen conversion", () => {
  it("applies the viewport origin and zoom", () => {
    expect(worldPointToScreen({ x: 0, y: 0 }, viewport)).toEqual({ x: -20, y: -40 });
    expect(worldPointToScreen({ x: 110, y: 120 }, viewport)).toEqual({ x: 200, y: 200 });
  });

  it("projects selection corners and their bounds", () => {
    expect(screenCorners(box, viewport)).toEqual([
      { x: -20, y: -40 },
      { x: 180, y: -40 },
      { x: 180, y: 160 },
      { x: -20, y: 160 },
    ]);
    expect(selectionScreenBounds([box], viewport)).toEqual({ x: -20, y: -40, w: 200, h: 200 });
  });

  it("projects a world marquee into a screen rect", () => {
    expect(marqueeScreenRect(null, viewport)).toBeNull();
    expect(marqueeScreenRect({ x: 10, y: 20, w: 30, h: 40 }, viewport)).toEqual({
      x: 0,
      y: 0,
      w: 60,
      h: 80,
    });
  });
});

describe("handles", () => {
  it("positions handles and hit-tests in screen space", () => {
    expect(handleScreenPoint(handle, viewport)).toEqual({ x: -20, y: -40 });
    expect(findHandleAtPoint([handle], { x: -20, y: -40 }, viewport)).toBe(handle);
    expect(findHandleAtPoint([handle], { x: 500, y: 500 }, viewport)).toBeNull();
  });
});

describe("cursorFor", () => {
  const base = { overlay: EMPTY_OVERLAY, screenPoint: { x: 0, y: 0 }, viewport };

  it("prioritizes text editing and the hand tool", () => {
    expect(cursorFor({ ...base, tool: "move", isTextEditing: true })).toBe("text");
    expect(cursorFor({ ...base, tool: "hand" })).toBe("grab");
  });

  it("uses a crosshair for creation tools", () => {
    expect(cursorFor({ ...base, tool: "rectangle" })).toBe("crosshair");
  });

  it("shows the resize cursor over a handle", () => {
    expect(
      cursorFor({
        ...base,
        tool: "move",
        overlay: { ...EMPTY_OVERLAY, handles: [handle] },
        screenPoint: { x: -20, y: -40 },
      }),
    ).toBe("nwse-resize");
  });

  it("moves over the hovered layer and defaults otherwise", () => {
    expect(cursorFor({ ...base, tool: "move", overlay: { ...EMPTY_OVERLAY, hover: box } })).toBe("move");
    expect(cursorFor({ ...base, tool: "move" })).toBe("default");
  });
});

describe("rulers", () => {
  it("chooses the smallest nice step that keeps ticks readable", () => {
    expect(niceRulerStep(1)).toBe(100);
    expect(niceRulerStep(4)).toBe(20);
    expect(niceRulerStep(0.5)).toBe(200);
    expect(niceRulerStep(100)).toBe(1);
  });

  it("centers ticks on the viewport origin", () => {
    const ruler = rulerTicks(0, 400, 2);
    expect(ruler.step).toBe(50);
    expect(ruler.ticks).toEqual([-100, -50, 0, 50, 100]);
    for (const tick of ruler.ticks) expect(Math.abs(tick % ruler.step)).toBe(0);
  });
});

describe("floatingToolbarPosition", () => {
  const size = { width: 200, height: 36 };

  it("centers above the canvas when there is no selection", () => {
    expect(floatingToolbarPosition(null, viewport, { width: 300, height: 36 })).toEqual({
      x: 250,
      y: 540,
      placement: "center",
    });
  });

  it("prefers above the selection and flips below when clipped", () => {
    expect(floatingToolbarPosition({ x: 100, y: 10, w: 100, h: 40 }, viewport, size)).toEqual({
      x: 50,
      y: 62,
      placement: "below",
    });
    expect(floatingToolbarPosition({ x: 100, y: 400, w: 100, h: 40 }, viewport, size)).toEqual({
      x: 50,
      y: 352,
      placement: "above",
    });
  });
});
