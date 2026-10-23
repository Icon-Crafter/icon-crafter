//! sRGB colours with alpha.
//!
//! Channels are stored as `f64` in the `0.0..=1.0` range. Values are sRGB
//! encoded (what designers type into the colour picker), not linear light: the
//! renderer converts to linear space for blending when that is needed.

use serde::{Deserialize, Serialize};

/// An RGBA colour in sRGB space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    /// Red channel, `0..=1`.
    pub r: f64,
    /// Green channel, `0..=1`.
    pub g: f64,
    /// Blue channel, `0..=1`.
    pub b: f64,
    /// Alpha channel, `0..=1` (`0` transparent, `1` opaque).
    #[serde(default = "default_alpha")]
    pub a: f64,
}

fn default_alpha() -> f64 {
    1.0
}

impl Default for Color {
    fn default() -> Color {
        Color::BLACK
    }
}

impl Color {
    /// Opaque black.
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque white.
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// Fully transparent.
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Creates a colour from channels.
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Color {
        Color { r, g, b, a }
    }

    /// Opaque colour from channels.
    pub const fn rgb(r: f64, g: f64, b: f64) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    /// Grey value `0..=1`.
    pub const fn grey(value: f64) -> Color {
        Color {
            r: value,
            g: value,
            b: value,
            a: 1.0,
        }
    }

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (without the leading
    /// `#` also accepted).
    pub fn from_hex(value: &str) -> Option<Color> {
        let hex = value.trim().trim_start_matches('#');
        let parse = |s: &str| u8::from_str_radix(s, 16).ok();
        let (r, g, b, a) = match hex.len() {
            3 => {
                let r = parse(&hex[0..1])? as f64 / 15.0;
                let g = parse(&hex[1..2])? as f64 / 15.0;
                let b = parse(&hex[2..3])? as f64 / 15.0;
                (r, g, b, 1.0)
            }
            4 => {
                let r = parse(&hex[0..1])? as f64 / 15.0;
                let g = parse(&hex[1..2])? as f64 / 15.0;
                let b = parse(&hex[2..3])? as f64 / 15.0;
                let a = parse(&hex[3..4])? as f64 / 15.0;
                (r, g, b, a)
            }
            6 => {
                let r = parse(&hex[0..2])? as f64 / 255.0;
                let g = parse(&hex[2..4])? as f64 / 255.0;
                let b = parse(&hex[4..6])? as f64 / 255.0;
                (r, g, b, 1.0)
            }
            8 => {
                let r = parse(&hex[0..2])? as f64 / 255.0;
                let g = parse(&hex[2..4])? as f64 / 255.0;
                let b = parse(&hex[4..6])? as f64 / 255.0;
                let a = parse(&hex[6..8])? as f64 / 255.0;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(Color::new(r, g, b, a))
    }

    /// Formats the colour as `#rrggbb` (or `#rrggbbaa` when not opaque).
    pub fn to_hex(self) -> String {
        let (r, g, b, a) = self.to_rgba8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Formats the colour as `rgb(r, g, b)` / `rgba(r, g, b, a)` CSS.
    pub fn to_css(self) -> String {
        let (r, g, b, a) = self.to_rgba8();
        if a == 255 {
            format!("rgb({r}, {g}, {b})")
        } else {
            format!("rgba({r}, {g}, {b}, {:.3})", a as f64 / 255.0)
        }
    }

    /// Converts to 8 bit channels.
    pub fn to_rgba8(self) -> (u8, u8, u8, u8) {
        let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        (
            channel(self.r),
            channel(self.g),
            channel(self.b),
            channel(self.a),
        )
    }

    /// Converts from 8 bit channels.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color::new(
            r as f64 / 255.0,
            g as f64 / 255.0,
            b as f64 / 255.0,
            a as f64 / 255.0,
        )
    }

    /// The same colour with a different alpha.
    pub fn with_alpha(self, alpha: f64) -> Color {
        Color {
            a: alpha.clamp(0.0, 1.0),
            ..self
        }
    }

    /// Whether the colour is fully transparent.
    pub fn is_transparent(self) -> bool {
        self.a <= 0.0
    }

    /// Multiplies the alpha channel (for instance with a fill's opacity).
    pub fn mul_alpha(self, factor: f64) -> Color {
        self.with_alpha(self.a * factor)
    }

    /// Channel-wise linear interpolation.
    pub fn lerp(self, other: Color, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }

    /// Clamps every channel into the valid range.
    pub fn clamped(self) -> Color {
        Color::new(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
            self.a.clamp(0.0, 1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let color = Color::from_hex("#ff6b4a").unwrap();
        assert_eq!(color.to_hex(), "#ff6b4a");
        let with_alpha = Color::from_hex("2dd4bf80").unwrap();
        assert_eq!(with_alpha.to_hex(), "#2dd4bf80");
        assert!((with_alpha.a - 128.0 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn css_format() {
        assert_eq!(Color::rgb(1.0, 0.0, 0.0).to_css(), "rgb(255, 0, 0)");
        assert!(
            Color::new(0.0, 0.0, 0.0, 0.5)
                .to_css()
                .starts_with("rgba(0, 0, 0, 0.502")
        );
    }

    #[test]
    fn invalid_hex_is_rejected() {
        assert!(Color::from_hex("#12").is_none());
        assert!(Color::from_hex("#gggggg").is_none());
    }

    #[test]
    fn serde_defaults_alpha_to_one() {
        let color: Color = serde_json::from_str(r#"{"r":1.0,"g":0.0,"b":0.0}"#).unwrap();
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn lerp_and_clamp() {
        let mid = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-9);
        let clamped = Color::new(2.0, -1.0, 0.5, 2.0).clamped();
        assert_eq!(clamped.r, 1.0);
        assert_eq!(clamped.g, 0.0);
        assert_eq!(clamped.a, 1.0);
    }
}
