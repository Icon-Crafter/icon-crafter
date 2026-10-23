/**
 * EnginePump — serialises UI input into engine calls.
 *
 * Pointer moves and wheel deltas are high-frequency: only the newest value is
 * kept and flushed on a microtask, so a fast drag cannot flood IPC. Everything
 * else (down/up/keys/commands) is strictly ordered on a promise chain, and every
 * resulting snapshot is applied exactly in order.
 */
import type {
  CommandArgs,
  CommandName,
  DocKind,
  EngineError,
  EngineState,
  KeyInput,
  NodeDetail,
  PointerInput,
  SaveResult,
  ToolId,
  WheelInput,
} from "./contract";
import type { EngineTransport } from "./transport";
import { normalizeEngineError } from "./transport";

type StateHandler = (state: EngineState) => void;
type ErrorHandler = (error: EngineError, context: string) => void;

export class EnginePump {
  readonly transport: EngineTransport;
  private readonly onState: StateHandler;
  private readonly onError: ErrorHandler;
  private chain: Promise<unknown> = Promise.resolve();
  private pendingPointer: PointerInput | null = null;
  private pendingWheel: WheelInput | null = null;
  private flushScheduled = false;

  constructor(transport: EngineTransport, onState: StateHandler, onError: ErrorHandler) {
    this.transport = transport;
    this.onState = onState;
    this.onError = onError;
  }

  /** Resolves after every queued call has been applied. */
  settle(): Promise<void> {
    return this.chain.then(() => undefined);
  }

  private enqueue<T>(task: () => Promise<T>): Promise<T> {
    const run = this.chain.then(task, task);
    this.chain = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  private apply = (state: EngineState): EngineState => {
    this.onState(state);
    return state;
  };

  private fail(context: string) {
    return (error: unknown): never => {
      this.onError(normalizeEngineError(error), context);
      throw error instanceof Error ? error : new Error(normalizeEngineError(error).message);
    };
  }

  private flushPending(): void {
    const pointer = this.pendingPointer;
    const wheel = this.pendingWheel;
    this.pendingPointer = null;
    this.pendingWheel = null;
    if (pointer) {
      void this.enqueue(() => this.transport.pointer(pointer)).then(this.apply).catch(() => undefined);
    }
    if (wheel) {
      void this.enqueue(() => this.transport.wheel(wheel)).then(this.apply).catch(() => undefined);
    }
  }

  private scheduleFlush(): void {
    if (this.flushScheduled) return;
    this.flushScheduled = true;
    queueMicrotask(() => {
      this.flushScheduled = false;
      this.flushPending();
    });
  }

  pointer(input: PointerInput): void {
    if (input.kind === "move") {
      this.pendingPointer = input;
      this.scheduleFlush();
      return;
    }
    this.flushPending();
    void this.enqueue(() => this.transport.pointer(input))
      .then(this.apply)
      .catch(() => undefined);
  }

  wheel(input: WheelInput): void {
    this.pendingWheel = input;
    this.scheduleFlush();
  }

  key(input: KeyInput): void {
    this.flushPending();
    void this.enqueue(() => this.transport.key(input))
      .then(this.apply)
      .catch(() => undefined);
  }

  setTool(tool: ToolId): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.setTool(tool)).then(this.apply).catch(this.fail("set_tool"));
  }

  command<C extends CommandName>(name: C, args?: CommandArgs[C]): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.command(name, args)).then(this.apply);
  }

  newDocument(kind: DocKind): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.newDocument(kind)).then(this.apply).catch(this.fail("new_document"));
  }

  openDocument(path: string): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.openDocument(path)).then(this.apply).catch(this.fail("open_document"));
  }

  closeDocument(): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.closeDocument()).then(this.apply).catch(this.fail("close_document"));
  }

  importSvg(args: { path: string; page_id?: string; parent_id?: string }): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.importSvg(args)).then(this.apply).catch(this.fail("import_svg"));
  }

  importImage(args: { path: string }): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.importImage(args)).then(this.apply).catch(this.fail("import_image"));
  }

  refresh(): Promise<EngineState> {
    return this.enqueue(() => this.transport.engineState()).then(this.apply);
  }

  renderFrame(quality = 1): Promise<ArrayBuffer> {
    return this.transport.renderFrame(quality);
  }

  nodeDetails(ids: string[]): Promise<NodeDetail[]> {
    return this.transport.nodeDetails(ids);
  }

  saveDocument(path?: string): Promise<SaveResult> {
    this.flushPending();
    return this.enqueue(() => this.transport.saveDocument(path));
  }
}
