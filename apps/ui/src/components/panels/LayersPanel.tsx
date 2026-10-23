import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NodeKind, TreeNode } from "../../engine/contract";
import { en } from "../../i18n/en";
import { format } from "../../i18n/en";
import { flattenVisibleNodes } from "../../utils/tree";
import { nodeKindLabel } from "../../i18n/labels";
import {
  ChevronDown,
  ChevronRight,
  Circle,
  ComponentGlyph,
  Eye,
  EyeOff,
  FrameGlyph,
  GroupGlyph,
  InstanceGlyph,
  ImageIcon,
  Layers,
  Lock,
  LockOpen,
  Minus,
  NodeGlyph,
  Pentagon,
  Search,
  Square,
  Star,
  Type,
  X,
} from "../../icons";
import { useAppStore } from "../../engine/store";
import { useUiStore } from "../../engine/uiStore";
import { runCommand } from "../../engine/commands";
import { IconButton } from "../primitives/IconButton";
import { EmptyState } from "../primitives/Layout";

export function nodeKindIcon(kind: NodeKind, size = 14) {
  switch (kind) {
    case "frame":
    case "slice":
      return <FrameGlyph width={size} height={size} />;
    case "group":
      return <GroupGlyph width={size} height={size} />;
    case "component":
    case "component_set":
      return <ComponentGlyph width={size} height={size} />;
    case "instance":
      return <InstanceGlyph width={size} height={size} />;
    case "rectangle":
      return <Square size={size} />;
    case "ellipse":
      return <Circle size={size} />;
    case "line":
      return <Minus size={size} />;
    case "polygon":
      return <Pentagon size={size} />;
    case "star":
      return <Star size={size} />;
    case "vector":
    case "boolean":
      return <NodeGlyph width={size} height={size} />;
    case "text":
      return <Type size={size} />;
    case "image":
      return <ImageIcon size={size} />;
    default:
      return <Square size={size} />;
  }
}

const EMPTY_NODES: TreeNode[] = [];
const ROW_INDENT = 12;

