/**
 * EnginePump — serialises UI input into engine calls.
 *
 * Pointer moves and wheel deltas are high-frequency: only the newest value is
 * kept and flushed on a microtask, so a fast drag cannot flood IPC. Everything
 * else (down/up/keys/commands) is strictly ordered on a promise chain, and every
 * resulting snapshot is applied exactly in order.
 */
import type {
  AddAnnotationArgs,
  AiTextTask,
  CodegenTarget,
  CommandArgs,
  CommandName,
  DocKind,
  EngineError,
  EngineState,
  GeneratedCode,
  Handoff,
  InspectSnapshot,
  KeyInput,
  MarkReadyArgs,
  NodeDetail,
  PointerInput,
  RevisionDiff,
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
  private pendingState: EngineState | null = null;
  private stateScheduled = false;

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
    // State updates drive a full React render (panels, layers, overlay) and a
    // canvas frame. Coalesce them to one commit per animation frame: during a
    // drag the engine can answer faster than the display refreshes, and
    // committing every response made the whole editor lag.
    this.pendingState = state;
    if (!this.stateScheduled) {
      this.stateScheduled = true;
      requestAnimationFrame(() => {
        this.stateScheduled = false;
        const pending = this.pendingState;
        this.pendingState = null;
        if (pending) this.onState(pending);
      });
    }
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
    // One pointer/wheel call per display frame: high-polling-rate mice and
    // trackpads otherwise flood the engine with more moves than it can answer
    // inside a frame, which is perceived as lag.
    requestAnimationFrame(() => {
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

  setDevMode(enabled: boolean): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.setDevMode(enabled)).then(this.apply).catch(this.fail("set_dev_mode"));
  }

  devInspect(): Promise<InspectSnapshot> {
    this.flushPending();
    return this.enqueue(() => this.transport.devInspect());
  }

  devCodegen(target: CodegenTarget, precision?: number): Promise<GeneratedCode> {
    this.flushPending();
    return this.enqueue(() => this.transport.devCodegen(target, precision));
  }

  devHandoff(): Promise<Handoff> {
    this.flushPending();
    return this.enqueue(() => this.transport.devHandoff());
  }

  devAddAnnotation(args: AddAnnotationArgs): Promise<Handoff> {
    this.flushPending();
    return this.enqueue(() => this.transport.devAddAnnotation(args));
  }

  devMarkReady(args: MarkReadyArgs): Promise<Handoff> {
    this.flushPending();
    return this.enqueue(() => this.transport.devMarkReady(args));
  }

  devSnapshot(): Promise<string> {
    this.flushPending();
    return this.enqueue(() => this.transport.devSnapshot());
  }

  devCompare(documentJson: string): Promise<RevisionDiff> {
    this.flushPending();
    return this.enqueue(() => this.transport.devCompare(documentJson));
  }

  devResolveAnnotation(annotationId: string): Promise<Handoff> {
    this.flushPending();
    return this.enqueue(() => this.transport.devResolveAnnotation(annotationId));
  }

  prototypePlay(frameId?: string): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.prototypePlay(frameId)).then(this.apply);
  }

  prototypeStop(): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.prototypeStop()).then(this.apply);
  }

  prototypeBack(): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.prototypeBack()).then(this.apply);
  }

  aiGenerateLayout(args: { prompt: string; width?: number; height?: number }): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.aiGenerateLayout(args)).then(this.apply);
  }

  aiApplyLast(args: { confirmed?: boolean } = {}): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.aiApplyLast(args)).then(this.apply);
  }

  aiDiscardLast(): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.aiDiscardLast()).then(this.apply);
  }

  aiRenameLayers(): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.aiRenameLayers()).then(this.apply);
  }

  aiTextTask(args: { task: AiTextTask; input: string }): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.aiTextTask(args)).then(this.apply);
  }

  pluginInstall(args: { manifest: Record<string, unknown> | string }): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.pluginInstall(args)).then(this.apply);
  }

  pluginInstallExample(): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.pluginInstallExample()).then(this.apply);
  }

  pluginRemove(pluginId: string): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.pluginRemove(pluginId)).then(this.apply);
  }

  pluginSetEnabled(pluginId: string, enabled: boolean): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.pluginSetEnabled(pluginId, enabled)).then(this.apply);
  }

  pluginSetSetting(pluginId: string, key: string, value: unknown): Promise<EngineState> {
    this.flushPending();
    return this.enqueue(() => this.transport.pluginSetSetting(pluginId, key, value)).then(this.apply);
  }
}
