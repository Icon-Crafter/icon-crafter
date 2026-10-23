import type { Color, Effect, Fill, NodeDetail, Paint, Stroke } from "../../engine/contract";
import { en } from "../../i18n/en";
import { openExportDialog, runCommand } from "../../engine/commands";
import { effectTypeLabel } from "../../i18n/labels";
import {
  Bold,
  Contrast,
  Eye,
  EyeOff,
  Italic,
  Minus,
  Plus,
  SlidersHorizontal,
  Trash2,
  Type,
} from "../../icons";
import { ColorField, NumberInput, Segmented } from "../primitives/Fields";
import { Section } from "../primitives/Layout";
import { IconButton } from "../primitives/IconButton";
import {
  createEffect,
  createFill,
  createStroke,
  effectColor,
  usePatchSession,
} from "./shared";

function paintTypeLabel(type: Paint["type"]): string {
  switch (type) {
    case "solid":
      return en.inspector.fillSolid;
    case "image":
      return en.inspector.fillImage;
    default:
      return en.inspector.fillGradient;
  }
}

function paintSolidColor(paint: Paint): Color | null {
  return paint.type === "solid" ? paint.color : null;
}

function patchFillAt(fills: Fill[], index: number, next: Fill): Fill[] {
  const copy = fills.slice();
  copy[index] = next;
  return copy;
}

/* -------------------------------------------------------------------------- */
/* Fill                                                                       */
/* -------------------------------------------------------------------------- */

export function FillSection({ node }: { node: NodeDetail }) {
  const session = usePatchSession("fill");
  const fills = node.fills;

  return (
    <Section
      id="fill"
      title={en.inspector.sectionFill}
      actions={
        <IconButton
          label={en.inspector.addFill}
          size="sm"
          onClick={() => session.apply({ fills: [...fills, createFill()] }, { label: "add_fill", coalesce: false })}
        >
          <Plus size={13} />
        </IconButton>
      }
    >
      {fills.length === 0 && (
        <button
          type="button"
          className="section-empty-button"
          onClick={() => session.apply({ fills: [createFill()] }, { label: "add_fill", coalesce: false })}
        >
          <Plus size={12} /> {en.inspector.addFill}
        </button>
      )}
      {fills.map((fill, index) => {
        const solid = paintSolidColor(fill.paint);
        const update = (next: Fill, coalesce = true) =>
          session.apply({ fills: patchFillAt(fills, index, next) }, { label: "fill", coalesce });
        return (
          <div className="fill-row" key={`fill-${index}`}>
            {solid ? (
              <ColorField
                ariaLabel={en.inspector.fillColor}
                color={solid}
                disabled={!fill.visible}
                onChange={(color) => update({ ...fill, paint: { type: "solid", color } })}
                onCommit={() => session.end()}
              />
            ) : (
              <div className="fill-type-unsupported">
                <span className="color-swatch color-swatch--muted" aria-hidden="true" />
                <span className="field-suffix">{paintTypeLabel(fill.paint.type)}</span>
              </div>
            )}
            <div className="fill-row-tools">
              <NumberInput
                ariaLabel={`${en.inspector.fillOpacity} ${index + 1}`}
                value={Math.round(fill.opacity * 100)}
                min={0}
                max={100}
                precision={0}
                onChange={(value) => update({ ...fill, opacity: value / 100 })}
                onCommit={() => session.end()}
                suffix="%"
              />
              <IconButton
                label={fill.visible ? en.common.hide : en.common.show}
                size="sm"
                onClick={() => update({ ...fill, visible: !fill.visible }, false)}
              >
                {fill.visible ? <Eye size={12} /> : <EyeOff size={12} />}
              </IconButton>
              <IconButton
                label={en.inspector.removeFill}
                size="sm"
                onClick={() =>
                  session.apply(
                    { fills: fills.filter((_, itemIndex) => itemIndex !== index) },
                    { label: "remove_fill", coalesce: false },
                  )
                }
              >
                <Minus size={12} />
              </IconButton>
            </div>
          </div>
        );
      })}
    </Section>
  );
}

