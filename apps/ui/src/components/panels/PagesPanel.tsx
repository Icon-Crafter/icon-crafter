import { useEffect, useRef, useState } from "react";
import { useAppStore } from "../../engine/store";
import { useUiStore } from "../../engine/uiStore";
import { runCommand } from "../../engine/commands";
import { en } from "../../i18n/en";
import { Check, FileText, Plus } from "../../icons";
import { IconButton } from "../primitives/IconButton";
import { EmptyState } from "../primitives/Layout";

/** Pages list: create, activate, rename and delete document pages. */
export function PagesPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const pages = summary?.pages ?? [];
  const activePage = summary?.active_page ?? null;
  const status = useAppStore((state) => state.status);
  const renamingPageId = useUiStore((state) => state.renamingPageId);
  const setRenamingPageId = useUiStore((state) => state.setRenamingPageId);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const ready = status === "ready";

  useEffect(() => {
    if (!renamingPageId) return;
    const page = pages.find((item) => item.id === renamingPageId);
    setDraft(page?.name ?? "");
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }, [renamingPageId, pages]);

  const commitRename = () => {
    if (!renamingPageId) return;
    const name = draft.trim();
    if (name.length > 0) {
      const page = pages.find((item) => item.id === renamingPageId);
      if (page && page.name !== name) {
        void runCommand("rename_page", { page_id: renamingPageId, name }, { quiet: true });
      }
    }
    setRenamingPageId(null);
  };

  return (
    <div className="panel-section" style={{ flex: "1 1 auto", minHeight: 0 }}>
      <div className="panel-section-header">
        <span className="panel-section-title">{en.pages.title}</span>
        <div className="panel-section-actions">
          <IconButton
            label={en.pages.addTooltip}
            size="sm"
            disabled={!ready}
            onClick={() => void runCommand("add_page")}
          >
            <Plus size={14} />
          </IconButton>
        </div>
      </div>

      {pages.length === 0 ? (
        <EmptyState icon={<FileText size={18} />} title={en.empty.pageTitle} body={en.empty.pageBody} />
      ) : (
        <div className="pages-list scroll-thin" role="listbox" aria-label={en.pages.title}>
          {pages.map((page) => {
            const active = page.id === activePage;
            const renaming = renamingPageId === page.id;
            return (
              <div
                key={page.id}
                className={`page-row${active ? " page-row--active" : ""}`}
                role="option"
                aria-selected={active}
                tabIndex={active ? 0 : -1}
                onClick={() => {
                  if (!active) void runCommand("set_active_page", { page_id: page.id });
                }}
                onDoubleClick={() => setRenamingPageId(page.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !active) void runCommand("set_active_page", { page_id: page.id });
                  else if (event.key === "F2") setRenamingPageId(page.id);
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  useUiStore.getState().openPageMenu(event.clientX, event.clientY, page.id);
                }}
              >
                <span className="page-icon" aria-hidden="true">
                  {active ? <Check size={12} /> : <FileText size={13} />}
                </span>
                {renaming ? (
                  <input
                    ref={inputRef}
                    className="page-name-input"
                    aria-label={en.pages.renameLabel}
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    onClick={(event) => event.stopPropagation()}
                    onBlur={commitRename}
                    onKeyDown={(event) => {
                      event.stopPropagation();
                      if (event.key === "Enter") commitRename();
                      else if (event.key === "Escape") setRenamingPageId(null);
                    }}
                  />
                ) : (
                  <>
                    <span className="page-name" title={page.name}>
                      {page.name}
                    </span>
                    {active && <span className="page-active-hint">{en.pages.active}</span>}
                  </>
                )}
                <div className="page-actions">
                  <button
                    type="button"
                    className="layer-toggle"
                    aria-label={en.pages.rename}
                    title={en.pages.rename}
                    onClick={(event) => {
                      event.stopPropagation();
                      setRenamingPageId(page.id);
                    }}
                  >
                    <FileText size={12} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      <div className="panel-footer panel-footer--hint">{en.pages.lastPageError}</div>
    </div>
  );
}
