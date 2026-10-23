import { useRef, useState, type ReactNode } from "react";
import { useAppStore, useCapabilities } from "../engine/store";
import type { DocKind, ToolId } from "../engine/contract";
import { en } from "../i18n/en";
import { formatZoom } from "../utils/number";
import {
  ChevronDown,
  Circle,
  ConnectorGlyph,
  Download,
  FrameGlyph,
  Hand,
  ImageIcon,
  Logo,
  Maximize2,
  MessageSquare,
  Minus,
  Move,
  NodeGlyph,
  Pentagon,
  PenTool,
  Plus,
  Redo2,
  Search,
  Share2,
  SliceGlyph,
  Square,
  Star,
  Type,
  Undo2,
  Play,
} from "../icons";
import {
  openExportDialog,
  requestCloseDocument,
  requestImportSvg,
  requestNewDocument,
  requestOpenDocument,
  requestPlaceImage,
  requestSave,
  runCommand,
  runTool,
  shortcutHint,
  toolShortcutHint,
} from "../engine/commands";
import { MenuItem, MenuLabel, MenuSeparator, MenuSurface } from "./primitives/IconButton";
import { IconButton } from "./primitives/IconButton";

interface ToolDefinition {
  id: ToolId;
  label: string;
  icon: ReactNode;
}

const TOOL_GROUPS: ToolDefinition[][] = [
  [
    { id: "move", label: en.tools.move, icon: <Move size={15} /> },
    { id: "hand", label: en.tools.hand, icon: <Hand size={15} /> },
  ],
  [
    { id: "frame", label: en.tools.frame, icon: <FrameGlyph width={15} height={15} /> },
    { id: "slice", label: en.tools.slice, icon: <SliceGlyph width={15} height={15} /> },
  ],
  [
    { id: "rectangle", label: en.tools.rectangle, icon: <Square size={15} /> },
    { id: "ellipse", label: en.tools.ellipse, icon: <Circle size={15} /> },
    { id: "line", label: en.tools.line, icon: <Minus size={15} /> },
    { id: "polygon", label: en.tools.polygon, icon: <Pentagon size={15} /> },
    { id: "star", label: en.tools.star, icon: <Star size={15} /> },
  ],
  [
    { id: "pen", label: en.tools.pen, icon: <PenTool size={15} /> },
    { id: "node", label: en.tools.node, icon: <NodeGlyph width={15} height={15} /> },
  ],
  [{ id: "text", label: en.tools.text, icon: <Type size={15} /> }],
  [
    { id: "image", label: en.tools.image, icon: <ImageIcon size={15} /> },
    { id: "comment", label: en.tools.comment, icon: <MessageSquare size={15} /> },
    { id: "connector", label: en.tools.connector, icon: <ConnectorGlyph width={15} height={15} /> },
  ],
];

function firstAvailableKind(): DocKind {
  return "design";
}

