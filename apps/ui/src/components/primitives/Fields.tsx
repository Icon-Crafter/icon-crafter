import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import type { Color } from "../../engine/contract";
import { Check } from "../../icons";
import { colorToCss, colorToHex, hexToColor } from "../../utils/color";
import { cx, roundForInput } from "../../utils/dom";

/* -------------------------------------------------------------------------- */
/* NumberInput                                                                */
/* -------------------------------------------------------------------------- */

export interface NumberInputProps {
  value: number;
  onChange: (value: number) => void;
  /** Called once when an interaction ends (used for history coalescing). */
  onCommit?: (value: number) => void;
  label?: ReactNode;
  suffix?: string;
  min?: number;
  max?: number;
  step?: number;
  precision?: number;
  disabled?: boolean;
  ariaLabel: string;
  /** Show the number scrubbed by dragging the label, like a pro design tool. */
  scrubbable?: boolean;
}

/**
 * Numeric field with Enter/blur commit, arrow-key stepping and optional
 * pointer scrubbing. Values are clamped and rounded; invalid drafts revert.
 */
function formatDraft(value: number, precision: number): string {
  return String(roundForInput(value, precision));
}

export function NumberInput({
  value,
  onChange,
  onCommit,
  label,
  suffix,
  min = -1_000_000,
  max = 1_000_000,
  step = 1,
  precision = 2,
  disabled,
  ariaLabel,
  scrubbable = true,
}: NumberInputProps) {
  const [draft, setDraft] = useState<string>(() => formatDraft(value, precision));
  const [focused, setFocused] = useState(false);
  const scrub = useRef<{ startX: number; startValue: number; moved: boolean } | null>(null);

  useEffect(() => {
    if (!focused) setDraft(formatDraft(value, precision));
  }, [value, focused, precision]);

  const clampValue = useCallback(
    (next: number) => {
      if (Number.isNaN(next)) return value;
      return Math.min(max, Math.max(min, next));
    },
    [max, min, value],
  );

  const commit = useCallback(
    (raw: string) => {
      const parsed = Number.parseFloat(raw.replace(",", "."));
      if (Number.isNaN(parsed)) {
        setDraft(formatDraft(value, precision));
        return;
      }
      const next = Math.round(clampValue(parsed) * 10 ** precision) / 10 ** precision;
      setDraft(formatDraft(next, precision));
      if (next !== value) onChange(next);
      onCommit?.(next);
    },
    [clampValue, onCommit, onChange, precision, value],
  );

  const nudge = useCallback(
    (direction: 1 | -1, shift: boolean, alt: boolean) => {
      const factor = shift ? 10 : alt ? 0.1 : 1;
      const next = Math.round(clampValue(value + direction * step * factor) * 10 ** precision) / 10 ** precision;
      onChange(next);
      onCommit?.(next);
    },
    [clampValue, onCommit, onChange, precision, step, value],
  );

  /* Pointer scrubbing on the label. */
  const onLabelPointerDown = (event: React.PointerEvent<HTMLSpanElement>) => {
    if (!scrubbable || disabled || event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    scrub.current = { startX: event.clientX, startValue: value, moved: false };
  };

  const onLabelPointerMove = (event: React.PointerEvent<HTMLSpanElement>) => {
    const state = scrub.current;
    if (!state) return;
    const delta = event.clientX - state.startX;
    if (!state.moved && Math.abs(delta) < 3) return;
    state.moved = true;
    const factor = event.shiftKey ? 10 : event.altKey ? 0.1 : 1;
    const next = Math.round(clampValue(state.startValue + delta * step * factor) * 10 ** precision) / 10 ** precision;
    onChange(next);
  };

  const endScrub = (event: React.PointerEvent<HTMLSpanElement>) => {
    const state = scrub.current;
    scrub.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (state?.moved) onCommit?.(value);
  };

  return (
    <label className={cx("field", disabled && "field--disabled")}>
      {label !== undefined && (
        <span
          className="field-label"
          onPointerDown={onLabelPointerDown}
          onPointerMove={onLabelPointerMove}
          onPointerUp={endScrub}
          onPointerCancel={endScrub}
          style={scrubbable && !disabled ? { cursor: "ew-resize", touchAction: "none" } : undefined}
          aria-hidden="true"
        >
          {label}
        </span>
      )}
      <input
        type="text"
        inputMode="decimal"
        aria-label={ariaLabel}
        value={draft}
        disabled={disabled}
        onFocus={(event) => {
          setFocused(true);
          event.currentTarget.select();
        }}
        onBlur={(event) => {
          setFocused(false);
          commit(event.currentTarget.value);
        }}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            commit(event.currentTarget.value);
            event.currentTarget.blur();
          } else if (event.key === "Escape") {
            setDraft(formatDraft(value, precision));
            event.currentTarget.blur();
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            nudge(1, event.shiftKey, event.altKey);
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            nudge(-1, event.shiftKey, event.altKey);
          }
        }}
      />
      {suffix && <span className="field-suffix">{suffix}</span>}
    </label>
  );
}