/* -------------------------------------------------------------------------- */
/* Stroke                                                                     */
/* -------------------------------------------------------------------------- */

export function StrokeSection({ node }: { node: NodeDetail }) {
  const session = usePatchSession("stroke");
  const strokes = node.strokes;

  return (
    <Section
      id="stroke"
      title={en.inspector.sectionStroke}
      actions={
        <IconButton
          label={en.inspector.addStroke}
          size="sm"
          onClick={() => session.apply({ strokes: [...strokes, createStroke()] }, { label: "add_stroke", coalesce: false })}
        >
          <Plus size={13} />
        </IconButton>
      }
    >
      {strokes.length === 0 && (
        <button
          type="button"
          className="section-empty-button"
          onClick={() => session.apply({ strokes: [createStroke()] }, { label: "add_stroke", coalesce: false })}
        >
          <Plus size={12} /> {en.inspector.addStroke}
        </button>
      )}
      {strokes.map((stroke, index) => {
        const solid = paintSolidColor(stroke.paint);
        const update = (next: Stroke, coalesce = true) =>
          session.apply({ strokes: patchStrokeAt(strokes, index, next) }, { label: "stroke", coalesce });
        return (
          <div className="fill-row" key={`stroke-${index}`}>
            {solid ? (
              <ColorField
                ariaLabel={en.inspector.strokeColor}
                color={solid}
                disabled={!stroke.visible}
                onChange={(color) => update({ ...stroke, paint: { type: "solid", color } })}
                onCommit={() => session.end()}
              />
            ) : (
              <div className="fill-type-unsupported">
                <span className="color-swatch color-swatch--muted" aria-hidden="true" />
                <span className="field-suffix">{paintTypeLabel(stroke.paint.type)}</span>
              </div>
            )}
            <div className="fill-row-tools">
              <NumberInput
                ariaLabel={en.inspector.strokeWidth}
                value={stroke.weight}
                min={0}
                step={0.5}
                precision={2}
                onChange={(value) => update({ ...stroke, weight: value })}
                onCommit={() => session.end()}
                suffix={en.common.px}
              />
              <IconButton
                label={en.inspector.removeStroke}
                size="sm"
                onClick={() =>
                  session.apply(
                    { strokes: strokes.filter((_, itemIndex) => itemIndex !== index) },
                    { label: "remove_stroke", coalesce: false },
                  )
                }
              >
                <Minus size={12} />
              </IconButton>
            </div>
            <div className="inline-row" style={{ gridColumn: "1 / -1" }}>
              <span className="inline-label">{en.inspector.strokePosition}</span>
              <Segmented
                ariaLabel={en.inspector.strokePosition}
                value={stroke.align}
                onChange={(value) => update({ ...stroke, align: value }, false)}
                options={[
                  { value: "inside", label: en.inspector.strokeInside },
                  { value: "center", label: en.inspector.strokeCenter },
                  { value: "outside", label: en.inspector.strokeOutside },
                ]}
              />
            </div>
          </div>
        );
      })}
    </Section>
  );
}

function patchStrokeAt(strokes: Stroke[], index: number, next: Stroke): Stroke[] {
  const copy = strokes.slice();
  copy[index] = next;
  return copy;
}

/* -------------------------------------------------------------------------- */
/* Effects                                                                    */
/* -------------------------------------------------------------------------- */

