//! Developer inspection: measurements and resolved style data for handoff.
//!
//! The inspection snapshot is the single source of truth for both the Dev Mode
//! panel and the code generators, so what a developer copies is exactly what the
//! code generator emits.

use dp_layout::LayoutMap;
use dp_model::{Color, Document, Node, NodeId, PageId};
use serde::{Deserialize, Serialize};

/// One gradient stop, resolved for display.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStopInfo {
    /// Normalised position `0..=1`.
    pub position: f64,
    /// Stop colour.
    pub color: Color,
    /// Hex representation.
    pub hex: String,
}

/// A fill paint resolved for handoff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintInfo {
    /// Paint kind: solid, linear, radial, angular, image.
    pub kind: String,
    /// Solid colour when applicable.
    pub color: Option<Color>,
    /// Hex string for solid colours.
    pub hex: Option<String>,
    /// Gradient stops when applicable.
    pub stops: Vec<GradientStopInfo>,
    /// Paint-level opacity.
    pub opacity: f64,
    /// Blend mode.
    pub blend: String,
    /// Shared style id, when the paint comes from a style.
    pub style_id: Option<String>,
    /// Variable id, when the paint is bound to a variable.
    pub variable_id: Option<String>,
    /// Variable name for UI display, when resolvable.
    pub variable_name: Option<String>,
}

/// One stroke resolved for handoff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeInfo {
    /// Stroke paint.
    pub paint: PaintInfo,
    /// Stroke weight in px.
    pub weight: f64,
    /// Alignment: center, inside, outside.
    pub align: String,
    /// Line cap.
    pub cap: String,
    /// Line join.
    pub join: String,
    /// Miter limit.
    pub miter_limit: f64,
    /// Dash pattern.
    pub dash: Option<Vec<f64>>,
}

/// One effect resolved for handoff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectInfo {
    /// Effect kind: drop_shadow, inner_shadow, layer_blur, background_blur.
    pub kind: String,
    /// Shadow colour.
    pub color: Option<Color>,
    /// Shadow offset.
    pub offset: Option<[f64; 2]>,
    /// Blur radius.
    pub blur: f64,
    /// Spread radius.
    pub spread: f64,
    /// Blend mode.
    pub blend: Option<String>,
}

/// Resolved typography.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextInfo {
    /// Text characters.
    pub characters: String,
    /// Font family requested by the design.
    pub font_family: String,
    /// Font size in px.
    pub font_size: f64,
    /// Font weight.
    pub font_weight: u16,
    /// Italic flag.
    pub italic: bool,
    /// Resolved line height in px (or the size × 1.2 fallback).
    pub line_height: f64,
    /// Letter spacing in px.
    pub letter_spacing: f64,
    /// Text alignment.
    pub align: String,
    /// Vertical alignment.
    pub vertical_align: String,
    /// Text colour (first resolved fill).
    pub color: Option<Color>,
    /// Whether the text overflows its resolved box.
    pub overflow: bool,
}

/// Component relationship of a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Role: component, component_set, instance.
    pub role: String,
    /// Component id for instances.
    pub component_id: Option<String>,
    /// Component display name for instances.
    pub component_name: Option<String>,
    /// How many overrides the instance carries.
    pub override_count: usize,
    /// Whether the node lives inside an instance subtree.
    pub inside_instance: bool,
}

