//! Visual style of a node and the document's shared style registry.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::geometry::FillRule;
use crate::geometry::TextStyle;
use crate::ids::StyleId;
use crate::paint::{Effect, Fill, LineCap, LineJoin, Paint, Stroke, StrokeAlign};
use dp_geom::CornerRadii;

/// Everything needed to paint a node's surface.
///
/// Fills and strokes are stored bottom-first. `corner_radii` applies to
/// rectangles (and to frames/groups that clip content) while `fill_rule` is
/// used for vectors and boolean results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeStyle {
    /// Fill layers.
    #[serde(default)]
    pub fills: Vec<Fill>,
    /// Stroke layers.
    #[serde(default)]
    pub strokes: Vec<Stroke>,
    /// Effects (shadows and blurs).
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Per-corner radii for rectangles/frames.
    #[serde(default)]
    pub corner_radii: CornerRadii,
    /// Fill rule for vector/boolean geometry.
    #[serde(default)]
    pub fill_rule: FillRule,
    /// Alignment of strokes (shared by all stroke layers).
    #[serde(default)]
    pub stroke_align: StrokeAlign,
    /// Draw strokes inset into the shape bounds (Figma's "inside" preview).
    #[serde(default)]
    pub inset_stroke: bool,
}

impl NodeStyle {
    /// A style with one solid fill.
    pub fn filled(color: crate::color::Color) -> NodeStyle {
        NodeStyle {
            fills: vec![Fill::solid(color)],
            ..NodeStyle::default()
        }
    }

    /// A style with one solid fill and one solid stroke of the given width.
    pub fn outlined(
        fill: crate::color::Color,
        stroke: crate::color::Color,
        weight: f64,
    ) -> NodeStyle {
        let mut stroke = Stroke::solid(stroke);
        stroke.weight = weight;
        NodeStyle {
            fills: vec![Fill::solid(fill)],
            strokes: vec![stroke],
            ..NodeStyle::default()
        }
    }

    /// Copies the effective stroke settings shared between layers.
    pub fn set_stroke_shared(&mut self, align: StrokeAlign, cap: LineCap, join: LineJoin) {
        self.stroke_align = align;
        for stroke in &mut self.strokes {
            stroke.align = align;
            stroke.cap = cap;
            stroke.join = join;
        }
    }

    /// Returns `true` when nothing would be painted except children.
    pub fn is_visually_empty(&self) -> bool {
        self.fills
            .iter()
            .all(|fill| !fill.visible || fill.paint.is_invisible())
            && self.strokes.iter().all(|stroke| !stroke.visible)
            && self.effects.is_empty()
    }

    /// Whether any paint references the given style id.
    pub fn uses_style(&self, style: StyleId) -> bool {
        self.fills.iter().any(|fill| fill.style_id == Some(style))
            || self
                .strokes
                .iter()
                .any(|stroke| stroke.style_id == Some(style))
    }
}

/// A shared paint style that fills/strokes can follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedPaintStyle {
    /// Style id.
    pub id: StyleId,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// The paint.
    pub paint: Paint,
}

/// A shared text style referenced by text nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedTextStyle {
    /// Style id.
    pub id: StyleId,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// The character style.
    pub style: TextStyle,
}

/// A shared effect style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedEffectStyle {
    /// Style id.
    pub id: StyleId,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// Effect layers.
    pub effects: Vec<Effect>,
}

/// All shared styles of a document, keyed by id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleRegistry {
    /// Paint styles.
    #[serde(default)]
    pub paints: IndexMap<StyleId, SharedPaintStyle>,
    /// Text styles.
    #[serde(default)]
    pub texts: IndexMap<StyleId, SharedTextStyle>,
    /// Effect styles.
    #[serde(default)]
    pub effects: IndexMap<StyleId, SharedEffectStyle>,
}

impl StyleRegistry {
    /// Number of registered styles of all kinds.
    pub fn len(&self) -> usize {
        self.paints.len() + self.texts.len() + self.effects.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether an id resolves to any style kind.
    pub fn contains(&self, id: StyleId) -> bool {
        self.paints.contains_key(&id)
            || self.texts.contains_key(&id)
            || self.effects.contains_key(&id)
    }

    /// Looks up a paint style.
    pub fn paint(&self, id: StyleId) -> Option<&SharedPaintStyle> {
        self.paints.get(&id)
    }

    /// Looks up a text style.
    pub fn text(&self, id: StyleId) -> Option<&SharedTextStyle> {
        self.texts.get(&id)
    }

    /// Looks up an effect style.
    pub fn effect(&self, id: StyleId) -> Option<&SharedEffectStyle> {
        self.effects.get(&id)
    }

    /// Adds or replaces a paint style.
    pub fn upsert_paint(&mut self, style: SharedPaintStyle) {
        self.paints.insert(style.id, style);
    }

    /// Adds or replaces a text style.
    pub fn upsert_text(&mut self, style: SharedTextStyle) {
        self.texts.insert(style.id, style);
    }

    /// Adds or replaces an effect style.
    pub fn upsert_effect(&mut self, style: SharedEffectStyle) {
        self.effects.insert(style.id, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn style_defaults_are_serde_safe() {
        let style = NodeStyle::default();
        let json = serde_json::to_value(&style).unwrap();
        assert_eq!(json["fills"].as_array().unwrap().len(), 0);
        assert_eq!(json["fill_rule"], "nonzero");
        assert_eq!(json["inset_stroke"], false);
        let back: NodeStyle = serde_json::from_value(json).unwrap();
        assert_eq!(back, style);
    }

    #[test]
    fn style_registry_roundtrip() {
        let mut registry = StyleRegistry::default();
        let id = StyleId::new_v4();
        registry.upsert_paint(SharedPaintStyle {
            id,
            name: "Brand/Coral".into(),
            description: String::new(),
            paint: Paint::solid(Color::from_hex("#ff6b4a").unwrap()),
        });
        assert!(registry.contains(id));
        assert_eq!(registry.paint(id).unwrap().name, "Brand/Coral");
        let json = serde_json::to_string(&registry).unwrap();
        let back: StyleRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, registry);
    }

    #[test]
    fn outlined_style_helper() {
        let style = NodeStyle::outlined(Color::WHITE, Color::BLACK, 2.0);
        assert_eq!(style.fills.len(), 1);
        assert_eq!(style.strokes.len(), 1);
        assert_eq!(style.strokes[0].weight, 2.0);
        assert!(!style.is_visually_empty());
    }
}
