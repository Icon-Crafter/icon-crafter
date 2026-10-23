//! Geometry descriptions, auto layout, constraints and text content.
//!
//! These are the persisted, user-editable descriptions of a node. Resolved
//! values (absolute matrices, laid out children boxes, shaped glyph runs) are
//! computed by `dp-layout`, `dp-text` and the engine, never stored here.

use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::paint::Fill;
use dp_geom::{BooleanOp, FillRule as GeomFillRule, Insets, Point, Size, VectorPath};

/// Fill rule used by model-level geometry (serialized as `nonzero`/`evenodd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillRule {
    /// Non-zero winding rule.
    #[default]
    NonZero,
    /// Even-odd rule.
    EvenOdd,
}

impl From<FillRule> for GeomFillRule {
    fn from(value: FillRule) -> GeomFillRule {
        match value {
            FillRule::NonZero => GeomFillRule::NonZero,
            FillRule::EvenOdd => GeomFillRule::EvenOdd,
        }
    }
}

impl From<GeomFillRule> for FillRule {
    fn from(value: GeomFillRule) -> FillRule {
        match value {
            GeomFillRule::NonZero => FillRule::NonZero,
            GeomFillRule::EvenOdd => FillRule::EvenOdd,
        }
    }
}

/// Editable geometry of a shape node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeGeometry {
    /// An editable bezier path.
    Vector {
        /// The path itself.
        path: VectorPath,
    },
    /// A regular polygon.
    Polygon {
        /// Number of sides (clamped to `3..=512` when rendered).
        point_count: u32,
    },
    /// A star.
    Star {
        /// Number of points (clamped to `3..=512`).
        point_count: u32,
        /// Inner radius ratio (`0.05..=0.95`).
        inner_ratio: f64,
    },
    /// A boolean combination of the node's children.
    Boolean {
        /// The boolean operation.
        operation: BooleanOp,
        /// Fill rule used to interpret the child outlines.
        #[serde(default)]
        fill_rule: FillRule,
    },
}

impl NodeGeometry {
    /// Convenience: editable path geometry.
    pub fn vector(path: VectorPath) -> NodeGeometry {
        NodeGeometry::Vector { path }
    }

    /// Convenience: boolean geometry.
    pub fn boolean(operation: BooleanOp) -> NodeGeometry {
        NodeGeometry::Boolean {
            operation,
            fill_rule: FillRule::NonZero,
        }
    }
}

/// Main axis of an auto layout container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutDirection {
    /// Children laid out left to right.
    #[default]
    Horizontal,
    /// Children laid out top to bottom.
    Vertical,
}

/// Distribution of children along the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryAlign {
    /// Pack at the start.
    #[default]
    Start,
    /// Pack in the centre.
    Center,
    /// Pack at the end.
    End,
    /// Spread with equal gaps.
    SpaceBetween,
}

/// Alignment of children on the cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CounterAlign {
    /// Align at the start.
    #[default]
    Start,
    /// Align in the centre.
    Center,
    /// Align at the end.
    End,
    /// Stretch to fill the container.
    Stretch,
    /// Align first text baseline.
    Baseline,
}

/// How a container sizes itself or a child sizes along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeSpec {
    /// Use an explicit size.
    #[default]
    Fixed,
    /// Shrink to fit content.
    Hug,
    /// Fill the available space.
    Fill,
}

/// Horizontal/vertical sizing pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Sizing {
    /// Horizontal sizing.
    #[serde(default)]
    pub h: SizeSpec,
    /// Vertical sizing.
    #[serde(default)]
    pub v: SizeSpec,
}

impl Sizing {
    /// Hug both axes.
    pub const HUG: Sizing = Sizing {
        h: SizeSpec::Hug,
        v: SizeSpec::Hug,
    };
    /// Fill both axes.
    pub const FILL: Sizing = Sizing {
        h: SizeSpec::Fill,
        v: SizeSpec::Fill,
    };
}

/// Auto layout configuration of a container node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoLayout {
    /// Layout direction.
    #[serde(default)]
    pub direction: LayoutDirection,
    /// Whether children wrap onto new lines.
    #[serde(default)]
    pub wrap: bool,
    /// Gap between children (and between wrapped lines).
    #[serde(default)]
    pub gap: f64,
    /// Padding around content.
    #[serde(default)]
    pub padding: Insets,
    /// Main-axis distribution.
    #[serde(default)]
    pub primary_align: PrimaryAlign,
    /// Cross-axis alignment.
    #[serde(default)]
    pub counter_align: CounterAlign,
    /// Container sizing.
    #[serde(default)]
    pub sizing: Sizing,
    /// Minimum size when hugging/filling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_size: Option<Size>,
    /// Maximum size when hugging/filling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<Size>,
    /// Reverses the visual order of children.
    #[serde(default)]
    pub item_reverse: bool,
}