export function EffectsSection({ node }: { node: NodeDetail }) {
  const session = usePatchSession("effects");
  const effects = node.effects;
  const update = (index: number, next: Effect, coalesce = true) => {
    const copy = effects.slice();
    copy[index] = next;
    session.apply({ effects: copy }, { label: "effect", coalesce });
  };

  return (
    <Section
      id="effects"
      title={en.inspector.sectionEffects}
      actions={
        <IconButton
          label={en.inspector.addEffect}
          size="sm"
          onClick={() =>
            session.apply({ effects: [...effects, createEffect()] }, { label: "add_effect", coalesce: false })
          }
        >
          <Plus size={13} />
        </IconButton>
      }
    >
      {effects.length === 0 && (
        <button
          type="button"
          className="section-empty-button"
          onClick={() => session.apply({ effects: [createEffect()] }, { label: "add_effect", coalesce: false })}
        >
          <Plus size={12} /> {en.inspector.addEffect}
        </button>
      )}
      {effects.map((effect, index) => {
        const isShadow = effect.type === "drop_shadow" || effect.type === "inner_shadow";
        return (
          <div className="effect-row" key={`effect-${index}`}>
            <div className="effect-row-header">
              <span className="effect-icon" aria-hidden="true">
                {isShadow ? <Contrast size={12} /> : <SlidersHorizontal size={12} />}
              </span>
              <span className="effect-label">{effectTypeLabel(effect.type)}</span>
              <IconButton
                label={en.inspector.removeEffect}
                size="sm"
                onClick={() =>
                  session.apply(
                    { effects: effects.filter((_, itemIndex) => itemIndex !== index) },
                    { label: "remove_effect", coalesce: false },
                  )
                }
              >
                <Trash2 size={12} />
              </IconButton>
            </div>
            {isShadow ? (
              <>
                <div className="field-grid">
                  <ColorField
                    ariaLabel={en.inspector.effectColor}
                    color={effectColor(effect)}
                    onChange={(color) => {
                      if (effect.type === "drop_shadow" || effect.type === "inner_shadow") {
                        update(index, { ...effect, color });
                      }
                    }}
                    onCommit={() => session.end()}
                  />
                  <NumberInput
                    ariaLabel={en.inspector.effectX}
                    label={en.inspector.effectX}
                    value={effect.offset.x}
                    precision={1}
                    onChange={(value) => update(index, { ...effect, offset: { ...effect.offset, x: value } })}
                    onCommit={() => session.end()}
                    suffix={en.common.px}
                  />
                  <NumberInput
                    ariaLabel={en.inspector.effectY}
                    label={en.inspector.effectY}
                    value={effect.offset.y}
                    precision={1}
                    onChange={(value) => update(index, { ...effect, offset: { ...effect.offset, y: value } })}
                    onCommit={() => session.end()}
                    suffix={en.common.px}
                  />
                  <NumberInput
                    ariaLabel={en.inspector.effectBlur}
                    label={en.inspector.effectBlur}
                    value={effect.blur}
                    min={0}
                    precision={1}
                    onChange={(value) => update(index, { ...effect, blur: value })}
                    onCommit={() => session.end()}
                    suffix={en.common.px}
                  />
                  <NumberInput
                    ariaLabel={en.inspector.effectSpread}
                    label={en.inspector.effectSpread}
                    value={effect.spread}
                    precision={1}
                    onChange={(value) => update(index, { ...effect, spread: value })}
                    onCommit={() => session.end()}
                    suffix={en.common.px}
                  />
                </div>
              </>
            ) : effect.type === "layer_blur" || effect.type === "background_blur" ? (
              <div className="field-grid">
                <NumberInput
                  ariaLabel={en.inspector.effectBlur}
                  label={en.inspector.effectBlur}
                  value={effect.radius}
                  min={0}
                  precision={1}
                  onChange={(value) => update(index, { ...effect, radius: value })}
                  onCommit={() => session.end()}
                  suffix={en.common.px}
                />
              </div>
            ) : null}
          </div>
        );
      })}
    </Section>
  );
}

/* -------------------------------------------------------------------------- */
/* Layout                                                                     */
/* -------------------------------------------------------------------------- */

