/**
 * Engine transport — the single boundary between the React presentation layer
 * and the Rust engine.
 *
 * Two implementations exist:
 *  - `TauriTransport` (this file): real IPC through Tauri 2 `invoke`.
 *  - `DevTransport` (`devTransport.ts`): a DEV-ONLY in-browser harness used by
 *    `pnpm dev` before/without the Rust IPC. It is loaded through a guarded
 *    dynamic import so it can never become production behaviour.
 *
 * Everything the UI knows about documents, geometry and rendering arrives
 * through this interface. The UI never mutates document state locally.
 */

import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type {
  Bootstrap,
  CommandArgs,
  CommandName,
  DocKind,
  EngineError,
  EngineState,
  ExportPdf,
  ExportRaster,
  ExportResult,
  ExportVector,
  KeyInput,
  NodeDetail,
  PointerInput,
  SaveResult,
  ToolId,
  WheelInput,
} from "./contract";

export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface PickSaveOptions {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
}

export interface PickOpenOptions {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
  multiple?: boolean;
}

export interface EngineTransport {
  /** Which transport is active; surfaced in the status bar / harness banner. */
  readonly kind: "tauri" | "dev";
  bootstrap(): Promise<Bootstrap>;
  newDocument(kind: DocKind): Promise<EngineState>;
  openDocument(path: string): Promise<EngineState>;
  closeDocument(): Promise<EngineState>;
  saveDocument(path?: string): Promise<SaveResult>;
  engineState(): Promise<EngineState>;
  setTool(tool: ToolId): Promise<EngineState>;
  pointer(input: PointerInput): Promise<EngineState>;
  wheel(input: WheelInput): Promise<EngineState>;
  key(input: KeyInput): Promise<EngineState>;
  command<C extends CommandName>(name: C, args?: CommandArgs[C]): Promise<EngineState>;
  /** Raw RGBA8 frame, length = frameWidth * frameHeight * 4. */
  renderFrame(quality?: number): Promise<ArrayBuffer>;
  nodeDetails(ids: string[]): Promise<NodeDetail[]>;
  exportPng(args: ExportRaster): Promise<ExportResult>;
  exportSvg(args: ExportVector): Promise<ExportResult>;
  exportPdf(args: ExportPdf): Promise<ExportResult>;
  importSvg(args: { path: string; page_id?: string; parent_id?: string }): Promise<EngineState>;
  importImage(args: { path: string }): Promise<EngineState>;
  setPreference(key: string, value: unknown): Promise<Bootstrap>;
  pickSavePath(options: PickSaveOptions): Promise<string | null>;
  pickOpenPath(options?: PickOpenOptions): Promise<string | null>;
}

export class TransportUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TransportUnavailableError";
  }
}

// ---------------------------------------------------------------------------
// Tauri implementation
// ---------------------------------------------------------------------------

