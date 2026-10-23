//! File type detection for uploads.
//!
//! The declared `Content-Type` is treated as a hint only: the bytes must match a
//! known magic signature before an asset is accepted.

/// Types accepted by the asset pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SniffedKind {
    /// PNG image.
    Png,
    /// JPEG image.
    Jpeg,
    /// WebP image.
    Webp,
    /// SVG image (XML based).
    Svg,
    /// TrueType font.
    Ttf,
    /// OpenType font.
    Otf,
    /// WOFF2 web font.
    Woff2,
}

impl SniffedKind {
    /// Canonical MIME type stored for the asset.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Svg => "image/svg+xml",
            Self::Ttf => "font/ttf",
            Self::Otf => "font/otf",
            Self::Woff2 => "font/woff2",
        }
    }

    /// File extension used in the content-addressed storage key.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Svg => "svg",
            Self::Ttf => "ttf",
            Self::Otf => "otf",
            Self::Woff2 => "woff2",
        }
    }

    /// Database asset kind (`image` or `font`).
    pub fn asset_kind(self) -> &'static str {
        match self {
            Self::Png | Self::Jpeg | Self::Webp | Self::Svg => "image",
            Self::Ttf | Self::Otf | Self::Woff2 => "font",
        }
    }

    /// Whether the declared MIME type is consistent with the sniffed type.
    pub fn matches_declared_mime(self, declared: Option<&str>) -> bool {
        let Some(declared) = declared else {
            return true;
        };
        let declared = declared
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match self {
            Self::Png => declared == "image/png",
            Self::Jpeg => matches!(declared.as_str(), "image/jpeg" | "image/jpg"),
            Self::Webp => declared == "image/webp",
            Self::Svg => matches!(
                declared.as_str(),
                "image/svg+xml" | "text/xml" | "text/plain"
            ),
            Self::Ttf => matches!(declared.as_str(), "font/ttf" | "application/x-font-ttf"),
            Self::Otf => matches!(declared.as_str(), "font/otf" | "application/x-font-otf"),
            Self::Woff2 => declared == "font/woff2",
        }
    }
}

/// Detect the file type from magic bytes. Returns `None` for anything unknown.
pub fn sniff(bytes: &[u8]) -> Option<SniffedKind> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(SniffedKind::Png);
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some(SniffedKind::Jpeg);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(SniffedKind::Webp);
    }
    if bytes.len() >= 4 {
        let tag = &bytes[0..4];
        if tag == b"wOF2" {
            return Some(SniffedKind::Woff2);
        }
        if tag == b"OTTO" {
            return Some(SniffedKind::Otf);
        }
        if tag == [0x00, 0x01, 0x00, 0x00] || tag == b"true" || tag == b"typ1" {
            return Some(SniffedKind::Ttf);
        }
    }
    if looks_like_svg(bytes) {
        return Some(SniffedKind::Svg);
    }
    None
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    // Only inspect a bounded prefix so a huge arbitrary file cannot make the
    // check expensive.
    let prefix_len = bytes.len().min(4096);
    let Ok(text) = std::str::from_utf8(&bytes[..prefix_len]) else {
        return false;
    };
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let lower = text.to_ascii_lowercase();
    // SVG is case-sensitive XML, but accept any casing in the prologue so a
    // hand-written `<SVG>` is still recognised as an image and rejected by the
    // content checks only if it is not markup at all.
    if !(lower.starts_with("<?xml") || lower.starts_with("<svg")) {
        return false;
    }
    lower.contains("<svg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_is_detected() {
        let kind = sniff(b"\x89PNG\r\n\x1a\nrest").expect("png");
        assert_eq!(kind, SniffedKind::Png);
        assert_eq!(kind.mime(), "image/png");
        assert_eq!(kind.asset_kind(), "image");
    }

    #[test]
    fn jpeg_and_webp_are_detected() {
        assert_eq!(sniff(b"\xff\xd8\xff\xe0rest"), Some(SniffedKind::Jpeg));
        assert_eq!(
            sniff(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some(SniffedKind::Webp)
        );
    }

    #[test]
    fn fonts_are_detected() {
        assert_eq!(sniff(b"\x00\x01\x00\x00font"), Some(SniffedKind::Ttf));
        assert_eq!(sniff(b"OTTOfont"), Some(SniffedKind::Otf));
        assert_eq!(sniff(b"wOF2font"), Some(SniffedKind::Woff2));
        assert_eq!(SniffedKind::Woff2.asset_kind(), "font");
    }

    #[test]
    fn svg_is_detected_with_or_without_xml_prologue() {
        assert_eq!(
            sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"),
            Some(SniffedKind::Svg)
        );
        assert_eq!(
            sniff(b"\xef\xbb\xbf<?xml version=\"1.0\"?><svg></svg>"),
            Some(SniffedKind::Svg)
        );
        assert_eq!(
            sniff(b"  \n<SVG xmlns=\"http://www.w3.org/2000/svg\">"),
            Some(SniffedKind::Svg)
        );
    }

    #[test]
    fn unknown_and_disguised_files_are_rejected() {
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"GIF89a"), None);
        assert_eq!(sniff(b"<html></html>"), None);
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><note/>"), None);
        assert_eq!(sniff(b"MZ\x90\x00"), None);
    }

    #[test]
    fn declared_mime_consistency() {
        assert!(SniffedKind::Png.matches_declared_mime(Some("image/png")));
        assert!(SniffedKind::Png.matches_declared_mime(Some("image/png; charset=binary")));
        assert!(!SniffedKind::Png.matches_declared_mime(Some("image/jpeg")));
        assert!(SniffedKind::Png.matches_declared_mime(None));
    }
}