impl Default for AutoLayout {
    fn default() -> AutoLayout {
        AutoLayout {
            direction: LayoutDirection::Horizontal,
            wrap: false,
            gap: 0.0,
            padding: Insets::default(),
            primary_align: PrimaryAlign::Start,
            counter_align: CounterAlign::Start,
            sizing: Sizing::default(),
            min_size: None,
            max_size: None,
            item_reverse: false,
        }
    }
}

impl AutoLayout {
    /// A horizontal layout with a gap and uniform padding.
    pub fn horizontal(gap: f64, padding: f64) -> AutoLayout {
        AutoLayout {
            direction: LayoutDirection::Horizontal,
            gap,
            padding: Insets::uniform(padding),
            ..AutoLayout::default()
        }
    }

    /// A vertical layout with a gap and uniform padding.
    pub fn vertical(gap: f64, padding: f64) -> AutoLayout {
        AutoLayout {
            direction: LayoutDirection::Vertical,
            gap,
            padding: Insets::uniform(padding),
            ..AutoLayout::default()
        }
    }
}

/// Horizontal resize constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalConstraint {
    /// Pin the left edge.
    #[default]
    Left,
    /// Pin the right edge.
    Right,
    /// Pin both edges (the node stretches).
    LeftRight,
    /// Keep the horizontal centre.
    Center,
    /// Scale proportionally.
    Scale,
}

/// Vertical resize constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalConstraint {
    /// Pin the top edge.
    #[default]
    Top,
    /// Pin the bottom edge.
    Bottom,
    /// Pin both edges (the node stretches).
    TopBottom,
    /// Keep the vertical centre.
    Center,
    /// Scale proportionally.
    Scale,
}

/// Constraints of a node inside a non auto-layout container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Constraints {
    /// Horizontal constraint.
    #[serde(default)]
    pub horizontal: HorizontalConstraint,
    /// Vertical constraint.
    #[serde(default)]
    pub vertical: VerticalConstraint,
}

/// Unit of a line height value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineHeightUnit {
    /// Pixels.
    Px,
    /// Automatic (font default).
    #[default]
    Auto,
    /// Percentage of the font size.
    Percent,
}

/// Line height setting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineHeight {
    /// Unit.
    #[serde(default)]
    pub unit: LineHeightUnit,
    /// Value interpreted according to `unit`.
    #[serde(default)]
    pub value: f64,
}

impl Default for LineHeight {
    fn default() -> LineHeight {
        LineHeight {
            unit: LineHeightUnit::Auto,
            value: 0.0,
        }
    }
}

impl LineHeight {
    /// Pixels line height.
    pub fn px(value: f64) -> LineHeight {
        LineHeight {
            unit: LineHeightUnit::Px,
            value,
        }
    }

    /// Percentage line height.
    pub fn percent(value: f64) -> LineHeight {
        LineHeight {
            unit: LineHeightUnit::Percent,
            value,
        }
    }

    /// Resolved line height in pixels for a font size.
    pub fn resolve(self, font_size: f64) -> Option<f64> {
        match self.unit {
            LineHeightUnit::Auto => None,
            LineHeightUnit::Px => Some(self.value),
            LineHeightUnit::Percent => Some(font_size * self.value / 100.0),
        }
    }
}

/// Letter case transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextCase {
    /// Keep the typed characters.
    #[default]
    Original,
    /// Uppercase.
    Upper,
    /// Lowercase.
    Lower,
    /// Title case.
    Title,
}

/// Text decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextDecoration {
    /// No decoration.
    #[default]
    None,
    /// Underline.
    Underline,
    /// Strikethrough.
    Strikethrough,
}

/// Horizontal paragraph alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    /// Left.
    #[default]
    Left,
    /// Centred.
    Center,
    /// Right.
    Right,
    /// Justified.
    Justified,
}

/// Vertical alignment inside a fixed-height text box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    /// Top.
    #[default]
    Top,
    /// Middle.
    Center,
    /// Bottom.
    Bottom,
}

/// How a text box sizes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoSize {
    /// Fixed width and height.
    #[default]
    None,
    /// Fixed width, height grows with content.
    Height,
    /// Both dimensions hug content.
    WidthAndHeight,
}

/// Which side of a path the text sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathSide {
    /// Left side of the direction of travel.
    #[default]
    Left,
    /// Right side.
    Right,
}

/// Text attached to a vector path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextOnPath {
    /// The path node the text follows.
    pub path_node_id: NodeId,
    /// Offset along the path in document units.
    #[serde(default)]
    pub start_offset: f64,
    /// Which side of the path is used.
    #[serde(default)]
    pub side: PathSide,
}

