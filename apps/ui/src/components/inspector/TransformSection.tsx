import type { NodeDetail, Transform } from "../../engine/contract";
import { en } from "../../i18n/en";
import { format } from "../../i18n/en";
import { blendModeLabel, BLEND_MODES } from "../../i18n/labels";
import { formatLength } from "../../utils/number";
import { unionBounds } from "../../utils/tree";
import {
  AlignHorizontalJustifyCenter,
  AlignHorizontalJustifyEnd,
  AlignHorizontalJustifyStart,
  AlignVerticalDistributeCenter,
  AlignVerticalJustifyCenter,
  AlignVerticalJustifyEnd,
  AlignVerticalJustifyStart,
  AlignHorizontalDistributeCenter,
  BooleanExcludeGlyph,
  BooleanIntersectGlyph,
  BooleanSubtractGlyph,
  BooleanUnionGlyph,
  GroupGlyph,
  UngroupGlyph,
} from "../../icons";
import { runCommand } from "../../engine/commands";
import { NumberInput } from "../primitives/Fields";
import { Section } from "../primitives/Layout";
import { IconButton } from "../primitives/IconButton";
import { commonValue, usePatchSession } from "./shared";

export function TransformSection({
  details,
  node,
}: {
  details: NodeDetail[];
  node: NodeDetail | null;
}) {
  const session = usePatchSession("transform");

  if (node) {
    const t = node.transform;
    const patchTransform = (partial: Partial<Transform>, label: string) => {
      session.apply({ transform: { ...t, ...partial } }, { label });
    };
    const supportsRadius = ["rectangle", "frame", "vector", "component", "instance", "image", "boolean"].includes(node.kind);

    return (
      <Section id="transform" title={en.inspector.sectionTransform}>
        <div className="field-grid">
          <NumberInput
            ariaLabel={en.inspector.x}
            label={en.inspector.x}
            value={t.x}
            onChange={(value) => patchTransform({ x: value }, "x")}
            onCommit={() => session.end()}
            suffix={en.common.px}
            precision={2}
          />
          <NumberInput
            ariaLabel={en.inspector.y}
            label={en.inspector.y}
            value={t.y}
            onChange={(value) => patchTransform({ y: value }, "y")}
            onCommit={() => session.end()}
            suffix={en.common.px}
            precision={2}
          />
          <NumberInput
            ariaLabel={en.inspector.w}
            label={en.inspector.w}
            value={t.w}
            min={0}
            onChange={(value) => patchTransform({ w: value }, "w")}
            onCommit={() => session.end()}
            suffix={en.common.px}
            precision={2}
          />
          <NumberInput
            ariaLabel={en.inspector.h}
            label={en.inspector.h}
            value={t.h}
            min={0}
            onChange={(value) => patchTransform({ h: value }, "h")}
            onCommit={() => session.end()}
            suffix={en.common.px}
            precision={2}
          />
          <NumberInput
            ariaLabel={en.inspector.rotation}
            label={<span title={en.inspector.rotation}>↻</span>}
            value={t.rotation}
            step={1}
            precision={1}
            onChange={(value) => patchTransform({ rotation: value }, "rotation")}
            onCommit={() => session.end()}
            suffix="°"
          />
          <div className="field field--buttons">
            <IconButton
              label={en.inspector.flipX}
              size="sm"
              onClick={() => patchTransform({ flip_x: !t.flip_x }, "flip")}
            >
              <span style={{ fontSize: 10, fontWeight: 700 }}>⇄</span>
            </IconButton>
            <IconButton
              label={en.inspector.flipY}
              size="sm"
              onClick={() => patchTransform({ flip_y: !t.flip_y }, "flip")}
            >
              <span style={{ fontSize: 10, fontWeight: 700 }}>⇅</span>
            </IconButton>
          </div>
        </div>

        {supportsRadius && (
          <div className="field-grid">
            <NumberInput
              ariaLabel={en.inspector.cornerRadius}
              label={<span title={en.inspector.cornerRadius}>◜</span>}
              value={node.corner_radii.tl}
              min={0}
              onChange={(value) =>
                session.apply(
                  { corner_radii: { tl: value, tr: value, br: value, bl: value } },
                  { label: "corner_radius" },
                )
              }
              onCommit={() => session.end()}
              suffix={en.common.px}
            />
            <NumberInput
              ariaLabel={en.inspector.opacity}
              label={en.inspector.opacity}
              value={Math.round(node.opacity * 100)}
              min={0}
              max={100}
              precision={0}
              onChange={(value) => session.apply({ opacity: value / 100 }, { label: "opacity" })}
              onCommit={() => session.end()}
              suffix="%"
            />
          </div>
        )}

        <div className="inline-row">
          <span className="inline-label">{en.inspector.blendMode}</span>
          <select
            className="select-field"
            aria-label={en.inspector.blendMode}
            value={node.blend}
            onChange={(event) => session.apply({ blend: event.target.value }, { label: "blend", coalesce: false })}
          >
            {BLEND_MODES.map((mode) => (
              <option key={mode} value={mode}>
                {blendModeLabel(mode)}
              </option>
            ))}
          </select>
        </div>

        <div className="row-2" style={{ marginTop: 4 }}>
          <span className="field-suffix">{formatLength(t.w)} × {formatLength(t.h)}</span>
          <span className="field-suffix" style={{ textAlign: "right" }}>
            {node.absolute_bounds.x.toFixed(1)}, {node.absolute_bounds.y.toFixed(1)}
          </span>
        </div>
      </Section>
    );
  }

  // Multiple selection: union bounds + alignment / distribution / booleans.
  const bounds = unionBounds(details);
  return (
    <Section id="transform" title={en.inspector.sectionTransform}>
      {bounds ? (
        <div className="field-grid">
          <NumberInput
            ariaLabel={en.inspector.x}
            label={en.inspector.x}
            value={bounds.x}
            onChange={() => undefined}
            disabled
            suffix={en.common.px}
          />
          <NumberInput
            ariaLabel={en.inspector.y}
            label={en.inspector.y}
            value={bounds.y}
            onChange={() => undefined}
            disabled
            suffix={en.common.px}
          />
          <NumberInput
            ariaLabel={en.inspector.w}
            label={en.inspector.w}
            value={bounds.w}
            onChange={() => undefined}
            disabled
            suffix={en.common.px}
          />
          <NumberInput
            ariaLabel={en.inspector.h}
            label={en.inspector.h}
            value={bounds.h}
            onChange={() => undefined}
            disabled
            suffix={en.common.px}
          />
        </div>
      ) : null}
      <div className="inline-row" style={{ marginTop: 6 }}>
        <span className="inline-label">{en.inspector.align}</span>
        <div className="segmented">
          <IconButton label={en.inspector.alignLeft} size="sm" onClick={() => void runCommand("align", { align: "left" })}>
            <AlignHorizontalJustifyStart size={13} />
          </IconButton>
          <IconButton
            label={en.inspector.alignCenterH}
            size="sm"
            onClick={() => void runCommand("align", { align: "hcenter" })}
          >
            <AlignHorizontalJustifyCenter size={13} />
          </IconButton>
          <IconButton label={en.inspector.alignRight} size="sm" onClick={() => void runCommand("align", { align: "right" })}>
            <AlignHorizontalJustifyEnd size={13} />
          </IconButton>
          <IconButton label={en.inspector.alignTop} size="sm" onClick={() => void runCommand("align", { align: "top" })}>
            <AlignVerticalJustifyStart size={13} />
          </IconButton>
          <IconButton
            label={en.inspector.alignCenterV}
            size="sm"
            onClick={() => void runCommand("align", { align: "vcenter" })}
          >
            <AlignVerticalJustifyCenter size={13} />
          </IconButton>
          <IconButton
            label={en.inspector.alignBottom}
            size="sm"
            onClick={() => void runCommand("align", { align: "bottom" })}
          >
            <AlignVerticalJustifyEnd size={13} />
          </IconButton>
        </div>
      </div>
      <div className="inline-row">
        <span className="inline-label">{en.inspector.distribute}</span>
        <div className="segmented">
          <IconButton
            label={en.inspector.distributeH}
            size="sm"
            onClick={() => void runCommand("distribute", { axis: "horizontal", spacing: 0 })}
          >
            <AlignHorizontalDistributeCenter size={13} />
          </IconButton>
          <IconButton
            label={en.inspector.distributeV}
            size="sm"
            onClick={() => void runCommand("distribute", { axis: "vertical", spacing: 0 })}
          >
            <AlignVerticalDistributeCenter size={13} />
          </IconButton>
        </div>
      </div>
      <div className="inline-row">
        <span className="inline-label">{en.inspector.boolean}</span>
        <div className="segmented">
          <IconButton label={en.inspector.booleanUnion} size="sm" onClick={() => void runCommand("boolean", { op: "union" })}>
            <BooleanUnionGlyph width={13} height={13} />
          </IconButton>
          <IconButton
            label={en.inspector.booleanSubtract}
            size="sm"
            onClick={() => void runCommand("boolean", { op: "subtract" })}
          >
            <BooleanSubtractGlyph width={13} height={13} />
          </IconButton>
          <IconButton
            label={en.inspector.booleanIntersect}
            size="sm"
            onClick={() => void runCommand("boolean", { op: "intersect" })}
          >
            <BooleanIntersectGlyph width={13} height={13} />
          </IconButton>
          <IconButton
            label={en.inspector.booleanExclude}
            size="sm"
            onClick={() => void runCommand("boolean", { op: "exclude" })}
          >
            <BooleanExcludeGlyph width={13} height={13} />
          </IconButton>
        </div>
      </div>
      <div className="inline-row">
        <span className="inline-label">{en.menu.group}</span>
        <div className="segmented">
          <IconButton label={en.menu.group} size="sm" onClick={() => void runCommand("group")}>
            <GroupGlyph width={13} height={13} />
          </IconButton>
          <IconButton label={en.menu.ungroup} size="sm" onClick={() => void runCommand("ungroup")}>
            <UngroupGlyph width={13} height={13} />
          </IconButton>
        </div>
      </div>
      <div className="field-grid" style={{ marginTop: 6 }}>
        <NumberInput
          ariaLabel={en.inspector.opacity}
          label={en.inspector.opacity}
          value={(commonValue(details, (detail) => Math.round(detail.opacity * 100)).value ?? 100)}
          min={0}
          max={100}
          onChange={(value) => void runCommand("set_opacity", { opacity: value / 100 }, { quiet: true })}
          suffix="%"
        />
      </div>
      <p className="section-hint">{format(en.inspector.multiSelectionBody, { count: details.length })}</p>
    </Section>
  );
}
