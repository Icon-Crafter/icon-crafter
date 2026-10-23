//! Fills, strokes, gradients and effects.
//!
//! The paint model follows Figma's proven concept: a paint describes *how* to
//! colour a region, while a [`Fill`] wraps it with visibility, opacity, blend
//! mode and optional style/variable links. Paint order is bottom-first inside a
//! vector.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::ids::{AssetId, StyleId, VariableId};
use dp_geom::Point;

/// How a layer is composited with what is below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
    /// Normal source-over blending.
    #[default]
    Normal,
    /// Multiply.
    Multiply,
    /// Screen.
    Screen,
    /// Overlay.
    Overlay,
    /// Darken.
    Darken,
    /// Lighten.
    Lighten,
    /// Colour dodge.
    ColorDodge,
    /// Colour burn.
    ColorBurn,
    /// Hard light.
    HardLight,
    /// Soft light.
    SoftLight,
    /// Difference.
    Difference,
    /// Exclusion.
    Exclusion,
    /// Hue.
    Hue,
    /// Saturation.
    Saturation,
    /// Color.
    Color,
    /// Luminosity.
    Luminosity,
    /// Group pass-through (groups only).
    PassThrough,
}

impl BlendMode {
    /// Whether this blend mode is only meaningful on containers.
    pub fn is_container_only(self) -> bool {
        matches!(self, BlendMode::PassThrough)
    }

    /// Stable identifier used by export formats.
    pub fn as_css(self) -> &'static str {
        match self {
            BlendMode::Normal | BlendMode::PassThrough => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::HardLight => "hard-light",
            BlendMode::SoftLight => "soft-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
        }
    }
}

/// One colour stop of a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient, `0.0..=1.0`.
    pub position: f64,
    /// Stop colour.
    pub color: Color,
}

impl GradientStop {
    /// Creates a stop.
    pub fn new(position: f64, color: Color) -> GradientStop {
        GradientStop {
            position: position.clamp(0.0, 1.0),
            color,
        }
    }
}

/// How an image is fitted into its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    /// Stretch to fill the box.
    #[default]
    Fill,
    /// Contain, preserving aspect ratio.
    Fit,
    /// Cover the box, cropping the overflow.
    Crop,
    /// Repeat the image.
    Tile,
}

/// A paint source.
///
/// Colours are plain sRGB values. Gradient coordinates are relative to the
/// painted node's local box (`0.0..=1.0` on both axes), so resizing a shape
/// keeps the gradient aligned with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Paint {
    /// A flat colour.
    Solid {
        /// The colour.
        color: Color,
    },
    /// A linear gradient.
    Linear {
        /// Colour stops, sorted by position when used.
        stops: Vec<GradientStop>,
        /// Start point in `0..=1` local coordinates.
        from: Point,
        /// End point in `0..=1` local coordinates.
        to: Point,
        /// Optional affine transform (2 rows of `[a, b, c]`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<[[f64; 3]; 2]>,
    },
    /// A radial gradient.
    Radial {
        /// Colour stops.
        stops: Vec<GradientStop>,
        /// Centre in `0..=1` local coordinates.
        center: Point,
        /// Radius relative to the box extent.
        radius: f64,
        /// Optional affine transform.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<[[f64; 3]; 2]>,
    },
    /// A conic (angular) gradient.
    Angular {
        /// Colour stops.
        stops: Vec<GradientStop>,
        /// Centre in `0..=1` local coordinates.
        center: Point,
        /// Rotation in degrees.
        angle: f64,
        /// Optional affine transform.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<[[f64; 3]; 2]>,
    },
    /// An image fill referencing an uploaded asset.
    Image {
        /// The image asset.
        asset_id: AssetId,
        /// How the image fits its box.
        #[serde(default)]
        fit: ImageFit,
        /// Optional affine transform (crop/rotation).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<[[f64; 3]; 2]>,
    },
}

impl Paint {
    /// Opaque solidcolour helper.
    pub fn solid(color: Color) -> Paint {
        Paint::Solid { color }
    }

