import { describe, expect, it } from "vitest";
import {
  clamp,
  formatBytes,
  formatCount,
  formatLength,
  formatZoom,
  normalizeRectValues,
  parseNumber,
  prettyNumber,
  roundTo,
  stepValue,
} from "./number";

describe("clamp", () => {
  it("bounds finite values", () => {
    expect(clamp(5, 0, 1)).toBe(1);
    expect(clamp(-4, 0, 1)).toBe(0);
    expect(clamp(0.4, 0, 1)).toBe(0.4);
  });

  it("returns the lower bound for NaN so inputs never break a field", () => {
    expect(clamp(Number.NaN, 2, 8)).toBe(2);
  });
});

describe("roundTo / prettyNumber", () => {
  it("removes floating point noise", () => {
    expect(roundTo(0.1 + 0.2, 2)).toBe(0.3);
    expect(roundTo(1.23456, 3)).toBe(1.235);
  });

  it("prints compact numbers and never shows negative zero", () => {
    expect(prettyNumber(1.5)).toBe("1.5");
    expect(prettyNumber(2)).toBe("2");
    expect(prettyNumber(-0)).toBe("0");
    expect(prettyNumber(37.5, 1)).toBe("37.5");
    expect(prettyNumber(1 / 3, 2)).toBe("0.33");
  });
});

describe("parseNumber", () => {
  it("accepts signed, decimal and exponent values with whitespace", () => {
    expect(parseNumber("42")).toBe(42);
    expect(parseNumber("  -3.5 ")).toBe(-3.5);
    expect(parseNumber("1e3")).toBe(1000);
    expect(parseNumber(".5")).toBe(0.5);
  });

  it("rejects partial or empty input so the field keeps its previous value", () => {
    expect(parseNumber("")).toBeNull();
    expect(parseNumber("-")).toBeNull();
    expect(parseNumber(".")).toBeNull();
    expect(parseNumber("1.2.3")).toBeNull();
    expect(parseNumber("abc")).toBeNull();
    expect(parseNumber("Infinity")).toBeNull();
  });
});

describe("display formatting", () => {
  it("formats zoom as a percentage with integer values at or above 100%", () => {
    expect(formatZoom(1)).toBe("100%");
    expect(formatZoom(0.375)).toBe("37.5%");
    expect(formatZoom(0.01)).toBe("1%");
    expect(formatZoom(64)).toBe("6400%");
  });

  it("formats design lengths", () => {
    expect(formatLength(24)).toBe("24");
    expect(formatLength(24.5)).toBe("24.5");
  });

  it("formats byte sizes with sensible precision", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(15 * 1024)).toBe("15 KB");
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });

  it("formats counts with grouping", () => {
    expect(formatCount(1234567)).toBe("1,234,567");
  });
});

describe("stepValue", () => {
  it("steps by the modifier factor", () => {
    expect(stepValue(10, 1)).toBe(11);
    expect(stepValue(10, -1, 1, true)).toBe(0);
    expect(stepValue(10, 1, 1, false, true)).toBe(10.1);
    expect(stepValue(0, -1, 1, false, true)).toBe(-0.1);
  });
});

describe("normalizeRectValues", () => {
  it("normalizes negative width and height", () => {
    expect(normalizeRectValues(10, 20, -5, -8)).toEqual({ x: 5, y: 12, w: 5, h: 8 });
  });

  it("leaves positive rectangles unchanged", () => {
    expect(normalizeRectValues(1, 2, 3, 4)).toEqual({ x: 1, y: 2, w: 3, h: 4 });
  });
});
