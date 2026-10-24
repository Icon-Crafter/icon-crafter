import { describe, expect, it } from "vitest";

import {
  manifestDisplayName,
  manifestEntryCount,
  manifestStringList,
  parsePluginManifest,
} from "./pluginManifest";

describe("parsePluginManifest", () => {
  it("rejects empty input", () => {
    expect(parsePluginManifest("   ")).toEqual({ manifest: null, error: "empty" });
  });

  it("reports JSON syntax errors", () => {
    const result = parsePluginManifest("{ nope");
    expect(result.manifest).toBeNull();
    expect(result.error).not.toBeNull();
  });

  it("rejects non-object payloads", () => {
    expect(parsePluginManifest("[1, 2]").error).toBe("manifest must be a JSON object");
    expect(parsePluginManifest("\"plugin\"").error).toBe("manifest must be a JSON object");
  });

  it("parses an object manifest", () => {
    const result = parsePluginManifest('{"name":"Example","version":"0.1.0"}');
    expect(result.error).toBeNull();
    expect(result.manifest?.name).toBe("Example");
  });
});

describe("manifest projections", () => {
  const manifest = {
    name: "Example",
    permissions: ["ui.panel", { custom: true }],
    commands: [{ id: "a" }, { id: "b" }],
  };

  it("normalizes permission entries to strings", () => {
    expect(manifestStringList(manifest, "permissions")).toEqual(["ui.panel", '{"custom":true}']);
    expect(manifestStringList(manifest, "missing")).toEqual([]);
  });

  it("counts array entries and tolerates absent fields", () => {
    expect(manifestEntryCount(manifest, "commands")).toBe(2);
    expect(manifestEntryCount(manifest, "panels")).toBe(0);
  });

  it("prefers the first non-empty known display name", () => {
    expect(manifestDisplayName(manifest, "Fallback")).toBe("Example");
    expect(manifestDisplayName({ id: "com.example.plugin" }, "Fallback")).toBe("com.example.plugin");
    expect(manifestDisplayName({}, "Fallback")).toBe("Fallback");
  });
});
