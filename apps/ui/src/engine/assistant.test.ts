import { describe, expect, it } from "vitest";

import type { Capabilities } from "./contract";
import { DEFAULT_CAPABILITIES } from "./contract";
import {
  aiSourceLabel,
  aiStepsLabel,
  assistantAvailability,
  parseSettingInput,
  pluginAvailability,
} from "./assistant";

function capabilities(overrides: Partial<Capabilities>): Capabilities {
  return { ...DEFAULT_CAPABILITIES, ...overrides };
}

describe("assistantAvailability", () => {
  it("is disabled when the engine reports no AI capability", () => {
    expect(assistantAvailability(capabilities({ ai: false }), "ready", true)).toBe("disabled");
  });

  it("is offline until a document is ready", () => {
    expect(assistantAvailability(capabilities({ ai: true }), "connecting", true)).toBe("offline");
    expect(assistantAvailability(capabilities({ ai: true }), "ready", false)).toBe("offline");
  });

  it("is ready with capability, connection and document", () => {
    expect(assistantAvailability(capabilities({ ai: true }), "ready", true)).toBe("ready");
  });
});

describe("pluginAvailability", () => {
  it("is disabled when plugins are not enabled", () => {
    expect(pluginAvailability(capabilities({ plugins: false }), "ready", true)).toBe("disabled");
  });

  it("is ready only with a live document", () => {
    expect(pluginAvailability(capabilities({ plugins: true }), "unavailable", true)).toBe("offline");
    expect(pluginAvailability(capabilities({ plugins: true }), "ready", true)).toBe("ready");
  });
});

describe("aiSourceLabel", () => {
  it("maps known engine sources and passes through unknown ones", () => {
    expect(aiSourceLabel("offline")).toBe("Local assistant (offline)");
    expect(aiSourceLabel("remote")).toBe("Remote model");
    expect(aiSourceLabel("custom-provider")).toBe("custom-provider");
  });
});

describe("aiStepsLabel", () => {
  it("uses singular for one step and plural otherwise", () => {
    expect(aiStepsLabel({ steps: 1 })).toBe("1 step");
    expect(aiStepsLabel({ steps: 4 })).toBe("4 steps");
  });
});

describe("parseSettingInput", () => {
  it("coerces text to the current setting type", () => {
    expect(parseSettingInput("42", 0)).toBe(42);
    expect(parseSettingInput("nope", 7)).toBe(7);
    expect(parseSettingInput("true", false)).toBe(true);
    expect(parseSettingInput("off", true)).toBe(false);
    expect(parseSettingInput("  hello ", "old")).toBe("  hello ");
  });
});