/// Character style of a text node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Font family name.
    #[serde(default = "default_font_family")]
    pub font_family: String,
    /// Font weight (`100..=900`).
    #[serde(default = "default_font_weight")]
    pub font_weight: u16,
    /// Italic style.
    #[serde(default)]
    pub italic: bool,
    /// Font size in document units.
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// Line height.
    #[serde(default)]
    pub line_height: LineHeight,
    /// Letter spacing in document units.
    #[serde(default)]
    pub letter_spacing: f64,
    /// Case transformation.
    #[serde(default)]
    pub text_case: TextCase,
    /// Decoration.
    #[serde(default)]
    pub decoration: TextDecoration,
    /// Text fills (bottom-first).
    #[serde(default)]
    pub fills: Vec<Fill>,
}

fn default_font_family() -> String {
    "Inter".to_string()
}

fn default_font_weight() -> u16 {
    400
}

fn default_font_size() -> f64 {
    14.0
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle {
            font_family: default_font_family(),
            font_weight: 400,
            italic: false,
            font_size: 14.0,
            line_height: LineHeight::default(),
            letter_spacing: 0.0,
            text_case: TextCase::default(),
            decoration: TextDecoration::default(),
            fills: vec![Fill::solid(crate::color::Color::BLACK)],
        }
    }
}

/// Paragraph level text settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextParagraph {
    /// Horizontal alignment.
    #[serde(default)]
    pub align: TextAlign,
    /// Vertical alignment in a fixed box.
    #[serde(default)]
    pub vertical_align: VerticalAlign,
    /// Maximum number of lines before truncation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<u32>,
    /// Extra space between paragraphs.
    #[serde(default)]
    pub paragraph_spacing: f64,
}

impl Default for TextParagraph {
    fn default() -> TextParagraph {
        TextParagraph {
            align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            max_lines: None,
            paragraph_spacing: 0.0,
        }
    }
}

/// Editable text content of a text node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    /// The raw characters.
    #[serde(default)]
    pub characters: String,
    /// Character style.
    #[serde(default)]
    pub style: TextStyle,
    /// Paragraph style.
    #[serde(default)]
    pub paragraph: TextParagraph,
    /// Auto sizing behaviour.
    #[serde(default)]
    pub auto_size: AutoSize,
    /// Text on path attachment, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_path: Option<TextOnPath>,
}

impl TextContent {
    /// A plain text node content.
    pub fn plain(characters: impl Into<String>) -> TextContent {
        TextContent {
            characters: characters.into(),
            style: TextStyle::default(),
            paragraph: TextParagraph::default(),
            auto_size: AutoSize::WidthAndHeight,
            on_path: None,
        }
    }

    /// Length in characters.
    pub fn len(&self) -> usize {
        self.characters.chars().count()
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }

    /// Clamps the caret index into `0..=len`.
    pub fn clamp_caret(&self, caret: usize) -> usize {
        caret.min(self.len())
    }
}

/// A world-space point used by text on path measurement.
pub type LocalPoint = Point;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_serde_shapes() {
        let geometry = NodeGeometry::Star {
            point_count: 5,
            inner_ratio: 0.38,
        };
        let json = serde_json::to_value(&geometry).unwrap();
        assert_eq!(json["type"], "star");
        assert_eq!(json["point_count"], 5);

        let path = VectorPath::new();
        let json = serde_json::to_value(NodeGeometry::vector(path)).unwrap();
        assert_eq!(json["type"], "vector");
    }

    #[test]
    fn fill_rule_serde_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&FillRule::NonZero).unwrap(),
            "\"nonzero\""
        );
        assert_eq!(
            serde_json::to_string(&FillRule::EvenOdd).unwrap(),
            "\"evenodd\""
        );
    }

    #[test]
    fn auto_layout_defaults_and_builders() {
        let layout = AutoLayout::vertical(8.0, 16.0);
        assert_eq!(layout.direction, LayoutDirection::Vertical);
        assert_eq!(layout.gap, 8.0);
        assert_eq!(layout.padding.top, 16.0);
        assert_eq!(layout.sizing, Sizing::default());
    }

    #[test]
    fn text_content_helpers() {
        let text = TextContent::plain("Hello");
        assert_eq!(text.len(), 5);
        assert_eq!(text.clamp_caret(99), 5);
        // Multi-byte characters count as one caret position.
        let unicode = TextContent::plain("héllo 🎨");
        assert_eq!(unicode.len(), 7);
    }

    #[test]
    fn line_height_resolution() {
        assert_eq!(LineHeight::px(20.0).resolve(14.0), Some(20.0));
        let percent = LineHeight::percent(150.0);
        assert!((percent.resolve(20.0).unwrap() - 30.0).abs() < 1e-9);
        assert_eq!(LineHeight::default().resolve(14.0), None);
    }
}