/** Top application toolbar: brand, document identity, tools, history, zoom, export. */
export function Toolbar() {
  const engine = useAppStore((state) => state.engine);
  const status = useAppStore((state) => state.status);
  const transportKind = useAppStore((state) => state.transportKind);
  const pushToast = useAppStore((state) => state.pushToast);
  const capabilities = useCapabilities();

  const [fileMenu, setFileMenu] = useState<{ x: number; y: number } | null>(null);
  const [zoomMenu, setZoomMenu] = useState<{ x: number; y: number } | null>(null);
  const [exportMenu, setExportMenu] = useState<{ x: number; y: number } | null>(null);
  const brandRef = useRef<HTMLButtonElement>(null);
  const zoomRef = useRef<HTMLDivElement>(null);
  const exportRef = useRef<HTMLButtonElement>(null);

  const doc = engine.doc;
  const ready = status === "ready";
  const title = doc?.name?.trim() || en.chrome.documentTitleFallback;
  const zoomLabel = formatZoom(engine.viewport.zoom);
  const toolHint = (tool: ToolId, label: string) => {
    const hint = toolShortcutHint(tool);
    return hint ? `${label} (${hint})` : label;
  };

  const openFileMenu = () => {
    const rect = brandRef.current?.getBoundingClientRect();
    if (rect) setFileMenu({ x: rect.left, y: rect.bottom + 6 });
  };

  const openZoomMenu = () => {
    const rect = zoomRef.current?.getBoundingClientRect();
    if (rect) setZoomMenu({ x: rect.left, y: rect.bottom + 6 });
  };

  const openExportMenu = () => {
    const rect = exportRef.current?.getBoundingClientRect();
    if (rect) setExportMenu({ x: rect.right, y: rect.bottom + 6 });
  };

  const newDocument = (kind: DocKind) => {
    setFileMenu(null);
    requestNewDocument(kind);
  };

  const unavailable = (label: string) => {
    pushToast({
      kind: "warning",
      title: en.toasts.unsupported.replace("{command}", label),
      message: en.toasts.unsupportedBody,
    });
  };

  return (
    <header className="toolbar" role="toolbar" aria-label={en.toolbar.tools}>
      <div className="toolbar-brand">
        <button
          ref={brandRef}
          type="button"
          className="brand-button"
          aria-label={en.toolbar.brand}
          title={en.toolbar.brand}
          onClick={openFileMenu}
        >
          <Logo size={26} className="brand-logo" />
        </button>
        <button
          type="button"
          className="toolbar-doc"
          aria-label={`${en.toolbar.fileMenu}: ${title}`}
          title={en.toolbar.fileMenu}
          onClick={openFileMenu}
        >
          <span className="doc-title">{title}</span>
          <span className="doc-state">
            {transportKind === "dev" ? (
              <>
                <span className="doc-state-dot doc-state-dot--dev" />
                {en.app.devBadge}
              </>
            ) : engine.dirty ? (
              <>
                <span className="doc-state-dot doc-state-dot--dirty" />
                {en.chrome.stateUnsaved}
              </>
            ) : (
              <>
                <span className="doc-state-dot doc-state-dot--saved" />
                {en.chrome.stateSaved}
              </>
            )}
          </span>
          <ChevronDown size={12} aria-hidden="true" style={{ color: "var(--text-tertiary)" }} />
        </button>
      </div>

      <div className="toolbar-center">
        <div className="tool-cluster" role="group" aria-label={en.toolbar.tools}>
          {TOOL_GROUPS.map((group, groupIndex) => (
            <div key={`tool-group-${groupIndex}`} style={{ display: "flex", alignItems: "center" }}>
              {groupIndex > 0 && <span className="toolbar-sep" aria-hidden="true" />}
              {group.map((tool) => (
                <button
                  key={tool.id}
                  type="button"
                  className={`tool-button${engine.tool === tool.id ? " tool-button--active" : ""}`}
                  aria-label={tool.label}
                  aria-pressed={engine.tool === tool.id}
                  title={toolHint(tool.id, tool.label)}
                  disabled={!ready}
                  onClick={() => void runTool(tool.id)}
                >
                  {tool.icon}
                </button>
              ))}
            </div>
          ))}
        </div>
      </div>

      <div className="toolbar-right">
        <div className="toolbar-group">
          <IconButton
            label={en.toolbar.undo}
            shortcut={shortcutHint("edit.undo")}
            disabled={!ready || !engine.can_undo}
            onClick={() => void runCommand("undo")}
          >
            <Undo2 size={15} />
          </IconButton>
          <IconButton
            label={en.toolbar.redo}
            shortcut={shortcutHint("edit.redo")}
            disabled={!ready || !engine.can_redo}
            onClick={() => void runCommand("redo")}
          >
            <Redo2 size={15} />
          </IconButton>
        </div>

        <span className="toolbar-sep" aria-hidden="true" />

        <div className="toolbar-group">
          <div className="zoom-cluster" ref={zoomRef} aria-label={en.toolbar.zoomMenu}>
            <IconButton label={en.toolbar.zoomOut} size="sm" disabled={!ready} onClick={() => void runCommand("zoom_out")}>
              <Minus size={13} />
            </IconButton>
            <button
              type="button"
              className="zoom-value"
              aria-label={`${en.toolbar.zoomLevel}: ${zoomLabel}`}
              title={en.toolbar.zoomMenu}
              disabled={!ready}
              onClick={openZoomMenu}
            >
              {zoomLabel}
            </button>
            <IconButton label={en.toolbar.zoomIn} size="sm" disabled={!ready} onClick={() => void runCommand("zoom_in")}>
              <Plus size={13} />
            </IconButton>
          </div>
        </div>

        <IconButton
          label={en.menu.zoomFit}
          title={`${en.menu.zoomFit} (${shortcutHint("view.zoomFit")})`}
          disabled={!ready}
          onClick={() => void runCommand("zoom_fit")}
        >
          <Maximize2 size={14} />
        </IconButton>

        <span className="toolbar-sep" aria-hidden="true" />

        <div className="toolbar-group">
          <button
            ref={exportRef}
            type="button"
            className="export-button"
            title={en.toolbar.exportHint}
            disabled={!ready || !doc}
            onClick={openExportMenu}
          >
            <Download size={14} />
            {en.toolbar.export}
            <ChevronDown size={12} />
          </button>
        </div>

        <IconButton
          label={en.palette.label}
          title={`${en.palette.label} (${shortcutHint("app.commandPalette")})`}
          disabled={!ready}
          onClick={() => useAppStore.getState().setPaletteOpen(true)}
        >
          <Search size={15} />
        </IconButton>

        <span className="toolbar-sep" aria-hidden="true" />

        <div className="toolbar-group">
          <IconButton
            label={en.chrome.present}
            title={en.chrome.presentHint}
            onClick={() => unavailable(en.chrome.present)}
          >
            <Play size={14} />
          </IconButton>
          <IconButton
            label={en.chrome.share}
            title={en.chrome.shareHint}
            onClick={() => unavailable(en.chrome.share)}
          >
            <Share2 size={14} />
          </IconButton>
        </div>
      </div>

      {fileMenu && (
        <MenuSurface x={fileMenu.x} y={fileMenu.y} onClose={() => setFileMenu(null)}>
          <MenuLabel>{en.menu.file}</MenuLabel>
          <MenuItem label={en.menu.newDocument} shortcut={shortcutHint("file.new")} onSelect={() => newDocument(firstAvailableKind())} />
          <MenuItem
            label={en.menu.newDocument.replace(en.menu.newDocument, `${en.menu.newDocument} · whiteboard`)}
            shortcut=""
            onSelect={() => newDocument("whiteboard")}
          />
          <MenuItem
            label={`${en.menu.newDocument} · slides`}
            shortcut=""
            onSelect={() => newDocument("slides")}
          />
          <MenuSeparator />
          <MenuItem
            label={en.menu.openDocument}
            shortcut={shortcutHint("file.open")}
            onSelect={() => {
              setFileMenu(null);
              void requestOpenDocument();
            }}
          />
          <MenuItem
            label={en.menu.save}
            shortcut={shortcutHint("file.save")}
            onSelect={() => {
              setFileMenu(null);
              void requestSave(false);
            }}
          />
          <MenuItem
            label={en.menu.saveAs}
            shortcut={shortcutHint("file.saveAs")}
            onSelect={() => {
              setFileMenu(null);
              void requestSave(true);
            }}
          />
          <MenuItem
            label={en.menu.export}
            shortcut={shortcutHint("file.export")}
            onSelect={() => {
              setFileMenu(null);
              openExportDialog();
            }}
          />
          <MenuSeparator />
          <MenuItem
            label={en.menu.closeDocument}
            shortcut={shortcutHint("file.close")}
            disabled={!doc}
            onSelect={() => {
              setFileMenu(null);
              requestCloseDocument();
            }}
          />
        </MenuSurface>
      )}

      {zoomMenu && (
        <MenuSurface x={zoomMenu.x} y={zoomMenu.y} onClose={() => setZoomMenu(null)}>
          <MenuLabel>{en.toolbar.zoomMenu}</MenuLabel>
          <MenuItem
            label={en.toolbar.zoomTo100}
            detail="100%"
            shortcut={shortcutHint("view.zoom100")}
            onSelect={() => {
              setZoomMenu(null);
              void runCommand("zoom_100");
            }}
          />
          <MenuItem
            label={en.toolbar.zoomToFit}
            shortcut={shortcutHint("view.zoomFit")}
            onSelect={() => {
              setZoomMenu(null);
              void runCommand("zoom_fit");
            }}
          />
          <MenuItem
            label={en.toolbar.zoomToSelection}
            shortcut={shortcutHint("view.zoomSelection")}
            disabled={engine.selection.length === 0}
            onSelect={() => {
              setZoomMenu(null);
              void runCommand("zoom_selection");
            }}
          />
          <MenuSeparator />
          {[0.25, 0.5, 1, 2, 4].map((zoom) => (
            <MenuItem
              key={zoom}
              label={formatZoom(zoom)}
              onSelect={() => {
                setZoomMenu(null);
                void runCommand("zoom_to", { zoom });
              }}
            />
          ))}
        </MenuSurface>
      )}

      {exportMenu && (
        <MenuSurface x={exportMenu.x} y={exportMenu.y} align="end" onClose={() => setExportMenu(null)}>
          <MenuLabel>{en.dialogs.export.title}</MenuLabel>
          <MenuItem
            label={en.dialogs.export.formatPng}
            onSelect={() => {
              setExportMenu(null);
              openExportDialog();
            }}
          />
          <MenuItem
            label={en.dialogs.export.formatSvg}
            onSelect={() => {
              setExportMenu(null);
              openExportDialog();
            }}
          />
          <MenuItem
            label={en.dialogs.export.formatPdf}
            disabled={capabilities.export_pdf !== true}
            title={
              capabilities.export_pdf !== true
                ? en.toasts.capabilityUnavailable
                : en.dialogs.export.formatPdf
            }
            onSelect={() => {
              setExportMenu(null);
              openExportDialog();
            }}
          />
          <MenuSeparator />
          <MenuItem
            label={en.menu.importSvg}
            disabled={capabilities.import_svg !== true}
            title={
              capabilities.import_svg !== true
                ? en.toasts.capabilityUnavailable
                : en.menu.importSvg
            }
            onSelect={() => {
              setExportMenu(null);
              requestImportSvg();
            }}
          />
          <MenuItem
            label={en.menu.placeImage}
            disabled={capabilities.import_image !== true}
            title={
              capabilities.import_image !== true
                ? en.toasts.capabilityUnavailable
                : en.menu.placeImage
            }
            onSelect={() => {
              setExportMenu(null);
              requestPlaceImage();
            }}
          />
        </MenuSurface>
      )}
    </header>
  );
}
