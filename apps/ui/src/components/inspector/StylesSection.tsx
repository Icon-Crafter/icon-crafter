import { useState } from "react";
import type { NodeDetail, StyleEntry } from "../../engine/contract";
import { useAppStore } from "../../engine/store";
import { applyStyle, createStyleFromSelection, deleteStyle, renameStyle } from "../../engine/commands";
import { detectStyleKind, styleNameFor } from "../../engine/designSystem";
import { en } from "../../i18n/en";
import { Plus, Trash2 } from "../../icons";
import { Section } from "../primitives/Layout";

/** Inspector styles section: apply document styles to the selection, or create one. */
export function StylesSection({ node }: { node: NodeDetail }) {
  const summary = useAppStore((state) => state.engine.summary);
  const selection = useAppStore((state) => state.engine.selection);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const styles = summary?.styles;
  const kind = detectStyleKind(node);
  const applicable: readonly StyleEntry[] =
    kind === "text"
      ? styles?.texts ?? []
      : kind === "effect"
        ? styles?.effects ?? []
        : styles?.paints ?? [];

  const commitRename = (style: StyleEntry) => {
    const name = draft.trim();
    if (name && name !== style.name) void renameStyle(style.id, name);
    setRenamingId(null);
  };

  return (
    <Section id="styles" title={en.inspector.sectionStyles}>
      <div className="style-apply-row">
        <select
          className="select-field"
          aria-label={en.styles.apply}
          defaultValue=""
          onChange={(event) => {
            if (event.target.value) void applyStyle(event.target.value);
            event.target.value = "";
          }}
        >
          <option value="">{applicable.length === 0 ? en.styles.emptyKind : en.styles.apply}</option>
          {applicable.map((style) => (
            <option key={style.id} value={style.id}>
              {style.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="button button--ghost button--small"
          disabled={!kind}
          title={kind ? en.assets.createHint : en.styles.fromSelectionUnavailable}
          onClick={() => void createStyleFromSelection()}
        >
          <Plus size={12} />
          {en.styles.createFromSelection}
        </button>
      </div>
      {kind && (
        <p className="asset-group-hint">
          {en.styles.selectedNode.replace("{name}", styleNameFor(kind, node))}
        </p>
      )}

      {applicable.length > 0 && (
        <div className="style-manage-list">
          {applicable.map((style) => (
            <div key={style.id} className="style-manage-row">
              {renamingId === style.id ? (
                <input
                  className="text-input"
                  autoFocus
                  value={draft}
                  aria-label={en.styles.renameLabel}
                  onChange={(event) => setDraft(event.target.value)}
                  onBlur={() => commitRename(style)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") commitRename(style);
                    if (event.key === "Escape") setRenamingId(null);
                  }}
                />
              ) : (
                <button
                  type="button"
                  className="style-manage-name"
                  title={`${en.styles.apply} · ${en.styles.rename}`}
                  onClick={() => {
                    if (selection.length > 0) void applyStyle(style.id);
                  }}
                  onDoubleClick={() => {
                    setRenamingId(style.id);
                    setDraft(style.name);
                  }}
                >
                  {style.name}
                </button>
              )}
              <button
                type="button"
                className="icon-button icon-button--danger"
                aria-label={en.styles.delete}
                title={en.styles.delete}
                onClick={() => void deleteStyle(style.id, style.name)}
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
    </Section>
  );
}
