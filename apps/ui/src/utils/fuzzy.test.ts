import { describe, expect, it } from "vitest";
import { fuzzyMatch, fuzzyScore, fuzzySort, highlightRuns } from "./fuzzy";

describe("fuzzyMatch", () => {
  it("matches everything for an empty query", () => {
    expect(fuzzyMatch("", "Layer")).toEqual({ score: 0, indices: [] });
  });

  it("returns null when the query is not a subsequence", () => {
    expect(fuzzyMatch("xyz", "Layer")).toBeNull();
  });

  it("matches case-insensitively and reports character indices", () => {
    const match = fuzzyMatch("lt", "Layer Tree");
    expect(match).not.toBeNull();
    expect(match?.indices).toEqual([0, 6]);
    expect(fuzzyMatch("LA", "layer")).not.toBeNull();
  });

  it("rewards a prefix match over a match in the middle", () => {
    const prefix = fuzzyMatch("la", "Layer");
    const middle = fuzzyMatch("ye", "Layer");
    expect(prefix).not.toBeNull();
    expect(middle).not.toBeNull();
    expect(prefix!.score).toBeGreaterThan(middle!.score);
  });
});

describe("fuzzyScore", () => {
  it("mirrors fuzzyMatch and is null for misses", () => {
    expect(fuzzyScore("undo", "Undo")).toBe(fuzzyMatch("undo", "Undo")?.score);
    expect(fuzzyScore("zzz", "Undo")).toBeNull();
  });
});

describe("fuzzySort", () => {
  it("preserves input order for an empty query", () => {
    const items = ["b", "A", "c"];
    expect(fuzzySort(items, "", (item) => item)).toEqual(items);
  });

  it("ranks the best title match first", () => {
    const items = ["Rotate", "Rectangle", "Delete"];
    const sorted = fuzzySort(items, "rect", (item) => item);
    expect(sorted).toEqual(["Rectangle"]);
  });
});

describe("highlightRuns", () => {
  it("splits text into matched and unmatched runs covering the whole string", () => {
    const runs = highlightRuns("Layer", [0, 2]);
    expect(runs.map((run) => run.text).join("")).toBe("Layer");
    expect(runs.filter((run) => run.match)).toHaveLength(2);
    expect(runs[0]).toEqual({ text: "L", match: true });
  });

  it("merges adjacent indices into one run", () => {
    expect(highlightRuns("abc", [0, 1, 2])).toEqual([{ text: "abc", match: true }]);
  });
});
