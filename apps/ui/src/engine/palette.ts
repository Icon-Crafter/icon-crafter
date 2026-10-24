/**
 * Command palette catalogue.
 *
 * The catalogue is data, not UI: every row carries its group, searchable text
 * and a `run` closure. Building it from the current engine snapshot keeps the
 * palette in lockstep with what the engine reports (layers, styles, undo state).
 */
import type { Capabilities, DocSummary, EngineState, TreeNode } from "./contract";
import { DEFAULT_CAPABILITIES } from "./contract";
import type { BindingMap, ShortcutActionId } from "./shortcuts";
import { en } from "../i18n/en";
import { fuzzyMatch } from "../utils/fuzzy";
import { useAppStore } from "./store";
import * as commands from "./commands";

export type PaletteGroup = "actions" | "tools" | "layers" | "styles" | "recent";

export const PALETTE_GROUP_ORDER: readonly PaletteGroup[] = [
  "actions",
  "tools",
  "layers",
  "styles",
  "recent",
];

export interface PaletteItem {
  id: string;
  group: PaletteGroup;
  title: string;
  subtitle?: string;
  keywords: string[];
  shortcut?: string | null;
  disabled?: boolean;
  run: () => void;
}

export function groupLabel(group: PaletteGroup): string {
  switch (group) {
    case "actions":
      return en.palette.groupActions;
    case "tools":
      return en.palette.groupTools;
    case "layers":
      return en.palette.groupLayers;
    case "styles":
      return en.palette.groupStyles;
    case "recent":
      return en.palette.groupRecent;
  }
}

function lazyCommands() {
  return Promise.resolve(commands);
}

function commandRunner(name: string, args?: unknown) {
  return () => {
    void lazyCommands().then((mod) => mod.runCommand(name as never, args as never));
  };
}

function shortcutFor(shortcuts: BindingMap, id: string): string | null {
  return shortcuts[id as ShortcutActionId] ?? null;
}