    /// Whether the paint has no visible contribution.
    pub fn is_invisible(&self) -> bool {
        match self {
            Paint::Solid { color } => color.a <= 0.0,
            Paint::Linear { stops, .. }
            | Paint::Radial { stops, .. }
            | Paint::Angular { stops, .. } => stops.iter().all(|stop| stop.color.a <= 0.0),
            Paint::Image { .. } => false,
        }
    }

    /// Sorted gradient stops (a copy; stops are cheap).
    pub fn sorted_stops(&self) -> Vec<GradientStop> {
        let stops = match self {
            Paint::Linear { stops, .. }
            | Paint::Radial { stops, .. }
            | Paint::Angular { stops, .. } => stops.clone(),
            _ => return Vec::new(),
        };
        let mut sorted = stops;
        sorted.sort_by(|a, b| a.position.total_cmp(&b.position));
        sorted
    }

    /// References to every asset this paint needs.
    pub fn asset_ids(&self) -> Vec<AssetId> {
        match self {
            Paint::Image { asset_id, .. } => vec![*asset_id],
            _ => Vec::new(),
        }
    }
}

/// A fill layer of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// Whether this fill is painted.
    #[serde(default = "crate::truth")]
    pub visible: bool,
    /// Fill opacity, multiplied with the paint colour alpha.
    #[serde(default = "crate::one")]
    pub opacity: f64,
    /// Blend mode for this fill.
    #[serde(default)]
    pub blend: BlendMode,
    /// The paint source.
    pub paint: Paint,
    /// Shared paint style this fill follows, if any.
    #[serde(default)]
    pub style_id: Option<StyleId>,
    /// Bound variable, if any.
    #[serde(default)]
    pub variable_id: Option<VariableId>,
}

impl Fill {
    /// A simple visible solid fill.
    pub fn solid(color: Color) -> Fill {
        Fill {
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            paint: Paint::solid(color),
            style_id: None,
            variable_id: None,
        }
    }

    /// Effective colour for a solid fill (ignores style/variable resolution).
    pub fn solid_color(&self) -> Option<Color> {
        match &self.paint {
            Paint::Solid { color } => Some(color.mul_alpha(self.opacity)),
            _ => None,
        }
    }
}

impl Default for Fill {
    fn default() -> Fill {
        Fill::solid(Color::WHITE)
    }
}

/// Stroke alignment relative to the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeAlign {
    /// Centred on the path.
    #[default]
    Center,
    /// Inside the path.
    Inside,
    /// Outside the path.
    Outside,
}

impl StrokeAlign {
    /// CSS `stroke-alignment` counterpart used by the SVG exporter.
    pub fn as_str(self) -> &'static str {
        match self {
            StrokeAlign::Center => "center",
            StrokeAlign::Inside => "inside",
            StrokeAlign::Outside => "outside",
        }
    }
}

/// Line cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineCap {
    /// Flat cap.
    #[default]
    Butt,
    /// Round cap.
    Round,
    /// Square cap.
    Square,
}

/// Line join style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineJoin {
    /// Mitred join.
    #[default]
    Miter,
    /// Rounded join.
    Round,
    /// Bevelled join.
    Bevel,
}

/// A stroke layer of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Whether this stroke is painted.
    #[serde(default = "crate::truth")]
    pub visible: bool,
    /// Stroke opacity.
    #[serde(default = "crate::one")]
    pub opacity: f64,
    /// Blend mode.
    #[serde(default)]
    pub blend: BlendMode,
    /// The paint source.
    pub paint: Paint,
    /// Shared style reference.
    #[serde(default)]
    pub style_id: Option<StyleId>,
    /// Bound variable.
    #[serde(default)]
    pub variable_id: Option<VariableId>,
    /// Stroke width in document units.
    #[serde(default = "crate::one")]
    pub weight: f64,
    /// Alignment relative to the outline.
    #[serde(default)]
    pub align: StrokeAlign,
    /// Cap style.
    #[serde(default)]
    pub cap: LineCap,
    /// Join style.
    #[serde(default)]
    pub join: LineJoin,
    /// Miter limit for `LineJoin::Miter`.
    #[serde(default = "default_miter_limit")]
    pub miter_limit: f64,
    /// Optional dash pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<Vec<f64>>,
}

fn default_miter_limit() -> f64 {
    4.0
}