export function LayersPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const selection = useAppStore((state) => state.engine.selection);
  const hover = useAppStore((state) => state.engine.hover);
  const status = useAppStore((state) => state.status);
  const setHoveredNodeId = useAppStore((state) => state.setHoveredNodeId);
  const renamingNodeId = useUiStore((state) => state.renamingNodeId);
  const setRenamingNodeId = useUiStore((state) => state.setRenamingNodeId);

  const [filter, setFilter] = useState("");
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const [renameDraft, setRenameDraft] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);

  const nodes = summary?.nodes ?? EMPTY_NODES;
  const visible = useMemo(() => flattenVisibleNodes(nodes, collapsed, filter), [nodes, collapsed, filter]);
  const selectedSet = useMemo(() => new Set(selection), [selection]);

  useEffect(() => {
    if (renamingNodeId) {
      const node = nodes.find((item) => item.id === renamingNodeId);
      setRenameDraft(node?.name ?? "");
      requestAnimationFrame(() => {
        renameInputRef.current?.focus();
        renameInputRef.current?.select();
      });
    }
  }, [renamingNodeId, nodes]);

  const toggleCollapsed = useCallback((id: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const selectNode = (node: TreeNode, additive: boolean) => {
    void runCommand("select_nodes", { ids: [node.id], additive });
  };

  const commitRename = () => {
    if (!renamingNodeId) return;
    const name = renameDraft.trim();
    if (name.length > 0) {
      const node = nodes.find((item) => item.id === renamingNodeId);
      if (node && node.name !== name) {
        void runCommand("rename_node", { id: renamingNodeId, name }, { quiet: true });
      }
    }
    setRenamingNodeId(null);
  };

  const onRowKeyDown = (event: React.KeyboardEvent<HTMLDivElement>, node: TreeNode, index: number) => {
    if (renamingNodeId === node.id) return;
    if (event.key === "Enter") {
      event.preventDefault();
      selectNode(node, event.shiftKey);
    } else if (event.key === "F2") {
      event.preventDefault();
      setRenamingNodeId(node.id);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const nextIndex = event.key === "ArrowDown" ? index + 1 : index - 1;
      const next = visible[nextIndex];
      if (next) {
        selectNode(next, event.shiftKey);
        const element = document.querySelector<HTMLElement>(`[data-layer-id="${next.id}"]`);
        element?.focus();
      }
    } else if (event.key === "ArrowLeft") {
      if (node.child_count > 0 && !collapsed.has(node.id)) {
        event.preventDefault();
        toggleCollapsed(node.id);
      } else if (node.parent) {
        void runCommand("select_nodes", { ids: [node.parent] });
      }
    } else if (event.key === "ArrowRight" && node.child_count > 0) {
      event.preventDefault();
      if (collapsed.has(node.id)) toggleCollapsed(node.id);
      else {
        const child = visible[index + 1];
        if (child) {
          void runCommand("select_nodes", { ids: [child.id] });
        }
      }
    }
  };

  return (
    <div className="panel-section" style={{ flex: "1 1 auto", minHeight: 0 }}>
      <div className="layers-toolbar">
        <div className="search-field">
          <Search size={13} aria-hidden="true" />
          <input
            type="search"
            value={filter}
            placeholder={en.layers.searchPlaceholder}
            aria-label={en.layers.searchLabel}
            onChange={(event) => setFilter(event.target.value)}
          />
          {filter && (
            <button type="button" className="search-clear" aria-label={en.common.cancel} onClick={() => setFilter("")}>
              <X size={11} />
            </button>
          )}
        </div>
        <IconButton
          label={en.layers.expand}
          title={en.layers.expand}
          size="sm"
          onClick={() => setCollapsed(new Set())}
          disabled={collapsed.size === 0}
        >
          <Layers size={14} />
        </IconButton>
      </div>

      {visible.length === 0 ? (
        filter ? (
          <EmptyState title={en.empty.searchTitle} body={format(en.layers.noResults, { query: filter })} />
        ) : (
          <EmptyState
            icon={<Layers size={18} />}
            title={en.layers.emptyTitle}
            body={en.layers.emptyBody}
            actions={
              <button
                type="button"
                className="button"
                onClick={() => void runCommand("select_all")}
                disabled={status !== "ready"}
              >
                {en.menu.selectAll}
              </button>
            }
          />
        )
      ) : (
        <div className="layer-tree scroll-thin" role="tree" aria-label={en.layers.title} aria-multiselectable="true">
          {visible.map((node, index) => {
            const selected = selectedSet.has(node.id);
            const isHovered = hover === node.id;
            const hasChildren = node.child_count > 0;
            const isCollapsed = collapsed.has(node.id);
            const renaming = renamingNodeId === node.id;
            return (
              <div
                key={node.id}
                data-layer-id={node.id}
                className={`layer-row${selected ? " layer-row--selected" : ""}${isHovered ? " layer-row--hovered" : ""}${
                  !node.visible ? " layer-row--dim" : ""
                }`}
                style={{ paddingLeft: 4 + node.depth * ROW_INDENT }}
                role="treeitem"
                aria-selected={selected}
                aria-expanded={hasChildren ? !isCollapsed : undefined}
                aria-level={node.depth + 1}
                tabIndex={selected ? 0 : -1}
                onPointerEnter={() => setHoveredNodeId(node.id)}
                onPointerLeave={() => setHoveredNodeId(null)}
                onClick={(event) => selectNode(node, event.shiftKey || event.metaKey || event.ctrlKey)}
                onDoubleClick={() => setRenamingNodeId(node.id)}
                onKeyDown={(event) => onRowKeyDown(event, node, index)}
                onContextMenu={(event) => {
                  event.preventDefault();
                  if (!selected) selectNode(node, false);
                  useUiStore.getState().openLayerMenu(event.clientX, event.clientY, node.id);
                }}
              >
                <button
                  type="button"
                  className={`layer-caret${hasChildren ? " layer-caret--visible" : ""}`}
                  aria-label={isCollapsed ? en.layers.expand : en.layers.collapse}
                  tabIndex={-1}
                  onClick={(event) => {
                    event.stopPropagation();
                    if (hasChildren) toggleCollapsed(node.id);
                  }}
                >
                  {hasChildren ? isCollapsed ? <ChevronRight size={11} /> : <ChevronDown size={11} /> : null}
                </button>
                <span className="layer-icon" aria-hidden="true">
                  {nodeKindIcon(node.kind)}
                </span>
                {renaming ? (
                  <input
                    ref={renameInputRef}
                    className="layer-name-input"
                    aria-label={en.layers.renameLabel}
                    value={renameDraft}
                    onChange={(event) => setRenameDraft(event.target.value)}
                    onClick={(event) => event.stopPropagation()}
                    onBlur={commitRename}
                    onKeyDown={(event) => {
                      event.stopPropagation();
                      if (event.key === "Enter") commitRename();
                      else if (event.key === "Escape") setRenamingNodeId(null);
                    }}
                  />
                ) : (
                  <span className="layer-name" title={`${node.name} · ${nodeKindLabel(node.kind)}`}>
                    {node.name}
                  </span>
                )}
                {node.child_count > 0 && !renaming && <span className="layer-count">{node.child_count}</span>}
                <div className="layer-actions">
                  <button
                    type="button"
                    className={`layer-toggle${node.visible ? " layer-toggle--on" : ""}`}
                    aria-label={node.visible ? en.layers.hide : en.layers.show}
                    title={node.visible ? en.layers.hide : en.layers.show}
                    onClick={(event) => {
                      event.stopPropagation();
                      void runCommand("toggle_visible", { ids: [node.id] });
                    }}
                  >
                    {node.visible ? <Eye size={12} /> : <EyeOff size={12} />}
                  </button>
                  <button
                    type="button"
                    className={`layer-toggle${node.locked ? " layer-toggle--active" : ""}`}
                    aria-label={node.locked ? en.layers.unlock : en.layers.lock}
                    title={node.locked ? en.layers.unlock : en.layers.lock}
                    onClick={(event) => {
                      event.stopPropagation();
                      void runCommand("toggle_locked", { ids: [node.id] });
                    }}
                  >
                    {node.locked ? <Lock size={12} /> : <LockOpen size={12} />}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
