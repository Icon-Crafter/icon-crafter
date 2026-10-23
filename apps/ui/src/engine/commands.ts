/**
 * Slash commands — the imperative layer every UI control calls.
 *
 * Responsibilities:
 *  - Wrap engine commands with error reporting (thrown errors become toasts).
 *  - Own multi-step flows: dialogs, file pickers, unsaved-changes guarding.
 *  - Translate keyboard actions into engine commands.
 *
 * Components should call these functions instead of talking to the transport
 * directly, so behaviour stays consistent across toolbar, palette and menus.
 */
import type {
  Capabilities,
  CommandArgs,
  CommandName,
  DocKind,
  EngineState,
  ExportPdf,
  ExportRaster,
  ExportResult,
  ExportVector,
  NodePatch,
  ToolId,
} from "./contract";
import { DEFAULT_CAPABILITIES } from "./contract";
import { getActivePump, useAppStore, type UnsavedIntent } from "./store";
import { isTauriRuntime, normalizeEngineError } from "./transport";
import type { ShortcutActionId } from "./shortcuts";
import { en, format } from "../i18n/en";
import { formatBindingForDisplay } from "./shortcuts";

export type ExportFormat = "png" | "svg" | "pdf";
export type ExportScope = "page" | "selection";

interface CommandOptions {
  /** Skip the toast for thrown errors (e.g. undo at history start). */
  quiet?: boolean;
}

const FILTER_DOCUMENT = [{ name: "Icon Crafter document", extensions: ["iconcrafter"] }];
const FILTER_SVG = [{ name: "SVG", extensions: ["svg"] }];
const FILTER_IMAGES = [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }];

/** Honest capability flags for the running engine (bootstrap, with fallbacks). */
export function engineCapabilities(): Capabilities {
  return useAppStore.getState().bootstrap?.capabilities ?? DEFAULT_CAPABILITIES;
}

// ---------------------------------------------------------------------------
// Command execution
// ---------------------------------------------------------------------------

export async function runCommand<C extends CommandName>(
  name: C,
  args?: CommandArgs[C],
  options: CommandOptions = {},
): Promise<EngineState | null> {
  const pump = getActivePump();
  if (!pump) return null;
  try {
    const state = await pump.command(name, args);
    return state;
  } catch (error) {
    if (!options.quiet) {
      useAppStore.getState().reportEngineError(normalizeEngineError(error));
    }
    return null;
  }
}

export async function runTool(tool: ToolId): Promise<void> {
  const pump = getActivePump();
  if (!pump) return;
  try {
    await pump.setTool(tool);
  } catch (error) {
    useAppStore.getState().reportEngineError(normalizeEngineError(error));
  }
}

export async function nudgeSelection(dx: number, dy: number): Promise<void> {
  await runCommand("nudge", { dx, dy });
}

/** Apply a per-node patch to the current selection (or explicit ids). */
export async function applyPatchToSelection(
  patch: NodePatch,
  options: { ids?: string[]; coalesceKey?: string; label?: string } = {},
): Promise<void> {
  const state = useAppStore.getState().engine;
  const ids = options.ids ?? state.selection;
  if (ids.length === 0) return;
  await runCommand("apply_patch", {
    ids,
    patch,
    label: options.label,
    coalesce_key: options.coalesceKey,
  });
}

export function selectedNodeIds(): string[] {
  return useAppStore.getState().engine.selection;
}

// ---------------------------------------------------------------------------
// Dialogs and file flows
// ---------------------------------------------------------------------------

export function openExportDialog(): void {
  useAppStore.getState().openDialog("export");
}

export function openSaveAsDialog(): void {
  useAppStore.getState().openDialog("saveAs");
}