declare global {
  interface Window {
    /** Set by the Tauri runtime before the bundle executes. */
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Coerce whatever `invoke` returned into an ArrayBuffer without trusting it. */
function toArrayBuffer(value: unknown): ArrayBuffer {
  if (value instanceof ArrayBuffer) return value;
  if (value instanceof Uint8Array) {
    const copy = new Uint8Array(value.byteLength);
    copy.set(value);
    return copy.buffer;
  }
  if (Array.isArray(value)) {
    return new Uint8Array(value as number[]).buffer;
  }
  throw new TransportUnavailableError(
    "render_frame returned an unsupported payload (expected ArrayBuffer).",
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/**
 * Invoke errors surface as the serialized Rust error value; normalize anything
 * unexpected so the UI always has a stable `{ code, message, severity }`.
 */
export function normalizeEngineError(error: unknown): EngineError {
  if (isRecord(error) && typeof error.message === "string" && typeof error.code === "string") {
    const severity = error.severity === "warning" ? "warning" : "error";
    return { code: error.code as EngineError["code"], message: error.message, severity };
  }
  if (error instanceof Error) {
    return { code: "internal", message: error.message, severity: "error" };
  }
  if (typeof error === "string") {
    return { code: "internal", message: error, severity: "error" };
  }
  return {
    code: "internal",
    message: "The engine returned an unexpected error.",
    severity: "error",
  };
}

/** True for errors the engine marks as an unimplemented command ("unsupported"). */
export function isUnsupportedError(error: unknown): boolean {
  return normalizeEngineError(error).code === "unsupported";
}

export class TauriTransport implements EngineTransport {
  readonly kind = "tauri" as const;

  bootstrap(): Promise<Bootstrap> {
    return invoke<Bootstrap>("bootstrap");
  }

  newDocument(kind: DocKind): Promise<EngineState> {
    return invoke<EngineState>("new_document", { kind });
  }

  openDocument(path: string): Promise<EngineState> {
    return invoke<EngineState>("open_document", { path });
  }

  closeDocument(): Promise<EngineState> {
    return invoke<EngineState>("close_document");
  }

  saveDocument(path?: string): Promise<SaveResult> {
    return invoke<SaveResult>("save_document", path ? { path } : {});
  }

  engineState(): Promise<EngineState> {
    return invoke<EngineState>("engine_state");
  }

  setTool(tool: ToolId): Promise<EngineState> {
    return invoke<EngineState>("set_tool", { tool });
  }

  pointer(input: PointerInput): Promise<EngineState> {
    return invoke<EngineState>("pointer", { input });
  }

  wheel(input: WheelInput): Promise<EngineState> {
    return invoke<EngineState>("wheel", { input });
  }

  key(input: KeyInput): Promise<EngineState> {
    return invoke<EngineState>("key", { input });
  }

  command<C extends CommandName>(name: C, args?: CommandArgs[C]): Promise<EngineState> {
    return invoke<EngineState>("command", { name, args: args ?? null });
  }

  async renderFrame(quality = 1): Promise<ArrayBuffer> {
    const raw = await invoke<unknown>("render_frame", { quality });
    return toArrayBuffer(raw);
  }

  nodeDetails(ids: string[]): Promise<NodeDetail[]> {
    return invoke<NodeDetail[]>("node_details", { ids });
  }

  exportPng(args: ExportRaster): Promise<ExportResult> {
    return invoke<ExportResult>("export_png", { args });
  }

  exportSvg(args: ExportVector): Promise<ExportResult> {
    return invoke<ExportResult>("export_svg", { args });
  }

  exportPdf(args: ExportPdf): Promise<ExportResult> {
    return invoke<ExportResult>("export_pdf", { args });
  }

  importSvg(args: { path: string; page_id?: string; parent_id?: string }): Promise<EngineState> {
    return invoke<EngineState>("import_svg", args);
  }

  importImage(args: { path: string }): Promise<EngineState> {
    return invoke<EngineState>("import_image", args);
  }

  setPreference(key: string, value: unknown): Promise<Bootstrap> {
    return invoke<Bootstrap>("set_preference", { key, value });
  }

  async pickSavePath(options: PickSaveOptions): Promise<string | null> {
    const path = await saveDialog({
      title: options.title,
      defaultPath: options.defaultPath,
      filters: options.filters,
    });
    return path ?? null;
  }

  async pickOpenPath(options: PickOpenOptions = {}): Promise<string | null> {
    const selected = await openDialog({
      title: options.title,
      defaultPath: options.defaultPath,
      filters: options.filters,
      multiple: false,
      directory: false,
    });
    return typeof selected === "string" ? selected : null;
  }
}

// ---------------------------------------------------------------------------
// Transport selection
// ---------------------------------------------------------------------------

/**
 * Create the transport for this environment.
 *
 * Inside Tauri: real IPC. In a browser dev server: the labelled dev harness.
 * A production browser build (which is never shipped — Tauri always hosts the
 * UI) fails loudly instead of silently running against fake data.
 */
export async function createTransport(): Promise<EngineTransport> {
  if (isTauriRuntime()) {
    return new TauriTransport();
  }
  if (import.meta.env.DEV) {
    // The dead-code branch is removed from production bundles; the dynamic
    // import exists so `devTransport.ts` is a separate dev-only chunk.
    const { DevTransport } = await import("./devTransport");
    return new DevTransport();
  }
  throw new TransportUnavailableError(
    "Icon Crafter's engine is only available inside the desktop application. Run `pnpm tauri dev` from the repository root.",
  );
}