/* -------------------------------------------------------------------------- */
/* ColorField                                                                 */
/* -------------------------------------------------------------------------- */

export interface ColorFieldProps {
  color: Color;
  onChange: (color: Color) => void;
  onCommit?: (color: Color) => void;
  ariaLabel: string;
  disabled?: boolean;
}

const RECENT_COLORS_KEY = "iconcrafter.recentColors.v1";

function loadRecentColors(): string[] {
  try {
    const raw = window.localStorage.getItem(RECENT_COLORS_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string").slice(0, 8) : [];
  } catch {
    return [];
  }
}

function rememberColor(hex: string) {
  try {
    const next = [hex, ...loadRecentColors().filter((item) => item !== hex)].slice(0, 8);
    window.localStorage.setItem(RECENT_COLORS_KEY, JSON.stringify(next));
  } catch {
    /* storage can be unavailable — non-critical */
  }
}

/** Hex field + swatch with a popover colour picker (native picker + recents). */
export function ColorField({ color, onChange, onCommit, ariaLabel, disabled }: ColorFieldProps) {
  const [open, setOpen] = useState(false);
  const [hexDraft, setHexDraft] = useState(() => colorToHex(color, false));
  const rootRef = useRef<HTMLDivElement>(null);
  const hex = colorToHex(color, false);

  useEffect(() => {
    setHexDraft(hex);
  }, [hex]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const applyHex = (raw: string) => {
    const parsed = hexToColor(raw);
    if (!parsed) {
      setHexDraft(hex);
      return;
    }
    const next = { ...parsed, a: color.a };
    onChange(next);
    onCommit?.(next);
    rememberColor(colorToHex(next, false));
  };

  const recents = useMemo(() => loadRecentColors(), [open]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="color-field" ref={rootRef} style={{ position: "relative" }}>
      <button
        type="button"
        className="color-swatch"
        aria-label={`${ariaLabel}: open color picker`}
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="color-swatch-inner" style={{ background: colorToCss(color) }} />
      </button>
      <span className="color-hex">
        <input
          type="text"
          aria-label={ariaLabel}
          value={hexDraft}
          disabled={disabled}
          spellCheck={false}
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) => setHexDraft(event.target.value)}
          onBlur={(event) => applyHex(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              applyHex(event.currentTarget.value);
              event.currentTarget.blur();
            } else if (event.key === "Escape") {
              setHexDraft(hex);
              event.currentTarget.blur();
            }
          }}
        />
      </span>
      {open && !disabled && (
        <div
          className="color-field-popover"
          style={{ top: "calc(100% + 6px)", left: 0 }}
          role="dialog"
          aria-label={`${ariaLabel} picker`}
        >
          <input
            type="color"
            className="color-native"
            aria-label={ariaLabel}
            value={hex}
            onChange={(event) => {
              const parsed = hexToColor(event.target.value);
              if (parsed) {
                const next = { ...parsed, a: color.a };
                onChange(next);
                onCommit?.(next);
              }
            }}
          />
          <label className="inline-row">
            <span className="inline-label">Alpha</span>
            <input
              className="slider"
              type="range"
              min={0}
              max={100}
              value={Math.round(color.a * 100)}
              aria-label={`${ariaLabel} alpha`}
              onChange={(event) => onChange({ ...color, a: Number(event.target.value) / 100 })}
              onPointerUp={() => onCommit?.(color)}
            />
            <span className="field-suffix">{Math.round(color.a * 100)}%</span>
          </label>
          {recents.length > 0 && (
            <div className="color-recents" aria-label="Recent colors">
              {recents.map((recent) => (
                <button
                  key={recent}
                  type="button"
                  className="color-recent"
                  style={{ background: recent }}
                  aria-label={`Use ${recent}`}
                  onClick={() => {
                    const parsed = hexToColor(recent);
                    if (parsed) {
                      const next = { ...parsed, a: color.a };
                      onChange(next);
                      onCommit?.(next);
                    }
                  }}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Segmented control                                                          */
/* -------------------------------------------------------------------------- */

export interface SegmentedOption<T extends string> {
  value: T;
  label?: string;
  icon?: ReactNode;
  title?: string;
  disabled?: boolean;
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  ariaLabel,
}: {
  options: ReadonlyArray<SegmentedOption<T>>;
  value: T;
  onChange: (value: T) => void;
  ariaLabel: string;
}) {
  return (
    <div className="segmented" role="group" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          title={option.title ?? option.label}
          aria-label={option.title ?? option.label}
          aria-pressed={value === option.value}
          disabled={option.disabled}
          onClick={() => onChange(option.value)}
        >
          {option.icon ?? option.label}
        </button>
      ))}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Checkbox                                                                   */
/* -------------------------------------------------------------------------- */

export interface CheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
  title?: string;
}

export function Checkbox({ checked, onChange, label, disabled, title }: CheckboxProps) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      title={title}
      disabled={disabled}
      className="toggle-row"
      onClick={() => onChange(!checked)}
    >
      <span className={cx("checkbox", checked && "checkbox--checked")} aria-hidden="true">
        <Check size={10} strokeWidth={3} />
      </span>
      <span>{label}</span>
    </button>
  );
}

/* -------------------------------------------------------------------------- */
/* Text input                                                                 */
/* -------------------------------------------------------------------------- */

export interface TextFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  ariaLabel: string;
  onCommit?: (value: string) => void;
}

export function TextField({
  ariaLabel,
  onCommit,
  className,
  value,
  defaultValue,
  onChange,
  onKeyDown,
  onBlur,
  ...rest
}: TextFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const externalValue = value !== undefined ? String(value) : undefined;
  const [draft, setDraft] = useState(() => externalValue ?? String(defaultValue ?? ""));

  useEffect(() => {
    if (externalValue === undefined) return;
    if (document.activeElement !== inputRef.current) setDraft(externalValue);
  }, [externalValue]);

  return (
    <input
      ref={inputRef}
      type="text"
      aria-label={ariaLabel}
      spellCheck={false}
      className={cx("text-input", className)}
      {...rest}
      value={externalValue !== undefined ? draft : undefined}
      defaultValue={externalValue === undefined ? defaultValue : undefined}
      onChange={(event) => {
        setDraft(event.target.value);
        onChange?.(event);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          onCommit?.(event.currentTarget.value);
          event.currentTarget.blur();
        }
        onKeyDown?.(event);
      }}
      onBlur={(event) => {
        onCommit?.(event.currentTarget.value);
        onBlur?.(event);
      }}
    />
  );
}
