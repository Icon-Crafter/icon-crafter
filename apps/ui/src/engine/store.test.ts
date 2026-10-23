import { beforeEach, describe, expect, it } from "vitest";
import type { DocMeta, DocSummary, EngineState } from "./contract";
import { initialEngineState } from "./contract";
import { appStore } from "./store";
import { defaultBindings, loadBindings } from "./shortcuts";

const get = () => appStore.getState();

beforeEach(() => {
  localStorage.clear();
  appStore.setState({
    toasts: [],
    dialog: null,
    unsavedIntent: null,
    busy: false,
    leftTab: "layers",
    leftCollapsed: false,
    rightCollapsed: false,
    paletteOpen: false,
    recentCommands: [],
    hoveredNodeId: null,
    status: "ready",
  });
});

describe("dialog state", () => {
  it("opens and closes dialogs with their intent", () => {
    get().openDialog("export");
    expect(get().dialog).toBe("export");
    expect(get().unsavedIntent).toBeNull();

    get().openDialog("unsaved", "close");
    expect(get().dialog).toBe("unsaved");
    expect(get().unsavedIntent).toBe("close");

    get().setBusy(true);
    get().closeDialog();
    expect(get().dialog).toBeNull();
    expect(get().unsavedIntent).toBeNull();
    expect(get().busy).toBe(false);
  });
});

describe("layout state", () => {
  it("switches left tabs and expands the panel", () => {
    get().toggleLeftCollapsed();
    expect(get().leftCollapsed).toBe(true);
    get().setLeftTab("pages");
    expect(get().leftTab).toBe("pages");
    expect(get().leftCollapsed).toBe(false);
  });
});

describe("toasts", () => {
  it("keeps the five most recent toasts and dismisses by id", () => {
    for (let index = 0; index < 6; index += 1) {
      get().pushToast({ kind: "info", title: `Toast ${index}` });
    }
    expect(get().toasts).toHaveLength(5);
    expect(get().toasts[0]?.title).toBe("Toast 1");
    expect(get().toasts[4]?.title).toBe("Toast 5");

    const firstId = get().toasts[0]?.id;
    expect(firstId).toBeDefined();
    get().dismissToast(firstId!);
    expect(get().toasts).toHaveLength(4);
  });
});

describe("recent commands", () => {
  it("deduplicates, orders by recency and caps the list", () => {
    get().recordCommand("undo");
    get().recordCommand("redo");
    get().recordCommand("undo");
    expect(get().recentCommands).toEqual(["undo", "redo"]);

    for (let index = 0; index < 10; index += 1) {
      get().recordCommand(`action.${index}`);
    }
    expect(get().recentCommands).toHaveLength(8);
    expect(get().recentCommands[0]).toBe("action.9");
    expect(get().recentCommands[7]).toBe("action.2");
  });
});

describe("shortcut settings", () => {
  it("persists rebinding through the shortcut storage", () => {
    get().setShortcuts({ ...defaultBindings(), "tool.move": "G" });
    expect(get().shortcuts["tool.move"]).toBe("G");
    expect(loadBindings()["tool.move"]).toBe("G");
  });
});

describe("engine ingestion", () => {
  const doc: DocMeta = { id: "doc1", name: "Untitled", path: null, schema: 1, revision: 3 };
  const summary: DocSummary = {
    pages: [{ id: "p1", name: "Page 1" }],
    active_page: "p1",
    nodes: [],
    styles: { paints: [], texts: [], effects: [] },
    components: [],
    variables: [],
  };

  const stateWithErrors = (errors: EngineState["errors"]): EngineState => ({
    ...initialEngineState(),
    doc,
    revision: 3,
    summary,
    errors,
  });

  it("stores the snapshot and surfaces one-shot engine errors as toasts", () => {
    get().ingest(
      stateWithErrors([
        { code: "unsupported", message: "Recovered prototypes are not available", severity: "warning" },
      ]),
    );
    expect(get().status).toBe("ready");
    expect(get().engine.doc?.name).toBe("Untitled");
    expect(get().toasts).toHaveLength(1);
    expect(get().toasts[0]?.kind).toBe("warning");
    expect(get().toasts[0]?.title).toBe("Not available yet");

    get().ingest(
      stateWithErrors([{ code: "validation", message: "Cannot delete the last page", severity: "error" }]),
    );
    expect(get().toasts).toHaveLength(2);
    expect(get().toasts[1]?.kind).toBe("error");
    expect(get().toasts[1]?.title).toBe("Cannot delete the last page");
  });

  it("deduplicates the same error arriving in consecutive snapshots", () => {
    const errors: EngineState["errors"] = [
      { code: "unsupported", message: "Prototype playback is not available", severity: "warning" },
    ];
    get().ingest(stateWithErrors(errors));
    get().ingest(stateWithErrors(errors));
    expect(get().toasts).toHaveLength(1);
  });

  it("tracks the hovered row without touching engine state", () => {
    get().setHoveredNodeId("node-1");
    expect(get().hoveredNodeId).toBe("node-1");
    get().setHoveredNodeId("node-1");
    expect(get().hoveredNodeId).toBe("node-1");
    get().setHoveredNodeId(null);
    expect(get().hoveredNodeId).toBeNull();
  });
});
