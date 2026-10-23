import { useEffect, useMemo, useRef, useState } from "react";
import { useAppStore, useCapabilities } from "../engine/store";
import {
  buildPaletteItems,
  filterPalette,
  groupPaletteItems,
  type PaletteItem,
} from "../engine/palette";
import { isMacPlatform, formatBindingForDisplay } from "../engine/shortcuts";
import { en } from "../i18n/en";
import { format } from "../i18n/en";
import { nodeKindLabel } from "../i18n/labels";
import { highlightRuns } from "../utils/fuzzy";
import { Search, X } from "../icons";
import { Kbd } from "./primitives/IconButton";

/**
 * Command palette (Ctrl/Cmd+K). Grouped and searchable; rows execute the same
 * command dispatcher as the rest of the shell.
 */
export function CommandPalette() {
  const open = useAppStore((state) => state.paletteOpen);
  const setPaletteOpen = useAppStore((state) => state.setPaletteOpen);
  const engine = useAppStore((state) => state.engine);
  const shortcuts = useAppStore((state) => state.shortcuts);
  const recentCommands = useAppStore((state) => state.recentCommands);
  const recordCommand = useAppStore((state) => state.recordCommand);
  const capabilities = useCapabilities();

  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const isMac = isMacPlatform();

  const items = useMemo(
    () =>
      open
        ? buildPaletteItems({
            engine,
            shortcuts,
            recentCommands,
            layerKindLabel: nodeKindLabel,
            capabilities,
          })
        : [],
    [open, engine, shortcuts, recentCommands, capabilities],
  );

  const ranked = useMemo(() => filterPalette(items, query), [items, query]);
  const sections = useMemo(() => groupPaletteItems(ranked), [ranked]);
  const flat = useMemo(() => sections.flatMap((section) => section.items), [sections]);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setActiveIndex(0);
      return;
    }
    returnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const frame = requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
    return () => cancelAnimationFrame(frame);
  }, [open]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    if (!open) return;
    const list = listRef.current;
    const row = list?.querySelector<HTMLElement>(`[data-palette-index="${activeIndex}"]`);
    row?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  useEffect(() => {
    if (open) return;
    const previous = returnFocusRef.current;
    if (previous && document.contains(previous)) previous.focus();
    returnFocusRef.current = null;
  }, [open]);

  if (!open) return null;

  const choose = (item: PaletteItem | undefined) => {
    if (!item || item.disabled) return;
    setPaletteOpen(false);
    recordCommand(item.id);
    // Let the palette close before running navigation commands.
    window.setTimeout(() => item.run(), 0);
  };

  const move = (delta: number) => {
    if (flat.length === 0) return;
    let next = activeIndex;
    for (let step = 0; step < flat.length; step += 1) {
      next = (next + delta + flat.length) % flat.length;
      if (!flat[next]?.disabled) break;
    }
    setActiveIndex(next);
  };

  const shortcutLabel = (item: PaletteItem) =>
    item.shortcut ? formatBindingForDisplay(item.shortcut, isMac) : null;

  let rowIndex = 0;

  return (
    <div
      className="palette-backdrop"
      role="presentation"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) setPaletteOpen(false);
      }}
    >
      <div className="palette" role="dialog" aria-modal="true" aria-label={en.palette.label}>
        <div className="palette-search">
          <Search size={16} className="palette-search-icon" aria-hidden="true" />
          <input
            ref={inputRef}
            className="palette-input"
            placeholder={en.palette.placeholder}
            aria-label={en.palette.label}
            value={query}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={flat[activeIndex] ? `palette-item-${flat[activeIndex]?.id}` : undefined}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" || (event.key === "n" && event.ctrlKey)) {
                event.preventDefault();
                move(1);
              } else if (event.key === "ArrowUp" || (event.key === "p" && event.ctrlKey)) {
                event.preventDefault();
                move(-1);
              } else if (event.key === "Enter") {
                event.preventDefault();
                choose(flat[activeIndex]);
              } else if (event.key === "Escape") {
                event.preventDefault();
                setPaletteOpen(false);
              } else if (event.key === "Tab") {
                event.preventDefault();
              }
            }}
          />
          <button
            type="button"
            className="palette-close"
            aria-label={en.common.close}
            onClick={() => setPaletteOpen(false)}
          >
            <X size={14} />
          </button>
        </div>
        <div className="palette-list" role="listbox" id="palette-list" ref={listRef}>
          {sections.length === 0 && (
            <div className="palette-empty">
              <span>{format(en.palette.noResults, { query: query.trim() })}</span>
              <span className="palette-empty-hint">{en.palette.hint}</span>
            </div>
          )}
          {sections.map((section) => (
            <div key={section.group} className="palette-group" role="group" aria-label={section.label}>
              <div className="palette-group-label">{section.label}</div>
              {section.items.map((item) => {
                const index = rowIndex;
                rowIndex += 1;
                const parts = highlightRuns(item.title, item.titleIndices);
                const shortcut = shortcutLabel(item);
                return (
                  <button
                    key={item.id}
                    id={`palette-item-${item.id}`}
                    type="button"
                    role="option"
                    data-palette-index={index}
                    aria-selected={index === activeIndex}
                    className={`palette-item${index === activeIndex ? " palette-item--active" : ""}${
                      item.disabled ? " palette-item--disabled" : ""
                    }`}
                    onPointerMove={() => setActiveIndex(index)}
                    onClick={() => choose(item)}
                  >
                    <span className="palette-item-title">
                      {parts.map((part, partIndex) =>
                        part.match ? (
                          <mark key={partIndex}>{part.text}</mark>
                        ) : (
                          <span key={partIndex}>{part.text}</span>
                        ),
                      )}
                    </span>
                    {item.subtitle && <span className="palette-item-subtitle">{item.subtitle}</span>}
                    {item.disabled && <span className="palette-item-badge">{en.common.unavailable}</span>}
                    {shortcut && (
                      <span className="palette-item-shortcut" aria-hidden="true">
                        {shortcut}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
        <div className="palette-footer">
          <span>
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd> {en.palette.navigateHint}
          </span>
          <span>
            <Kbd>↵</Kbd> {en.palette.hint}
          </span>
          <span>
            <Kbd>Esc</Kbd> {en.palette.close}
          </span>
          {recentCommands.length > 0 && <span className="palette-footer-spacer" />}
        </div>
      </div>
    </div>
  );
}