/// One inspected node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeInspect {
    /// Node id as a hyphenated uuid.
    pub id: String,
    /// Layer name.
    pub name: String,
    /// Node kind (serde tag value).
    pub kind: String,
    /// Owning page.
    pub page: String,
    /// Parent node id.
    pub parent: Option<String>,
    /// Child node ids in paint order.
    pub children: Vec<String>,
    /// Page-space axis-aligned bounds.
    pub bounds: [f64; 4],
    /// Size after layout.
    pub size: [f64; 2],
    /// Position relative to the parent.
    pub local_position: [f64; 2],
    /// Rotation in degrees.
    pub rotation: f64,
    /// Flip flags.
    pub flip: [bool; 2],
    /// Effective opacity.
    pub opacity: f64,
    /// Blend mode.
    pub blend: String,
    /// Visibility.
    pub visible: bool,
    /// Lock state.
    pub locked: bool,
    /// Absolute matrix `[m11, m12, m21, m22, tx, ty]`.
    pub matrix: [f64; 6],
    /// Fill list.
    pub fills: Vec<PaintInfo>,
    /// Stroke list.
    pub strokes: Vec<StrokeInfo>,
    /// Effect list.
    pub effects: Vec<EffectInfo>,
    /// Corner radii `[tl, tr, br, bl]`.
    pub corner_radii: [f64; 4],
    /// Auto-layout direction (horizontal / vertical) when present.
    pub layout_direction: Option<String>,
    /// Auto-layout gap.
    pub layout_gap: Option<f64>,
    /// Auto-layout padding `[top, right, bottom, left]`.
    pub layout_padding: Option<[f64; 4]>,
    /// Auto-layout sizing as `(h, v)`.
    pub layout_sizing: Option<[String; 2]>,
    /// Constraints as `(horizontal, vertical)`.
    pub constraints: Option<[String; 2]>,
    /// Typography for text nodes.
    pub text: Option<TextInfo>,
    /// Component information.
    pub component: Option<ComponentInfo>,
    /// Variable references used by this node's paints.
    pub variable_refs: Vec<String>,
    /// Shared style references used by this node.
    pub style_refs: Vec<String>,
}

/// Entire inspection result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InspectSnapshot {
    /// Inspected nodes in selection order.
    pub nodes: Vec<NodeInspect>,
    /// Union bounds of the selection in page space.
    pub bounds: Option<[f64; 4]>,
    /// Sum of the inspected node sizes (useful for distribution checks).
    pub total_size: [f64; 2],
    /// Non-fatal problems found while inspecting.
    pub warnings: Vec<String>,
}

