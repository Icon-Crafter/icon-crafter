import { useState } from "react";
import type { StyleEntry } from "../../engine/contract";
import { useAppStore } from "../../engine/store";
import { applyStyle, createStyleFromSelection, deleteStyle, renameStyle } from "../../engine/commands";
import { en } from "../../i18n/en";
import { Contrast, Palette, Plus, Sparkles, Trash2, Type } from "../../icons";
import { EmptyState } from "../primitives/Layout";

interface StyleListProps {
  title: string;
  icon: React.ReactNode;
  kind: "paint" | "text" | "effect";
  items: readonly StyleEntry[];
}

function StyleList({ title, icon, kind, items }: StyleListProps) {
  const selection = useAppStore((state) => state.engine.selection);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  if (items.length === 0) {
    return (
      <div className="panel-section">
        <div className="panel-section-header">
          <span className="panel-section-title">
            {icon}
            {title}
          </span>
          <span className="panel-tab-count">0</span>
        </div>
        <p className="asset-group-hint">{en.styles.emptyKind}</p>
      </div>
    );
  }

  const commitRename = (style: StyleEntry) => {
    const name = draft.trim();
    if (name && name !== style.name) void renameStyle(style.id, name);
    setRenamingId(null);
  };

  return (
    <div className="panel-section">
      <div className="panel-section-header">
        <span className="panel-section-title">
          {icon}
          {title}
        </span>
        <span className="panel-tab-count">{items.length}</span>
      </div>
      <div className="asset-list">
        {items.map((style) => {
          const renaming = renamingId === style.id;
          return (
            <div key={style.id} className="asset-row asset-row--style">
              <button
                type="button"
                className={`asset-main${selection.length === 0 ? " asset-main--disabled" : ""}`}
                title={selection.length === 0 ? en.styles.noSelection : en.styles.applyHint}
                aria-label={`${en.styles.apply}: ${style.name}`}
                disabled={selection.length === 0}
                onClick={() => void applyStyle(style.id)}
              >
                <span
                  className={`asset-swatch${
                    kind === "text" ? " asset-swatch--text" : kind === "effect" ? " asset-swatch--effect" : ""
                  }`}
                >
                  {kind === "text" ? <Type size={11} /> : kind === "effect" ? <Sparkles size={11} /> : null}
                </span>
                {renaming ? (
                  <input
                    className="text-input asset-name-input"
                    autoFocus
                    value={draft}
                    aria-label={en.styles.renameLabel}
                    onChange={(event) => setDraft(event.target.value)}
                    onBlur={() => commitRename(style)}
                    onClick={(event) => event.stopPropagation()}
                    onKeyDown={(event) => {
                      event.stopPropagation();
                      if (event.key === "Enter") commitRename(style);
                      if (event.key === "Escape") setRenamingId(null);
                    }}
                  />
                ) : (
                  <span className="asset-name">{style.name}</span>
                )}
                <span className="asset-kind">{en.styles[kind === "paint" ? "kindPaint" : kind === "text" ? "kindText" : "kindEffect"]}</span>
              </button>
              <div className="asset-row-actions">
                <button
                  type="button"
                  className="icon-button"
                  aria-label={en.styles.rename}
                  title={en.styles.rename}
                  onClick={() => {
                    setRenamingId(style.id);
                    setDraft(style.name);
                  }}
                >
                  <Type size={12} />
                </button>
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
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Assets tab: document-wide styles, creatable from the current selection. */
export function AssetsPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const selection = useAppStore((state) => state.engine.selection);
  const styles = summary?.styles;

  const total =
    (styles?.paints.length ?? 0) + (styles?.texts.length ?? 0) + (styles?.effects.length ?? 0);

  return (
    <div className="assets-panel">
      <div className="panel-section-header">
        <span className="panel-section-title">{en.assets.title}</span>
        <button
          type="button"
          className="button button--ghost button--small"
          title={en.assets.createHint}
          disabled={selection.length === 0}
          onClick={() => void createStyleFromSelection()}
        >
          <Plus size={12} />
          {en.styles.createFromSelection}
        </button>
      </div>

      {total === 0 ? (
        <EmptyState icon={<Palette size={18} />} title={en.assets.emptyTitle} body={en.assets.emptyBody} />
      ) : (
        <div className="assets-scroll scroll-thin">
          <StyleList title={en.assets.paletteTitle} icon={<Palette size={12} />} kind="paint" items={styles?.paints ?? []} />
          <StyleList title={en.assets.textTitle} icon={<Type size={12} />} kind="text" items={styles?.texts ?? []} />
          <StyleList title={en.assets.effectsTitle} icon={<Contrast size={12} />} kind="effect" items={styles?.effects ?? []} />
        </div>
      )}

      <p className="panel-footer-hint">{en.assets.footer}</p>
    </div>
  );
}