export function buildActionItems(
  engine: EngineState,
  shortcuts: BindingMap,
  capabilities: Capabilities = DEFAULT_CAPABILITIES,
): PaletteItem[] {
  const items: PaletteItem[] = [];
  const push = (
    id: string,
    title: string,
    run: () => void,
    options: { shortcutId?: string; disabled?: boolean; keywords?: string[]; subtitle?: string } = {},
  ) => {
    items.push({
      id: `action.${id}`,
      group: "actions",
      title,
      subtitle: options.subtitle,
      keywords: options.keywords ?? [],
      shortcut: options.shortcutId ? shortcutFor(shortcuts, options.shortcutId) : null,
      disabled: options.disabled,
      run,
    });
  };

  push("new", en.menu.newDocument, () => void lazyCommands().then((m) => m.requestNewDocument()), {
    shortcutId: "file.new",
  });
  push("open", en.menu.openDocument, () => void lazyCommands().then((m) => m.requestOpenDocument()), {
    shortcutId: "file.open",
  });
  push("save", en.menu.save, () => void lazyCommands().then((m) => m.requestSave()), {
    shortcutId: "file.save",
    disabled: !engine.doc,
  });
  push("save-as", en.menu.saveAs, () => void lazyCommands().then((m) => m.openSaveAsDialog()), {
    shortcutId: "file.saveAs",
    disabled: !engine.doc,
  });
  push("export", en.menu.export, () => useAppStore.getState().openDialog("export"), {
    shortcutId: "file.export",
    disabled: !engine.doc,
  });
  push("close", en.menu.closeDocument, () => void lazyCommands().then((m) => m.requestCloseDocument()), {
    shortcutId: "file.close",
    disabled: !engine.doc,
  });
  push("import-svg", en.menu.importSvg, () => void lazyCommands().then((m) => m.requestImportSvg()), {
    keywords: ["import", "svg", "vector"],
    disabled: capabilities.import_svg !== true,
  });
  push("place-image", en.menu.placeImage, () => void lazyCommands().then((m) => m.requestPlaceImage()), {
    keywords: ["image", "import", "photo", "bitmap"],
    disabled: capabilities.import_image !== true,
  });
  push("ai-assistant", en.toolbar.ai, () => void lazyCommands().then((m) => m.toggleAiPanel()), {
    keywords: ["ai", "assistant", "generate", "layout", "proposal"],
    disabled: capabilities.ai !== true || !engine.doc,
  });
  push("plugins", en.toolbar.plugins, () => void lazyCommands().then((m) => m.togglePluginsPanel()), {
    keywords: ["plugin", "extension", "install", "manifest"],
    disabled: capabilities.plugins !== true || !engine.doc,
  });

  push("undo", en.menu.undo, commandRunner("undo"), {
    shortcutId: "edit.undo",
    disabled: !engine.can_undo,
  });
  push("redo", en.menu.redo, commandRunner("redo"), {
    shortcutId: "edit.redo",
    disabled: !engine.can_redo,
  });
  push("cut", en.menu.cut, commandRunner("cut"), {
    shortcutId: "edit.cut",
    disabled: engine.selection.length === 0,
  });
  push("copy", en.menu.copy, commandRunner("copy"), {
    shortcutId: "edit.copy",
    disabled: engine.selection.length === 0,
  });
  push("paste", en.menu.paste, commandRunner("paste"), { shortcutId: "edit.paste" });
  push("duplicate", en.menu.duplicate, commandRunner("duplicate"), {
    shortcutId: "edit.duplicate",
    disabled: engine.selection.length === 0,
  });
  push("delete", en.menu.delete, commandRunner("delete"), {
    shortcutId: "edit.delete",
    disabled: engine.selection.length === 0,
  });
  push("select-all", en.menu.selectAll, commandRunner("select_all"), { shortcutId: "edit.selectAll" });
  push("deselect", en.menu.deselect, commandRunner("select_none"), {
    shortcutId: "edit.deselect",
    disabled: engine.selection.length === 0,
  });
  push("group", en.menu.group, commandRunner("group"), {
    shortcutId: "edit.group",
    disabled: engine.selection.length < 2,
  });
  push("ungroup", en.menu.ungroup, commandRunner("ungroup"), {
    shortcutId: "edit.ungroup",
    disabled: engine.selection.length === 0,
  });
  push("create-component", "Create component", commandRunner("create_component"), {
    shortcutId: "edit.createComponent",
    disabled: engine.selection.length === 0,
  });
  push("bring-to-front", en.menu.bringToFront, commandRunner("bring_to_front"), {
    shortcutId: "edit.bringToFront",
    disabled: engine.selection.length === 0,
  });
  push("bring-forward", en.menu.bringForward, commandRunner("bring_forward"), {
    shortcutId: "edit.bringForward",
    disabled: engine.selection.length === 0,
  });
  push("send-backward", en.menu.sendBackward, commandRunner("send_backward"), {
    shortcutId: "edit.sendBackward",
    disabled: engine.selection.length === 0,
  });
  push("send-to-back", en.menu.sendToBack, commandRunner("send_to_back"), {
    shortcutId: "edit.sendToBack",
    disabled: engine.selection.length === 0,
  });
  push("zoom-in", en.menu.zoomIn, commandRunner("zoom_in"), { shortcutId: "view.zoomIn" });
  push("zoom-out", en.menu.zoomOut, commandRunner("zoom_out"), { shortcutId: "view.zoomOut" });
  push("zoom-100", en.menu.zoom100, commandRunner("zoom_100"), { shortcutId: "view.zoom100" });
  push("zoom-fit", en.menu.zoomFit, commandRunner("zoom_fit"), { shortcutId: "view.zoomFit" });
  push("zoom-selection", en.menu.zoomSelection, commandRunner("zoom_selection"), {
    shortcutId: "view.zoomSelection",
    disabled: engine.selection.length === 0,
  });
  return items;
}

export function buildToolItems(shortcuts: BindingMap): PaletteItem[] {
  const tools = [
    "move",
    "hand",
    "frame",
    "slice",
    "rectangle",
    "ellipse",
    "line",
    "polygon",
    "star",
    "pen",
    "node",
    "text",
    "image",
    "comment",
    "connector",
  ] as const;
  return tools.map((tool) => ({
    id: `tool.${tool}`,
    group: "tools" as const,
    title: en.tools[tool],
    keywords: ["tool", tool],
    shortcut: shortcutFor(shortcuts, `tool.${tool}`),
    run:
      tool === "image"
        ? () => void lazyCommands().then((m) => m.requestPlaceImage())
        : () => void lazyCommands().then((m) => m.runTool(tool)),
  }));
}