export function LayoutSection({ node }: { node: NodeDetail }) {
  const session = usePatchSession("layout");
  if (!node.layout) {
    return (
      <Section id="layout" title={en.inspector.sectionLayout} defaultOpen={false}>
        <p className="section-hint">{en.inspector.layoutUnavailable}</p>
      </Section>
    );
  }
  const layout = node.layout;
  const update = (partial: Partial<typeof layout>) =>
    session.apply({ layout: { ...layout, ...partial } }, { label: "layout" });
  return (
    <Section id="layout" title={en.inspector.sectionLayout}>
      <div className="inline-row">
        <span className="inline-label">{en.inspector.direction}</span>
        <Segmented
          ariaLabel={en.inspector.direction}
          value={layout.direction}
          onChange={(value) => update({ direction: value })}
          options={[
            { value: "horizontal", label: "H" },
            { value: "vertical", label: "V" },
          ]}
        />
      </div>
      <div className="field-grid">
        <NumberInput
          ariaLabel={en.inspector.gap}
          label={en.inspector.gap}
          value={layout.gap}
          min={0}
          onChange={(value) => update({ gap: value })}
          onCommit={() => session.end()}
          suffix={en.common.px}
        />
        <NumberInput
          ariaLabel={`${en.inspector.gap} vertical`}
          label="↕"
          value={layout.gap}
          min={0}
          onChange={(value) => update({ gap: value })}
          onCommit={() => session.end()}
          suffix={en.common.px}
          disabled
        />
      </div>
      <div className="field-grid">
        <NumberInput
          ariaLabel={en.inspector.padding}
          label="T"
          value={layout.padding.top}
          min={0}
          onChange={(value) => update({ padding: { ...layout.padding, top: value } })}
          onCommit={() => session.end()}
          suffix={en.common.px}
        />
        <NumberInput
          ariaLabel={en.inspector.padding}
          label="R"
          value={layout.padding.right}
          min={0}
          onChange={(value) => update({ padding: { ...layout.padding, right: value } })}
          onCommit={() => session.end()}
          suffix={en.common.px}
        />
        <NumberInput
          ariaLabel={en.inspector.padding}
          label="B"
          value={layout.padding.bottom}
          min={0}
          onChange={(value) => update({ padding: { ...layout.padding, bottom: value } })}
          onCommit={() => session.end()}
          suffix={en.common.px}
        />
        <NumberInput
          ariaLabel={en.inspector.padding}
          label="L"
          value={layout.padding.left}
          min={0}
          onChange={(value) => update({ padding: { ...layout.padding, left: value } })}
          onCommit={() => session.end()}
          suffix={en.common.px}
        />
      </div>
      <p className="section-hint">{en.inspector.constraintsUnavailable}</p>
    </Section>
  );
}

/* -------------------------------------------------------------------------- */
/* Typography                                                                 */
/* -------------------------------------------------------------------------- */

