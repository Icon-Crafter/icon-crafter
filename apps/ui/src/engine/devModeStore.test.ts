import { beforeEach, describe, expect, it } from "vitest";

import { CAPTURE_STORAGE_KEY, type RevisionCapture } from "./devMode";
import { useDevModeStore } from "./devModeStore";
import type { GeneratedCode, Handoff, InspectSnapshot, RevisionDiff } from "./contract";

const initial = useDevModeStore.getState();

function capture(id: string, revision: number): RevisionCapture {
  return {
    id,
    label: `Revision ${revision}`,
    revision,
    captured_at: "2026-10-24T00:00:00.000Z",
    snapshot: `{"revision":${revision}}`,
  };
}

function resetStore(): void {
  useDevModeStore.setState({
    ...initial,
    active: false,
    tab: "inspect",
    inspect: null,
    inspectLoading: false,
    inspectError: null,
    code: null,
    codeTarget: "css",
    codeLoading: false,
    codeError: null,
    handoff: null,
    handoffLoading: false,
    handoffError: null,
    captures: [],
    compareResult: null,
    compareResultCapture: null,
    compareLoading: false,
    compareError: null,
  });
}

beforeEach(() => {
  window.localStorage.clear();
  resetStore();
});

describe("devModeStore reducers", () => {
  it("switches tabs", () => {
    useDevModeStore.getState().setTab("code");
    expect(useDevModeStore.getState().tab).toBe("code");
    useDevModeStore.getState().setTab("compare");
    expect(useDevModeStore.getState().tab).toBe("compare");
  });

  it("toggles active and resets to inspect when disabled", async () => {
    await useDevModeStore.getState().setActive(true);
    expect(useDevModeStore.getState().active).toBe(true);
    expect(useDevModeStore.getState().inspectError).toBe("The engine is not connected yet.");

    useDevModeStore.getState().setTab("annotate");
    await useDevModeStore.getState().setActive(false);
    expect(useDevModeStore.getState().active).toBe(false);
    expect(useDevModeStore.getState().tab).toBe("inspect");
  });

  it("stores inspect snapshots and loading errors", () => {
    const snapshot: InspectSnapshot = { nodes: [], bounds: null, total_size: [0, 0], warnings: [] };
    useDevModeStore.getState().setInspectState({ inspect: snapshot, inspectLoading: false });
    expect(useDevModeStore.getState().inspect).toBe(snapshot);

    useDevModeStore.getState().setInspectState({ inspectError: "boom" });
    expect(useDevModeStore.getState().inspectError).toBe("boom");
  });

  it("records a connection error when refreshing without a pump", async () => {
    await useDevModeStore.getState().refreshInspect();
    expect(useDevModeStore.getState().inspectError).toBe("The engine is not connected yet.");
    expect(useDevModeStore.getState().inspectLoading).toBe(false);
  });

  it("stores generated code state", async () => {
    const code: GeneratedCode = {
      target: "css",
      language: "css",
      extension: "css",
      code: ".card { }",
      warnings: [],
      requires_manual_adaptation: true,
      node_count: 1,
    };
    useDevModeStore.getState().setCodeState({ code });
    expect(useDevModeStore.getState().code).toBe(code);

    await useDevModeStore.getState().generateCode();
    expect(useDevModeStore.getState().codeError).toBe("The engine is not connected yet.");
  });

  it("stores handoff state", () => {
    const handoff: Handoff = {
      revision: 4,
      status: "draft",
      annotations: [],
      marked_ready_by: null,
      marked_ready_at: null,
    };
    useDevModeStore.getState().setHandoffState({ handoff });
    expect(useDevModeStore.getState().handoff).toBe(handoff);
  });

  it("stores compare results", () => {
    const result: RevisionDiff = {
      added: [],
      removed: [],
      changed: [],
      counts: { added: 0, removed: 0, changed: 0 },
    };
    useDevModeStore.getState().setCompareState({ compareResult: result });
    expect(useDevModeStore.getState().compareResult).toBe(result);
  });
});

describe("devModeStore captures", () => {
  it("caps stored captures and persists them", () => {
    const captures = [1, 2, 3, 4, 5, 6].map((revision) => capture(`cap_${revision}`, revision));
    useDevModeStore.getState().setCaptures(captures);

    const stored = useDevModeStore.getState().captures;
    expect(stored).toHaveLength(5);
    expect(stored[0]?.id).toBe("cap_1");

    const raw = window.localStorage.getItem(CAPTURE_STORAGE_KEY);
    expect(raw).toBeTruthy();
    const parsed = JSON.parse(raw ?? "[]") as RevisionCapture[];
    expect(parsed).toHaveLength(5);
    expect(parsed[4]?.id).toBe("cap_5");
  });

  it("clears captures and persisted data", () => {
    useDevModeStore.getState().setCaptures([capture("cap_1", 1)]);
    useDevModeStore.getState().clearCaptures();

    expect(useDevModeStore.getState().captures).toEqual([]);
    expect(window.localStorage.getItem(CAPTURE_STORAGE_KEY)).toBeNull();
    expect(useDevModeStore.getState().compareResult).toBeNull();
  });

  it("removes a capture by id", () => {
    useDevModeStore.getState().setCaptures([capture("a", 1), capture("b", 2)]);
    useDevModeStore.getState().removeCapture("a");
    expect(useDevModeStore.getState().captures.map((item) => item.id)).toEqual(["b"]);
  });
});