impl Stroke {
    /// A 1px centred stroke in the given colour.
    pub fn solid(color: Color) -> Stroke {
        Stroke {
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            paint: Paint::solid(color),
            style_id: None,
            variable_id: None,
            weight: 1.0,
            align: StrokeAlign::Center,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: None,
        }
    }
}

impl Default for Stroke {
    fn default() -> Stroke {
        Stroke::solid(Color::BLACK)
    }
}

/// A visual effect.
///
/// Shadows carry all four Figma parameters; the renderer clamps the radius to
/// simulation quality limits and documents the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Shadow drawn behind the layer.
    DropShadow {
        /// Shadow colour (alpha matters).
        color: Color,
        /// Offset in document units.
        offset: Point,
        /// Blur radius.
        blur: f64,
        /// Spread (grow) of the shadow.
        spread: f64,
        /// Blend mode.
        #[serde(default)]
        blend: BlendMode,
    },
    /// Shadow drawn inside the layer.
    InnerShadow {
        /// Shadow colour.
        color: Color,
        /// Offset in document units.
        offset: Point,
        /// Blur radius.
        blur: f64,
        /// Spread.
        spread: f64,
        /// Blend mode.
        #[serde(default)]
        blend: BlendMode,
    },
    /// Blurs the layer contents.
    LayerBlur {
        /// Blur radius.
        radius: f64,
    },
    /// Blurs what is behind the layer.
    BackgroundBlur {
        /// Blur radius.
        radius: f64,
    },
}

impl Effect {
    /// Drop shadow helper.
    pub fn drop_shadow(color: Color, offset: Point, blur: f64) -> Effect {
        Effect::DropShadow {
            color,
            offset,
            blur,
            spread: 0.0,
            blend: BlendMode::Normal,
        }
    }

    /// The blur radius of blur effects, if this is one.
    pub fn blur_radius(&self) -> Option<f64> {
        match self {
            Effect::LayerBlur { radius } | Effect::BackgroundBlur { radius } => {
                Some(radius.max(0.0))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_paint_is_tagged() {
        let paint = Paint::solid(Color::from_hex("#ff6b4a").unwrap());
        let json = serde_json::to_value(&paint).unwrap();
        assert_eq!(json["type"], "solid");
        let back: Paint = serde_json::from_value(json).unwrap();
        assert_eq!(back, paint);
    }

    #[test]
    fn gradient_stops_sort() {
        let paint = Paint::Linear {
            stops: vec![
                GradientStop::new(0.8, Color::WHITE),
                GradientStop::new(0.2, Color::BLACK),
                GradientStop::new(0.5, Color::grey(0.5)),
            ],
            from: Point::new(0.0, 0.5),
            to: Point::new(1.0, 0.5),
            transform: None,
        };
        let sorted = paint.sorted_stops();
        assert_eq!(sorted[0].position, 0.2);
        assert_eq!(sorted[2].position, 0.8);
    }

    #[test]
    fn fill_defaults_and_colours() {
        let fill = Fill::solid(Color::from_hex("#2dd4bf").unwrap());
        assert!(fill.visible);
        assert_eq!(fill.opacity, 1.0);
        assert_eq!(fill.solid_color().unwrap().to_hex(), "#2dd4bf");
        let faded = Fill {
            opacity: 0.5,
            ..fill.clone()
        };
        assert!((faded.solid_color().unwrap().a - 0.5).abs() < 1e-9);
    }

    #[test]
    fn effect_serialization_variants() {
        let effect = Effect::drop_shadow(Color::BLACK, Point::new(0.0, 4.0), 12.0);
        let json = serde_json::to_value(&effect).unwrap();
        assert_eq!(json["type"], "drop_shadow");
        let blur = Effect::LayerBlur { radius: 8.0 };
        assert_eq!(blur.blur_radius(), Some(8.0));
        assert_eq!(effect.blur_radius(), None);
    }

    #[test]
    fn blend_mode_css_names() {
        assert_eq!(BlendMode::ColorDodge.as_css(), "color-dodge");
        assert_eq!(BlendMode::PassThrough.as_css(), "normal");
        assert!(BlendMode::PassThrough.is_container_only());
        assert!(!BlendMode::Multiply.is_container_only());
    }
}
