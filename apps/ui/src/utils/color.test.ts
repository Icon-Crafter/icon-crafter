import { describe, expect, it } from "vitest";
import type { Color } from "../engine/contract";
import {
  clamp01,
  colorToCss,
  colorToHex,
  colorToHsv,
  colorsEqual,
  hexToColor,
  hsvToColor,
  normalizeColor,
  readableInk,
  withAlpha,
} from "./color";

const coral: Color = { r: 1, g: 107 / 255, b: 74 / 255, a: 1 };

describe("hexToColor", () => {
  it("parses 6-digit, 3-digit and 8-digit hex values", () => {
    const fromSix = hexToColor("#ff6b4a");
    expect(fromSix).not.toBeNull();
    expect(fromSix?.r).toBeCloseTo(1, 5);
    expect(fromSix?.g).toBeCloseTo(107 / 255, 5);
    expect(fromSix?.b).toBeCloseTo(74 / 255, 5);
    expect(fromSix?.a).toBe(1);

    expect(hexToColor("fff")?.r).toBeCloseTo(1, 5);
    expect(hexToColor("#00000000")?.a).toBe(0);
    expect(hexToColor("ff6b4a")?.g).toBeCloseTo(107 / 255, 5);
  });

  it("rejects malformed input", () => {
    expect(hexToColor("#12")).toBeNull();
    expect(hexToColor("#gggggg")).toBeNull();
    expect(hexToColor("")).toBeNull();
  });
});

describe("colorToHex", () => {
  it("round-trips a color without alpha", () => {
    expect(colorToHex(coral)).toBe("#ff6b4a");
  });

  it("appends alpha only when requested and not fully opaque", () => {
    expect(colorToHex(withAlpha(coral, 0.5), true)).toBe("#ff6b4a80");
    expect(colorToHex(coral, true)).toBe("#ff6b4a");
  });
});

describe("CSS and normalization", () => {
  it("serializes rgba with rounded channels", () => {
    expect(colorToCss({ r: 1, g: 0.5, b: 0, a: 0.25 })).toBe("rgba(255, 128, 0, 0.25)");
  });

  it("clamps channels and alpha", () => {
    expect(normalizeColor({ r: 1.4, g: -0.2, b: 0.5, a: 2 })).toEqual({
      r: 1,
      g: 0,
      b: 0.5,
      a: 1,
    });
    expect(clamp01(-3)).toBe(0);
    expect(clamp01(7)).toBe(1);
  });
});

describe("HSV conversion", () => {
  it("round-trips through HSV", () => {
    const back = hsvToColor(colorToHsv(coral));
    expect(colorsEqual(back, coral)).toBe(true);
  });
});

describe("withAlpha / colorsEqual", () => {
  it("clamps alpha overrides", () => {
    expect(withAlpha(coral, 2).a).toBe(1);
    expect(withAlpha(coral, -1).a).toBe(0);
  });

  it("compares with a small tolerance", () => {
    expect(colorsEqual(coral, { ...coral, r: coral.r + 1e-5 })).toBe(true);
    expect(colorsEqual(coral, { ...coral, r: coral.r + 1e-3 })).toBe(false);
  });
});

describe("readableInk", () => {
  it("picks white text on dark backgrounds and near-black on light ones", () => {
    expect(readableInk({ r: 0, g: 0, b: 0, a: 1 })).toBe("#ffffff");
    expect(readableInk({ r: 1, g: 1, b: 1, a: 1 })).toBe("#12151b");
  });
});
