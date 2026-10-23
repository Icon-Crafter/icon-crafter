import { beforeEach, describe, expect, it } from "vitest";
import type { KeyboardEventLike } from "./shortcuts";
import {
  bindingConflicts,
  comboMatchesEvent,
  defaultBindings,
  formatBindingForDisplay,
  isEditableTarget,
  loadBindings,
  parseBinding,
  resetBindings,
  resolveShortcut,
  saveBindings,
  SHORTCUT_STORAGE_KEY,
  toolForShortcut,
} from "./shortcuts";

function keyEvent(patch: Partial<KeyboardEventLike> & { key: string }): KeyboardEventLike {
  const base: KeyboardEventLike = {
    key: "",
    code: "",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
  };
  return { ...base, ...patch };
}

beforeEach(() => {
  localStorage.clear();
});

describe("parseBinding", () => {
  it("maps Mod to Ctrl on Windows and Cmd on macOS", () => {
    const pc = parseBinding("Mod+Shift+Z", false);
    expect(pc).not.toBeNull();
    expect(pc).toMatchObject({ key: "Z", code: "", ctrl: true, meta: false, alt: false, shift: true });

    const mac = parseBinding("Mod+Shift+Z", true);
    expect(mac).toMatchObject({ key: "Z", ctrl: false, meta: true, shift: true });
  });

  it("maps digits to key codes so shifted symbols still match", () => {
    expect(parseBinding("Shift+0", false)).toMatchObject({ code: "Digit0", key: "", shift: true });
  });

  it("rejects unknown modifier tokens", () => {
    expect(parseBinding("Nope+Q", false)).toBeNull();
  });
});

describe("comboMatchesEvent", () => {
  it("requires an exact modifier match", () => {
    const combo = parseBinding("Mod+Z", false);
    expect(combo).not.toBeNull();
    expect(comboMatchesEvent(combo!, keyEvent({ key: "z", ctrlKey: true }))).toBe(true);
    expect(comboMatchesEvent(combo!, keyEvent({ key: "z", ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(comboMatchesEvent(combo!, keyEvent({ key: "z" }))).toBe(false);
  });
});

describe("resolveShortcut", () => {
  const bindings = defaultBindings();

  it("resolves undo, redo and delete", () => {
    expect(resolveShortcut(keyEvent({ key: "z", ctrlKey: true }), bindings, false)).toBe("edit.undo");
    expect(resolveShortcut(keyEvent({ key: "z", ctrlKey: true, shiftKey: true }), bindings, false)).toBe(
      "edit.redo",
    );
    expect(resolveShortcut(keyEvent({ key: "Delete" }), bindings, false)).toBe("edit.delete");
  });

  it("resolves single-letter tool shortcuts without modifiers", () => {
    expect(resolveShortcut(keyEvent({ key: "v" }), bindings, false)).toBe("tool.move");
    expect(resolveShortcut(keyEvent({ key: "R" }), bindings, false)).toBe("tool.rectangle");
  });

  it("resolves shifted digit bindings via key code", () => {
    expect(
      resolveShortcut(keyEvent({ key: ")", code: "Digit0", shiftKey: true }), bindings, false),
    ).toBe("view.zoom100");
  });

  it("uses Cmd for Mod on macOS", () => {
    expect(resolveShortcut(keyEvent({ key: "z", metaKey: true }), bindings, true)).toBe("edit.undo");
  });

  it("returns null for unbound combinations", () => {
    expect(resolveShortcut(keyEvent({ key: "q", ctrlKey: true, altKey: true }), bindings, false)).toBeNull();
  });
});

describe("display formatting", () => {
  it("prints platform-appropriate shortcut labels", () => {
    expect(formatBindingForDisplay("Mod+Shift+Z", false)).toBe("Ctrl+Shift+Z");
    expect(formatBindingForDisplay("Mod+Shift+Z", true)).toBe("⌘⇧Z");
    expect(formatBindingForDisplay(null)).toBe("");
  });
});

describe("conflicts", () => {
  it("reports actions that share a binding signature", () => {
    const bindings = { ...defaultBindings(), "edit.undo": "Mod+K", "app.commandPalette": "Mod+K" };
    const conflicts = bindingConflicts(bindings);
    expect(conflicts.size).toBe(1);
    const ids = [...conflicts.values()][0] ?? [];
    expect(ids).toContain("edit.undo");
    expect(ids).toContain("app.commandPalette");
  });
});

describe("persistence", () => {
  it("round-trips bindings through localStorage and can reset them", () => {
    const custom = { ...defaultBindings(), "tool.move": "G" };
    saveBindings(custom);
    expect(localStorage.getItem(SHORTCUT_STORAGE_KEY)).toBeTruthy();
    expect(loadBindings()["tool.move"]).toBe("G");

    const defaults = resetBindings();
    expect(localStorage.getItem(SHORTCUT_STORAGE_KEY)).toBeNull();
    expect(defaults["tool.move"]).toBe("V");
  });
});

describe("helpers", () => {
  it("detects editable targets so shortcuts do not steal typing", () => {
    expect(isEditableTarget(document.createElement("input"))).toBe(true);
    expect(isEditableTarget(document.createElement("textarea"))).toBe(true);
    expect(isEditableTarget(document.createElement("div"))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });

  it("maps tool actions to tool ids", () => {
    expect(toolForShortcut("tool.rectangle")).toBe("rectangle");
    expect(toolForShortcut("edit.undo")).toBeNull();
  });
});