impl InspectSnapshot {
    /// Number of inspected nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether nothing was inspected.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Inspects `ids`, resolving layout, variables and styles against `document`.
pub fn inspect(document: &Document, layout: &LayoutMap, ids: &[NodeId]) -> InspectSnapshot {
    let mut nodes = Vec::new();
    let mut warnings = Vec::new();
    let mut bounds: Option<[f64; 4]> = None;
    let mut total_size = [0.0, 0.0];
    for id in ids {
        let Some((page, node)) = document.locate_node(*id) else {
            warnings.push(format!("node {id} is not part of this document"));
            continue;
        };
        let Some(node_layout) = layout.get(*id) else {
            warnings.push(format!(
                "node {id} has no layout entry; layout the page before inspecting"
            ));
            continue;
        };
        let inspect = inspect_node(document, page.id, node, node_layout);
        let [x, y, w, h] = inspect.bounds;
        total_size[0] += w;
        total_size[1] += h;
        bounds = Some(match bounds {
            None => [x, y, w, h],
            Some([bx, by, bw, bh]) => {
                let right = (bx + bw).max(x + w);
                let bottom = (by + bh).max(y + h);
                let left = bx.min(x);
                let top = by.min(y);
                [left, top, right - left, bottom - top]
            }
        });
        nodes.push(inspect);
    }
    InspectSnapshot {
        nodes,
        bounds,
        total_size,
        warnings,
    }
}

/// Inspects one node.
pub fn inspect_node(
    document: &Document,
    page: PageId,
    node: &Node,
    layout: &dp_layout::NodeLayout,
) -> NodeInspect {
    let mode = document_mode(document);
    let mut variable_refs = Vec::new();
    let mut style_refs = Vec::new();
    let mut fills = Vec::new();
    for fill in &node.style.fills {
        if fill.visible {
            fills.push(paint_info(
                document,
                mode,
                &fill.paint,
                fill.opacity,
                fill.style_id,
                fill.variable_id,
                &mut variable_refs,
                &mut style_refs,
            ));
        }
    }
    let mut strokes = Vec::new();
    for stroke in &node.style.strokes {
        if stroke.visible {
            let paint = paint_info(
                document,
                mode,
                &stroke.paint,
                stroke.opacity,
                stroke.style_id,
                stroke.variable_id,
                &mut variable_refs,
                &mut style_refs,
            );
            strokes.push(StrokeInfo {
                paint,
                weight: stroke.weight,
                align: serde_name(stroke.align),
                cap: serde_name(stroke.cap),
                join: serde_name(stroke.join),
                miter_limit: stroke.miter_limit,
                dash: stroke.dash.clone(),
            });
        }
    }
    let effects = node
        .style
        .effects
        .iter()
        .map(|effect| effect_info(document, mode, effect))
        .collect();
    let text = node
        .text
        .as_ref()
        .map(|content| text_info(content, &node.style.fills));
    let component = node.component.as_ref().map(|role| {
        let value = serde_json::to_value(role).unwrap_or(serde_json::Value::Null);
        let kind = value["type"].as_str().unwrap_or("unknown").to_owned();
        let component_id = value["component_id"].as_str().map(str::to_owned);
        let component_name = component_id
            .as_deref()
            .and_then(|id| id.parse::<dp_model::ComponentId>().ok())
            .and_then(|id| document.components.components.get(&id))
            .map(|meta| meta.name.clone());
        let override_count = value["overrides"]
            .as_array()
            .map(|overrides| overrides.len())
            .unwrap_or(0);
        ComponentInfo {
            role: kind,
            component_id,
            component_name,
            override_count,
            inside_instance: inside_instance(document, node),
        }
    });
    let (layout_direction, layout_gap, layout_padding, layout_sizing) = match &node.layout {
        Some(auto) => (
            Some(serde_name(auto.direction)),
            Some(auto.gap),
            Some([
                auto.padding.top,
                auto.padding.right,
                auto.padding.bottom,
                auto.padding.left,
            ]),
            Some([serde_name(auto.sizing.h), serde_name(auto.sizing.v)]),
        ),
        None => (None, None, None, None),
    };
    let constraints = node.constraints.as_ref().map(|constraints| {
        [
            serde_name(constraints.horizontal),
            serde_name(constraints.vertical),
        ]
    });
    NodeInspect {
        id: node.id.to_string(),
        name: node.name.clone(),
        kind: serde_name(node.kind),
        page: page.to_string(),
        parent: node.parent.map(|parent| parent.to_string()),
        children: node.children.iter().map(ToString::to_string).collect(),
        bounds: [
            layout.world.x,
            layout.world.y,
            layout.world.w,
            layout.world.h,
        ],
        size: [layout.size.w, layout.size.h],
        local_position: [layout.local.x, layout.local.y],
        rotation: node.transform.rotation,
        flip: [node.transform.flip_x, node.transform.flip_y],
        opacity: node.opacity,
        blend: serde_name(node.blend),
        visible: node.visible,
        locked: node.locked,
        matrix: [
            layout.matrix.m11,
            layout.matrix.m12,
            layout.matrix.m21,
            layout.matrix.m22,
            layout.matrix.tx,
            layout.matrix.ty,
        ],
        fills,
        strokes,
        effects,
        corner_radii: [
            node.style.corner_radii.tl,
            node.style.corner_radii.tr,
            node.style.corner_radii.br,
            node.style.corner_radii.bl,
        ],
        layout_direction,
        layout_gap,
        layout_padding,
        layout_sizing,
        constraints,
        text,
        component,
        variable_refs,
        style_refs,
    }
}

fn inside_instance(document: &Document, node: &Node) -> bool {
    let mut current = node.parent;
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 256 {
            return false;
        }
        let Some((_, current_node)) = document.locate_node(id) else {
            return false;
        };
        if matches!(current_node.kind, dp_model::NodeKind::Instance) {
            return true;
        }
        current = current_node.parent;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn paint_info(
    document: &Document,
    mode: dp_model::ModeId,
    paint: &dp_model::Paint,
    opacity: f64,
    style_id: Option<dp_model::StyleId>,
    variable_id: Option<dp_model::VariableId>,
    variable_refs: &mut Vec<String>,
    style_refs: &mut Vec<String>,
) -> PaintInfo {
    if let Some(style_id) = style_id {
        style_refs.push(style_id.to_string());
    }
    let mut resolved = serde_json::to_value(paint).unwrap_or(serde_json::Value::Null);
    let kind = resolved["type"].as_str().unwrap_or("unknown").to_owned();
    if let Some(variable_id) = variable_id {
        variable_refs.push(variable_id.to_string());
        if let Some(color) = document.variables.resolve_color(variable_id, mode) {
            resolved = serde_json::json!({ "type": "solid", "color": color });
        }
    }
    if kind == "solid" {
        let color = serde_json::from_value::<Color>(resolved["color"].clone()).ok();
        PaintInfo {
            kind,
            color,
            hex: color.map(|color| color.to_hex()),
            stops: Vec::new(),
            opacity,
            blend: String::new(),
            style_id: style_id.map(|id| id.to_string()),
            variable_id: variable_id.map(|id| id.to_string()),
            variable_name: variable_id.and_then(|id| {
                document
                    .variables
                    .variables
                    .get(&id)
                    .map(|variable| variable.name.clone())
            }),
        }
    } else {
        let stops = resolved["stops"]
            .as_array()
            .map(|stops| {
                stops
                    .iter()
                    .filter_map(|stop| {
                        let position = stop["position"].as_f64()?;
                        let color = serde_json::from_value::<Color>(stop["color"].clone()).ok()?;
                        Some(GradientStopInfo {
                            position,
                            color,
                            hex: color.to_hex(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        PaintInfo {
            kind,
            color: None,
            hex: None,
            stops,
            opacity,
            blend: String::new(),
            style_id: style_id.map(|id| id.to_string()),
            variable_id: variable_id.map(|id| id.to_string()),
            variable_name: variable_id.and_then(|id| {
                document
                    .variables
                    .variables
                    .get(&id)
                    .map(|variable| variable.name.clone())
            }),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn effect_info(
    document: &Document,
    mode: dp_model::ModeId,
    effect: &dp_model::Effect,
) -> EffectInfo {
    let _ = (document, mode);
    let value = serde_json::to_value(effect).unwrap_or(serde_json::Value::Null);
    let kind = value["type"].as_str().unwrap_or("unknown").to_owned();
    let color = serde_json::from_value::<Color>(value["color"].clone()).ok();
    let offset = value["offset"]
        .as_object()
        .and_then(|offset| Some([offset.get("x")?.as_f64()?, offset.get("y")?.as_f64()?]));
    let blur = value["blur"]
        .as_f64()
        .or_else(|| value["radius"].as_f64())
        .unwrap_or(0.0);
    let spread = value["spread"].as_f64().unwrap_or(0.0);
    let blend = value["blend"].as_str().map(str::to_owned);
    EffectInfo {
        kind,
        color,
        offset,
        blur,
        spread,
        blend,
    }
}

fn text_info(content: &dp_model::TextContent, fallback_fills: &[dp_model::Fill]) -> TextInfo {
    let style = &content.style;
    let line_height = style
        .line_height
        .resolve(style.font_size)
        .unwrap_or(style.font_size * 1.2);
    let color = style
        .fills
        .iter()
        .find(|fill| fill.visible)
        .and_then(|fill| serde_json::to_value(&fill.paint).ok())
        .and_then(|paint| serde_json::from_value::<Color>(paint["color"].clone()).ok())
        .or_else(|| {
            fallback_fills
                .iter()
                .find(|fill| fill.visible)
                .and_then(|fill| serde_json::to_value(&fill.paint).ok())
                .and_then(|paint| serde_json::from_value::<Color>(paint["color"].clone()).ok())
        });
    TextInfo {
        characters: content.characters.clone(),
        font_family: style.font_family.clone(),
        font_size: style.font_size,
        font_weight: style.font_weight,
        italic: style.italic,
        line_height,
        letter_spacing: style.letter_spacing,
        align: serde_name(content.paragraph.align),
        vertical_align: serde_name(content.paragraph.vertical_align),
        color,
        overflow: false,
    }
}

/// Document-level active mode used to resolve variables.
pub fn document_mode(document: &Document) -> dp_model::ModeId {
    document
        .settings
        .active_modes
        .values()
        .next()
        .copied()
        .unwrap_or_else(dp_model::ModeId::new_v4)
}

fn serde_name<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_layout::LayoutEngine;
    use dp_model::{Node, Transform};

    fn document() -> Document {
        let mut document = Document::new("Inspect");
        let page_id = document.active_page_id();
        let page = document.page_mut(page_id).expect("page");
        let mut frame = Node::frame("Card", 20.0, 30.0, 320.0, 180.0);
        frame.style.fills = vec![dp_model::Fill::solid(Color::from_hex("#ff6b4a").unwrap())];
        frame.style.corner_radii = dp_geom::CornerRadii::uniform(12.0);
        let frame_id = frame.id;
        page.insert_node(frame, None, None).unwrap();
        let mut text = Node::text("Title", "Hello", 40.0, 50.0);
        if let Some(content) = text.text.as_mut() {
            content.style.fills = vec![dp_model::Fill::solid(Color::WHITE)];
            content.style.font_size = 24.0;
            content.style.font_weight = 600;
        }
        page.insert_node(text, Some(frame_id), None).unwrap();
        document
    }

    fn layout(document: &Document) -> LayoutMap {
        let mut engine = LayoutEngine::new();
        engine.compute_page(document.active_page())
    }

    #[test]
    fn inspects_bounds_and_style() {
        let document = document();
        let layout = layout(&document);
        let ids: Vec<NodeId> = document.active_page().roots.clone();
        let snapshot = inspect(&document, &layout, &ids);
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.warnings.is_empty());
        let node = &snapshot.nodes[0];
        assert_eq!(node.name, "Card");
        assert_eq!(node.kind, "frame");
        assert_eq!(node.fills.len(), 1);
        assert_eq!(node.fills[0].hex.as_deref(), Some("#ff6b4a"));
        assert_eq!(node.corner_radii, [12.0, 12.0, 12.0, 12.0]);
        let [x, y, w, h] = node.bounds;
        assert!((x - 20.0).abs() < 0.01);
        assert!((y - 30.0).abs() < 0.01);
        assert!((w - 320.0).abs() < 0.01);
        assert!((h - 180.0).abs() < 0.01);
    }

    #[test]
    fn inspects_text_typography_and_world_bounds() {
        let document = document();
        let layout = layout(&document);
        let text_id = document.pages[0]
            .nodes
            .values()
            .find(|node| node.name == "Title")
            .unwrap()
            .id;
        let snapshot = inspect(&document, &layout, &[text_id]);
        let node = &snapshot.nodes[0];
        let text = node.text.as_ref().expect("text info");
        assert_eq!(text.characters, "Hello");
        assert_eq!(text.font_size, 24.0);
        assert_eq!(text.font_weight, 600);
        assert_eq!(
            text.color.map(|color| color.to_hex()).as_deref(),
            Some("#ffffff")
        );
        // world position is inside the card
        assert!(node.bounds[0] >= 20.0);
    }

    #[test]
    fn unknown_nodes_are_reported() {
        let document = document();
        let layout = layout(&document);
        let snapshot = inspect(&document, &layout, &[NodeId::new_v4()]);
        assert!(snapshot.is_empty());
        assert!(!snapshot.warnings.is_empty());
    }

    #[test]
    fn transform_rotation_is_reported() {
        let mut document = document();
        let page_id = document.active_page_id();
        let node_id = document.active_page().roots[0];
        let page = document.page_mut(page_id).expect("page");
        page.nodes.get_mut(&node_id).unwrap().transform = Transform {
            rotation: 15.0,
            ..Transform::new(20.0, 30.0, 320.0, 180.0)
        };
        let layout = layout(&document);
        let snapshot = inspect(&document, &layout, &[node_id]);
        assert!((snapshot.nodes[0].rotation - 15.0).abs() < 0.01);
    }
}
