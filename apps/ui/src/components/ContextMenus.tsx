import { useUiStore } from "../engine/uiStore";
import { useAppStore } from "../engine/store";
import {
  detachSelectedInstance,
  openExportDialog,
  requestImportSvg,
  runCommand,
  runTool,
  shortcutHint,
} from "../engine/commands";
import { en } from "../i18n/en";
import { nodeKindLabel } from "../i18n/labels";
import {
  ChevronDown,
  ChevronUp,
  Copy,
  Download,
  Eye,
  EyeOff,
  GroupGlyph,
  ImagePlus,
  Lock,
  LockOpen,
  Pencil,
  Scissors,
  Trash2,
  UngroupGlyph,
  ZoomIn,
  ZoomOut,
} from "../icons";
import { MenuItem, MenuLabel, MenuSeparator, MenuSurface } from "./primitives/IconButton";

/**
 * All context menus for the shell (canvas, layer rows, page rows). They reuse
 * the same command dispatcher as toolbars and shortcuts.
 */
export function ContextMenus() {
  const canvasMenu = useUiStore((state) => state.canvasMenu);
  const layerMenu = useUiStore((state) => state.layerMenu);
  const pageMenu = useUiStore((state) => state.pageMenu);
  const closeMenus = useUiStore((state) => state.closeMenus);
  const setRenamingNodeId = useUiStore((state) => state.setRenamingNodeId);
  const setRenamingPageId = useUiStore((state) => state.setRenamingPageId);

  const engine = useAppStore((state) => state.engine);
  const summary = engine.summary;
  const node = layerMenu ? summary?.nodes.find((item) => item.id === layerMenu.nodeId) ?? null : null;
  const page = pageMenu ? summary?.pages.find((item) => item.id === pageMenu.pageId) ?? null : null;

  return (
    <>
      {canvasMenu && (
        <MenuSurface x={canvasMenu.x} y={canvasMenu.y} onClose={closeMenus}>
          <MenuItem label={en.context.canvas.pasteHere} detail={shortcutHint("edit.paste")} icon={<Copy size={14} />} onSelect={() => void runCommand("paste")} />
          <MenuSeparator />
          <MenuItem
            label={en.context.canvas.selectAll}
            detail={shortcutHint("edit.selectAll")}
            onSelect={() => void runCommand("select_all")}
          />
          <MenuItem
            label={en.context.canvas.deselect}
            detail={shortcutHint("edit.deselect")}
            onSelect={() => void runCommand("select_none")}
          />
          <MenuSeparator />
          <MenuItem label={en.context.canvas.addFrame} icon={<ImagePlus size={14} />} onSelect={() => runTool("frame")} />
          <MenuItem
            label={en.context.canvas.createComponent}
            detail={shortcutHint("edit.createComponent")}
            onSelect={() => void runCommand("create_component")}
          />
          <MenuItem label={en.context.canvas.importSvg} icon={<Download size={14} />} onSelect={() => void requestImportSvg()} />
          <MenuSeparator />
          <MenuLabel>{en.menu.view}</MenuLabel>
          <MenuItem
            label={en.menu.zoomIn}
            detail={shortcutHint("view.zoomIn")}
            icon={<ZoomIn size={14} />}
            onSelect={() => void runCommand("zoom_in")}
          />
          <MenuItem
            label={en.menu.zoomOut}
            detail={shortcutHint("view.zoomOut")}
            icon={<ZoomOut size={14} />}
            onSelect={() => void runCommand("zoom_out")}
          />
          <MenuItem
            label={en.menu.zoomFit}
            detail={shortcutHint("view.zoomFit")}
            onSelect={() => void runCommand("zoom_fit")}
          />
          <MenuItem
            label={en.menu.zoomSelection}
            detail={shortcutHint("view.zoomSelection")}
            onSelect={() => void runCommand("zoom_selection")}
          />
          <MenuItem
            label={en.menu.zoom100}
            detail={shortcutHint("view.zoom100")}
            onSelect={() => void runCommand("zoom_100")}
          />
          <MenuSeparator />
          <MenuItem label={en.menu.export} detail={shortcutHint("file.export")} onSelect={() => openExportDialog()} />
        </MenuSurface>
      )}

      {layerMenu && node && (
        <MenuSurface x={layerMenu.x} y={layerMenu.y} onClose={closeMenus}>
          <div className="menu-context-title">
            {nodeKindLabel(node.kind)} · {node.name}
          </div>
          <MenuItem
            label={en.context.layer.rename}
            detail="F2"
            icon={<Pencil size={14} />}
            onSelect={() => setRenamingNodeId(node.id)}
          />
          <MenuSeparator />
          <MenuItem
            label={en.context.layer.cut}
            detail={shortcutHint("edit.cut")}
            icon={<Scissors size={14} />}
            onSelect={() => void runCommand("cut")}
          />
          <MenuItem
            label={en.context.layer.copy}
            detail={shortcutHint("edit.copy")}
            icon={<Copy size={14} />}
            onSelect={() => void runCommand("copy")}
          />
          <MenuItem
            label={en.context.layer.duplicate}
            detail={shortcutHint("edit.duplicate")}
            onSelect={() => void runCommand("duplicate")}
          />
          <MenuItem
            label={en.context.layer.delete}
            detail={shortcutHint("edit.delete")}
            icon={<Trash2 size={14} />}
            danger
            onSelect={() => void runCommand("delete")}
          />
          <MenuSeparator />
          <MenuItem
            label={en.context.layer.group}
            detail={shortcutHint("edit.group")}
            icon={<GroupGlyph width={14} height={14} />}
            onSelect={() => void runCommand("group")}
          />
          <MenuItem
            label={en.context.layer.ungroup}
            detail={shortcutHint("edit.ungroup")}
            icon={<UngroupGlyph width={14} height={14} />}
            onSelect={() => void runCommand("ungroup")}
          />
          <MenuItem
            label={en.context.layer.createComponent}
            detail={shortcutHint("edit.createComponent")}
            onSelect={() => void runCommand("create_component")}
          />
          {node.is_instance && (
            <MenuItem
              label={en.context.layer.detachInstance}
              onSelect={() => void detachSelectedInstance()}
            />
          )}
          <MenuSeparator />
          <MenuItem
            label={en.context.layer.bringToFront}
            detail={shortcutHint("edit.bringToFront")}
            icon={<ChevronUp size={14} />}
            onSelect={() => void runCommand("bring_to_front")}
          />
          <MenuItem
            label={en.context.layer.bringForward}
            detail={shortcutHint("edit.bringForward")}
            icon={<ChevronUp size={14} />}
            onSelect={() => void runCommand("bring_forward")}
          />
          <MenuItem
            label={en.context.layer.sendBackward}
            detail={shortcutHint("edit.sendBackward")}
            icon={<ChevronDown size={14} />}
            onSelect={() => void runCommand("send_backward")}
          />
          <MenuItem
            label={en.context.layer.sendToBack}
            detail={shortcutHint("edit.sendToBack")}
            icon={<ChevronDown size={14} />}
            onSelect={() => void runCommand("send_to_back")}
          />
          <MenuSeparator />
          <MenuItem
            label={node.visible ? en.context.layer.hide : en.context.layer.show}
            icon={node.visible ? <EyeOff size={14} /> : <Eye size={14} />}
            onSelect={() => void runCommand("toggle_visible", { ids: [node.id] })}
          />
          <MenuItem
            label={node.locked ? en.context.layer.unlock : en.context.layer.lock}
            icon={node.locked ? <LockOpen size={14} /> : <Lock size={14} />}
            onSelect={() => void runCommand("toggle_locked", { ids: [node.id] })}
          />
        </MenuSurface>
      )}

      {pageMenu && page && (
        <MenuSurface x={pageMenu.x} y={pageMenu.y} onClose={closeMenus}>
          <div className="menu-context-title">{page.name}</div>
          <MenuItem
            label={en.context.page.rename}
            icon={<Pencil size={14} />}
            onSelect={() => setRenamingPageId(page.id)}
          />
          <MenuItem label={en.context.page.add} onSelect={() => void runCommand("add_page")} />
          <MenuSeparator />
          <MenuItem
            label={en.context.page.delete}
            icon={<Trash2 size={14} />}
            danger
            disabled={summary?.pages.length === 1}
            onSelect={() => void runCommand("delete_page", { page_id: page.id })}
          />
        </MenuSurface>
      )}
    </>
  );
}