export function buildLayerItems(
  summary: DocSummary | null,
  selected: readonly string[],
  layerKindLabel: (kind: TreeNode["kind"]) => string,
): PaletteItem[] {
  if (!summary) return [];
  const selectedSet = new Set(selected);
  return summary.nodes.map((node) => ({
    id: `layer.${node.id}`,
    group: "layers" as const,
    title: node.name || en.layers.typeUnknown,
    subtitle: layerKindLabel(node.kind),
    keywords: ["layer", node.kind, node.name],
    disabled: selectedSet.has(node.id),
    run: () => {
      void lazyCommands().then((m) =>
        m.runCommand("select_nodes", { ids: [node.id], additive: true }),
      );
    },
  }));
}

export function buildStyleItems(summary: DocSummary | null): PaletteItem[] {
  if (!summary) return [];
  const items: PaletteItem[] = [];
  const push = (group: "paints" | "texts" | "effects", entry: { id: string; name: string }) => {
    items.push({
      id: `style.${group}.${entry.id}`,
      group: "styles",
      title: entry.name,
      subtitle:
        group === "paints"
          ? en.assets.colors
          : group === "texts"
            ? en.assets.textStyles
            : en.assets.effects,
      keywords: ["style", group, entry.name],
      run: () => {
        useAppStore.getState().pushToast({
          kind: "info",
          title: en.assets.applyHint,
          message: en.toasts.unsupportedBody,
        });
      },
    });
  };
  for (const entry of summary.styles.paints) push("paints", entry);
  for (const entry of summary.styles.texts) push("texts", entry);
  for (const entry of summary.styles.effects) push("effects", entry);
  return items;
}

/** Attach recent commands (by id) as their own group, preserving recency order. */
export function buildRecentItems(
  items: readonly PaletteItem[],
  recentIds: readonly string[],
): PaletteItem[] {
  const byId = new Map(items.map((item) => [item.id, item]));
  const recent: PaletteItem[] = [];
  for (const id of recentIds) {
    const item = byId.get(id);
    if (item) recent.push({ ...item, group: "recent", title: item.title, keywords: item.keywords });
  }
  return recent;
}

export function buildPaletteItems(input: {
  engine: EngineState;
  shortcuts: BindingMap;
  recentCommands: readonly string[];
  layerKindLabel: (kind: TreeNode["kind"]) => string;
  capabilities?: Capabilities;
}): PaletteItem[] {
  const actions = buildActionItems(
    input.engine,
    input.shortcuts,
    input.capabilities ?? DEFAULT_CAPABILITIES,
  );
  const tools = buildToolItems(input.shortcuts);
  const layers = buildLayerItems(input.engine.summary, input.engine.selection, input.layerKindLabel);
  const styles = buildStyleItems(input.engine.summary);
  const recent = buildRecentItems([...tools, ...actions, ...layers, ...styles], input.recentCommands);
  return [...actions, ...tools, ...layers, ...styles, ...recent];
}

export interface RankedPaletteItem extends PaletteItem {
  score: number;
  titleIndices: number[];
}

/** Filter + rank palette rows; empty query keeps group order. */
export function filterPalette(items: readonly PaletteItem[], query: string): RankedPaletteItem[] {
  const trimmed = query.trim();
  if (trimmed === "") {
    return items.map((item) => ({ ...item, score: 0, titleIndices: [] }));
  }
  const ranked: RankedPaletteItem[] = [];
  items.forEach((item) => {
    const titleMatch = fuzzyMatch(trimmed, item.title);
    let bestScore = titleMatch?.score ?? null;
    const haystacks = [item.subtitle ?? "", ...item.keywords].filter((text) => text.length > 0);
    for (const text of haystacks) {
      const match = fuzzyMatch(trimmed, text);
      if (match && (bestScore === null || match.score > bestScore)) bestScore = match.score;
    }
    if (bestScore === null) return;
    ranked.push({
      ...item,
      score: bestScore + (titleMatch ? 2 : 0),
      titleIndices: titleMatch?.indices ?? [],
    });
  });
  ranked.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return PALETTE_GROUP_ORDER.indexOf(a.group) - PALETTE_GROUP_ORDER.indexOf(b.group);
  });
  return ranked;
}

/** Group ranked rows in section order for rendering. */
export function groupPaletteItems(
  items: readonly RankedPaletteItem[],
): { group: PaletteGroup; label: string; items: RankedPaletteItem[] }[] {
  const sections: { group: PaletteGroup; label: string; items: RankedPaletteItem[] }[] = [];
  for (const group of PALETTE_GROUP_ORDER) {
    const groupItems = items.filter((item) => item.group === group);
    if (groupItems.length > 0) sections.push({ group, label: groupLabel(group), items: groupItems });
  }
  return sections;
}