export function TypographySection({ node }: { node: NodeDetail }) {
  const session = usePatchSession("text");
  const text = node.text;
  if (!text) return null;
  const style = text.style;
  const patchText = (partial: Partial<typeof text>) =>
    session.apply({ text: { ...text, ...partial } }, { label: "text" });
  const patchStyle = (partial: Partial<typeof style>) =>
    session.apply({ text: { ...text, style: { ...style, ...partial } } }, { label: "text_style" });

  return (
    <Section id="typography" title={en.inspector.sectionTypography}>
      <div className="inline-row">
        <span className="inline-label">{en.inspector.fontFamily}</span>
        <span className="field-suffix" title={en.inspector.fontUnavailable} style={{ flex: 1, textAlign: "right" }}>
          {style.font_family}
        </span>
      </div>
      <div className="field-grid">
        <NumberInput
          ariaLabel={en.inspector.fontSize}
          label={en.inspector.fontSize}
          value={style.font_size}
          min={1}
          step={1}
          onChange={(value) => patchStyle({ font_size: value })}
          onCommit={() => session.end()}
          suffix={en.common.px}
        />
        <NumberInput
          ariaLabel={en.inspector.fontWeight}
          label={en.inspector.fontWeight}
          value={style.font_weight}
          min={100}
          max={900}
          step={100}
          precision={0}
          onChange={(value) => patchStyle({ font_weight: value })}
          onCommit={() => session.end()}
        />
        <NumberInput
          ariaLabel={en.inspector.lineHeight}
          label={en.inspector.lineHeight}
          value={style.line_height.value}
          min={0}
          precision={2}
          onChange={(value) => patchStyle({ line_height: { ...style.line_height, value } })}
          onCommit={() => session.end()}
        />
        <NumberInput
          ariaLabel={en.inspector.letterSpacing}
          label={en.inspector.letterSpacing}
          value={style.letter_spacing}
          step={0.1}
          precision={2}
          onChange={(value) => patchStyle({ letter_spacing: value })}
          onCommit={() => session.end()}
        />
      </div>
      <div className="inline-row">
        <span className="inline-label">{en.inspector.textAlign}</span>
        <div className="segmented">
          {(["left", "center", "right", "justified"] as const).map((align) => (
            <button
              key={align}
              type="button"
              aria-label={align}
              aria-pressed={text.paragraph.align === align}
              onClick={() =>
                session.apply(
                  { text: { ...text, paragraph: { ...text.paragraph, align } } },
                  { label: "text_align", coalesce: false },
                )
              }
            >
              {align === "left" ? "L" : align === "center" ? "C" : align === "right" ? "R" : "J"}
            </button>
          ))}
        </div>
        <span className="inline-spacer" />
        <div className="segmented">
          <IconButton
            label={en.inspector.fontWeight}
            size="sm"
            onClick={() => patchStyle({ italic: !style.italic })}
          >
            <Italic size={12} />
          </IconButton>
          <IconButton
            label={en.inspector.fontWeight}
            size="sm"
            onClick={() => patchStyle({ font_weight: style.font_weight >= 600 ? 400 : 700 })}
          >
            <Bold size={12} />
          </IconButton>
        </div>
      </div>
      <div className="field field--full">
        <span className="field-label">
          <Type size={11} /> {en.inspector.textContent}
        </span>
        <textarea
          className="text-input textarea"
          aria-label={en.inspector.textContent}
          value={text.characters}
          rows={3}
          onChange={(event) => patchText({ characters: event.target.value })}
          onBlur={() => session.end()}
        />
      </div>
      <p className="section-hint">{en.inspector.textEditHint}</p>
    </Section>
  );
}

/* -------------------------------------------------------------------------- */
/* Component                                                                  */
/* -------------------------------------------------------------------------- */

export function ComponentSection({ node }: { node: NodeDetail }) {
  const role = node.component;
  const isInstance = role?.type === "instance";
  return (
    <Section id="component" title={en.inspector.sectionComponent} defaultOpen={isInstance}>
      {isInstance ? (
        <>
          <p className="section-hint">{en.inspector.componentInstance}</p>
          <button
            type="button"
            className="button"
            onClick={() => void runCommand("detach_instance")}
          >
            {en.inspector.detachInstance}
          </button>
        </>
      ) : node.kind === "component" ? (
        <p className="section-hint">{en.inspector.componentMain}</p>
      ) : (
        <>
          <p className="section-hint">{en.components.createHint}</p>
          <button
            type="button"
            className="button"
            onClick={() => void runCommand("create_component")}
          >
            {en.components.createFromSelection}
          </button>
        </>
      )}
    </Section>
  );
}

/* -------------------------------------------------------------------------- */
/* Export                                                                     */
/* -------------------------------------------------------------------------- */

export function ExportSection() {
  return (
    <Section id="export" title={en.inspector.sectionExport} defaultOpen={false}>
      <p className="section-hint">{en.inspector.exportHint}</p>
      <button
        type="button"
        className="button button--primary"
        onClick={() => openExportDialog()}
      >
        {en.inspector.exportSelection}
      </button>
    </Section>
  );
}