export function suggestFileName(extension: string): string {
  const { engine } = useAppStore.getState();
  const base = engine.doc?.name?.trim() || en.chrome.documentTitleFallback;
  const safe = base.replace(/[\\/:*?"<>|]+/g, "-");
  return `${safe}.${extension}`;
}

function suggestedPath(extension: string): string | undefined {
  const { engine } = useAppStore.getState();
  if (engine.doc?.path) {
    return engine.doc.path.replace(/\.[^./\\]+$/, `.${extension}`);
  }
  return suggestFileName(extension);
}

export async function chooseSavePath(): Promise<string | null> {
  const pump = getActivePump();
  if (!pump) return null;
  try {
    return await pump.transport.pickSavePath({
      title: en.dialogs.saveAs.title,
      defaultPath: suggestedPath("iconcrafter"),
      filters: FILTER_DOCUMENT,
    });
  } catch (error) {
    useAppStore.getState().pushToast({
      kind: "error",
      title: en.dialogs.filePickerUnavailable,
      message: normalizeEngineError(error).message,
    });
    return null;
  }
}

export async function chooseExportPath(exportFormat: ExportFormat): Promise<string | null> {
  const pump = getActivePump();
  if (!pump) return null;
  try {
    return await pump.transport.pickSavePath({
      title: en.dialogs.export.title,
      defaultPath: suggestedPath(exportFormat),
      filters: [{ name: exportFormat.toUpperCase(), extensions: [exportFormat] }],
    });
  } catch (error) {
    useAppStore.getState().pushToast({
      kind: "error",
      title: en.dialogs.filePickerUnavailable,
      message: normalizeEngineError(error).message,
    });
    return null;
  }
}

/** Save silently when a path is known, otherwise fall through to Save As. */
export async function requestSave(saveAs = false): Promise<boolean> {
  const { engine, pump } = useAppStore.getState();
  if (!engine.doc || !pump) return false;
  if (!saveAs && engine.doc.path) {
    return saveDocumentTo(engine.doc.path);
  }
  openSaveAsDialog();
  return false;
}

export async function saveDocumentTo(path: string): Promise<boolean> {
  const { pump, setBusy } = useAppStore.getState();
  if (!pump) return false;
  setBusy(true);
  try {
    const result = await pump.saveDocument(path);
    await useAppStore.getState().refresh();
    useAppStore.getState().pushToast({
      kind: "success",
      title: en.toasts.saved,
      message: format(en.dialogs.saveAs.success, { path: result.path }),
    });
    return true;
  } catch (error) {
    useAppStore.getState().reportEngineError(error);
    return false;
  } finally {
    setBusy(false);
  }
}

export interface ExportOptions {
  format: ExportFormat;
  path: string;
  scale: number;
  quality: number;
  scope: ExportScope;
  transparent: boolean;
}

export async function performExport(options: ExportOptions): Promise<ExportResult | null> {
  const { pump, engine, setBusy } = useAppStore.getState();
  if (!pump) return null;
  // The current Rust engine ignores `node_ids` and `transparent`; the dialog
  // disables those controls for that reason (see apps/ui/README.md).
  const nodeIds = options.scope === "selection" && engine.selection.length > 0 ? engine.selection : undefined;
  setBusy(true);
  try {
    let result: ExportResult;
    if (options.format === "png") {
      const args: ExportRaster = {
        scale: options.scale,
        transparent: options.transparent,
        path: options.path,
        node_ids: nodeIds,
      };
      result = await pump.transport.exportPng(args);
    } else if (options.format === "svg") {
      const args: ExportVector = { path: options.path, node_ids: nodeIds };
      result = await pump.transport.exportSvg(args);
    } else {
      const args: ExportPdf = { path: options.path, node_ids: nodeIds };
      result = await pump.transport.exportPdf(args);
    }
    useAppStore.getState().pushToast({
      kind: "success",
      title: en.toasts.exported,
      message: format(en.dialogs.export.success, { path: result.path }),
    });
    return result;
  } catch (error) {
    const normalized = normalizeEngineError(error);
    const title =
      normalized.code === "unsupported"
        ? format(en.toasts.unsupported, { command: `${options.format.toUpperCase()} export` })
        : en.toasts.fileError;
    useAppStore.getState().pushToast({ kind: "error", title, message: normalized.message });
    return null;
  } finally {
    setBusy(false);
  }
}

// ---------------------------------------------------------------------------
// Document lifecycle with unsaved-changes guarding
// ---------------------------------------------------------------------------

export function requestNewDocument(kind: DocKind = "design"): void {
  guardUnsaved("new", () => performNewDocument(kind));
}

export function requestOpenDocument(): void {
  guardUnsaved("open", () => performOpenDocument());
}

export function requestCloseDocument(): void {
  guardUnsaved("close", () => performCloseDocument());
}

export function guardUnsaved(intent: UnsavedIntent, action: () => Promise<void>): void {
  const { engine, openDialog } = useAppStore.getState();
  if (engine.doc && engine.dirty) {
    openDialog("unsaved", intent);
    return;
  }
  void action();
}

export async function performIntent(intent: UnsavedIntent): Promise<void> {
  if (intent === "new") await performNewDocument("design");
  else if (intent === "open") await performOpenDocument();
  else await performCloseDocument();
}

async function performNewDocument(kind: DocKind): Promise<void> {
  const pump = getActivePump();
  if (!pump) return;
  try {
    await pump.newDocument(kind);
    useAppStore.getState().pushToast({ kind: "success", title: en.toasts.newDocument });
  } catch (error) {
    useAppStore.getState().reportEngineError(error);
  }
}

async function performOpenDocument(): Promise<void> {
  const pump = getActivePump();
  if (!pump) return;
  try {
    const path = await pump.transport.pickOpenPath({
      title: en.menu.openDocument,
      filters: FILTER_DOCUMENT,
    });
    if (!path) return;
    await pump.openDocument(path);
    const name = path.split(/[\\/]/).pop() ?? path;
    useAppStore.getState().pushToast({ kind: "success", title: format(en.toasts.opened, { name }) });
  } catch (error) {
    useAppStore.getState().reportEngineError(error);
  }
}

async function performCloseDocument(): Promise<void> {
  const pump = getActivePump();
  if (!pump) return;
  try {
    await pump.closeDocument();
  } catch (error) {
    useAppStore.getState().reportEngineError(error);
  }
}

/** Unsaved dialog actions. */
export async function resolveUnsavedWithSave(): Promise<void> {
  const store = useAppStore.getState();
  const intent = store.unsavedIntent;
  const { engine } = store;
  if (!engine.doc) {
    store.closeDialog();
    if (intent) await performIntent(intent);
    return;
  }
  if (!engine.doc.path) {
    // Keep the pending intent alive while the Save As dialog asks for a path.
    store.openDialog("saveAs", intent ?? undefined);
    return;
  }
  const saved = await saveDocumentTo(engine.doc.path);
  if (!saved) return;
  store.closeDialog();
  if (intent) await performIntent(intent);
}

export async function resolveUnsavedWithDiscard(): Promise<void> {
  const store = useAppStore.getState();
  const intent = store.unsavedIntent;
  store.closeDialog();
  if (intent) await performIntent(intent);
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

export async function requestImportSvg(): Promise<void> {
  const store = useAppStore.getState();
  const { engine, pump } = store;
  if (!pump) return;
  if (engineCapabilities().import_svg !== true) {
    store.pushToast({
      kind: "warning",
      title: format(en.toasts.unsupported, { command: en.menu.importSvg }),
      message: en.toasts.capabilityUnavailable,
    });
    return;
  }
  try {
    const path = await pump.transport.pickOpenPath({
      title: en.menu.importSvg,
      filters: FILTER_SVG,
    });
    if (!path) return;
    // SVG import targets the active page; when exactly one container layer is
    // selected the SVG is imported into that container (engine validates both).
    const args: { path: string; page_id?: string; parent_id?: string } = { path };
    const pageId = engine.summary?.active_page;
    if (pageId) args.page_id = pageId;
    const parentId = selectedContainerId(engine);
    if (parentId) args.parent_id = parentId;
    await pump.importSvg(args);
  } catch (error) {
    useAppStore.getState().reportEngineError(error);
  }
}

export async function requestPlaceImage(): Promise<void> {
  const store = useAppStore.getState();
  const pump = store.pump;
  if (!pump) return;
  if (engineCapabilities().import_image !== true) {
    store.pushToast({
      kind: "warning",
      title: format(en.toasts.unsupported, { command: en.menu.placeImage }),
      message: en.toasts.capabilityUnavailable,
    });
    return;
  }
  try {
    const path = await pump.transport.pickOpenPath({
      title: en.menu.placeImage,
      filters: FILTER_IMAGES,
    });
    if (!path) return;
    await pump.importImage({ path });
  } catch (error) {
    useAppStore.getState().reportEngineError(error);
  }
}

const CONTAINER_KINDS: ReadonlySet<string> = new Set([
  "frame",
  "group",
  "component",
  "component_set",
]);

/** The single selected container that can receive an import, if any. */
function selectedContainerId(engine: EngineState): string | undefined {
  if (engine.selection.length !== 1) return undefined;
  const node = engine.selection_details[0];
  if (!node) return undefined;
  return CONTAINER_KINDS.has(node.kind) ? node.id : undefined;
}

// ---------------------------------------------------------------------------
// Keyboard actions
// ---------------------------------------------------------------------------

export function shortcutHint(id: ShortcutActionId): string {
  const binding = useAppStore.getState().shortcuts[id];
  return formatBindingForDisplay(binding);
}

export function handleShortcut(id: ShortcutActionId): void {
  switch (id) {
    case "tool.move":
      void runTool("move");
      return;
    case "tool.hand":
      void runTool("hand");
      return;
    case "tool.frame":
      void runTool("frame");
      return;
    case "tool.slice":
      void runTool("slice");
      return;
    case "tool.rectangle":
      void runTool("rectangle");
      return;
    case "tool.ellipse":
      void runTool("ellipse");
      return;
    case "tool.line":
      void runTool("line");
      return;
    case "tool.polygon":
      void runTool("polygon");
      return;
    case "tool.star":
      void runTool("star");
      return;
    case "tool.pen":
      void runTool("pen");
      return;
    case "tool.node":
      void runTool("node");
      return;
    case "tool.text":
      void runTool("text");
      return;
    case "tool.image":
      void requestPlaceImage();
      return;
    case "tool.comment":
      void runTool("comment");
      return;
    case "tool.connector":
      void runTool("connector");
      return;
    case "edit.undo":
      void runCommand("undo", undefined, { quiet: true });
      return;
    case "edit.redo":
      void runCommand("redo", undefined, { quiet: true });
      return;
    case "edit.cut":
      void runCommand("cut");
      return;
    case "edit.copy":
      void runCommand("copy");
      return;
    case "edit.paste":
      void runCommand("paste");
      return;
    case "edit.duplicate":
      void runCommand("duplicate");
      return;
    case "edit.delete":
      void runCommand("delete");
      return;
    case "edit.selectAll":
      void runCommand("select_all");
      return;
    case "edit.deselect":
      void runCommand("select_none");
      return;
    case "edit.group":
      void runCommand("group");
      return;
    case "edit.ungroup":
      void runCommand("ungroup");
      return;
    case "edit.createComponent":
      void runCommand("create_component");
      return;
    case "edit.bringToFront":
      void runCommand("bring_to_front");
      return;
    case "edit.bringForward":
      void runCommand("bring_forward");
      return;
    case "edit.sendBackward":
      void runCommand("send_backward");
      return;
    case "edit.sendToBack":
      void runCommand("send_to_back");
      return;
    case "view.zoomIn":
      void runCommand("zoom_in");
      return;
    case "view.zoomOut":
      void runCommand("zoom_out");
      return;
    case "view.zoom100":
      void runCommand("zoom_100");
      return;
    case "view.zoomFit":
      void runCommand("zoom_fit");
      return;
    case "view.zoomSelection":
      void runCommand("zoom_selection");
      return;
    case "file.new":
      requestNewDocument();
      return;
    case "file.open":
      requestOpenDocument();
      return;
    case "file.save":
      void requestSave();
      return;
    case "file.saveAs":
      openSaveAsDialog();
      return;
    case "file.export":
      openExportDialog();
      return;
    case "file.close":
      requestCloseDocument();
      return;
    case "app.commandPalette": {
      const open = useAppStore.getState().paletteOpen;
      useAppStore.getState().setPaletteOpen(!open);
      return;
    }
    default:
      return;
  }
}

/** Tool availability note shown in the status bar / tooltip. */
export function toolShortcutHint(toolId: ToolId): string | null {
  const bindings = useAppStore.getState().shortcuts;
  const id = `tool.${toolId}` as ShortcutActionId;
  const binding = bindings[id];
  return binding ? formatBindingForDisplay(binding) : null;
}

/** Environment note surfaced in the status bar. */
export function engineEnvironmentLabel(): string {
  return isTauriRuntime() ? "Tauri IPC" : "Browser harness";
}
