//! Raster image import: validate and embed PNG/JPEG/WebP/GIF as inline assets.
//!
//! Images are stored inside the document JSON as base64 (`data_base64` on
//! `ImageAssetMeta`) so a saved `.icraft` file is self-contained. The importer
//! refuses empty/oversized/corrupt data and unsupported formats instead of
//! letting them fail later during rendering.

use base64::Engine as _;
use dp_model::{AssetId, ImageAssetMeta};
use sha2::{Digest, Sha256};

use crate::ExportError;

/// Maximum accepted encoded image size (32 MiB).
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Maximum accepted image edge length in pixels.
pub const MAX_IMAGE_EDGE: u32 = 16384;
/// Maximum accepted pixel count (64 MP).
pub const MAX_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;
/// Maximum base64 payload stored per asset (matches document validation).
pub const MAX_INLINE_BASE64: usize = 24 * 1024 * 1024;

/// A decoded, validated raster image ready to register as a document asset.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedImage {
    /// Asset metadata (including inline base64 payload).
    pub asset: ImageAssetMeta,
    /// File extension of the embedded encoding.
    pub extension: String,
    /// Human readable file name (sanitized).
    pub file_name: String,
    /// Non-fatal notes about the import.
    pub warnings: Vec<String>,
}

/// Imports an encoded raster image from memory.
pub fn import_image_bytes(bytes: &[u8], name: &str) -> Result<ImportedImage, ExportError> {
    if bytes.is_empty() {
        return Err(ExportError::Conversion("image file is empty".to_owned()));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ExportError::Conversion(format!(
            "image file is {} bytes; the limit is {} bytes",
            bytes.len(),
            MAX_IMAGE_BYTES
        )));
    }
    let format = image::guess_format(bytes).map_err(|error| {
        ExportError::Conversion(format!("unsupported or corrupt image: {error}"))
    })?;
    let (mime, extension) = match format {
        image::ImageFormat::Png => ("image/png", "png"),
        image::ImageFormat::Jpeg => ("image/jpeg", "jpg"),
        image::ImageFormat::WebP => ("image/webp", "webp"),
        image::ImageFormat::Gif => ("image/gif", "gif"),
        other => {
            return Err(ExportError::Conversion(format!(
                "unsupported image format {other:?}; supported: PNG, JPEG, WebP, GIF"
            )));
        }
    };
    let decoded = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| ExportError::Conversion(format!("image could not be decoded: {error}")))?;
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return Err(ExportError::Conversion(
            "image has a zero dimension".to_owned(),
        ));
    }
    if width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE {
        return Err(ExportError::Conversion(format!(
            "image is {width}x{height} px; the limit is {MAX_IMAGE_EDGE} px per edge"
        )));
    }
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(ExportError::Conversion(format!(
            "image has {width}x{height} = {} pixels; the limit is {MAX_IMAGE_PIXELS}",
            u64::from(width) * u64::from(height)
        )));
    }
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    if data_base64.len() > MAX_INLINE_BASE64 {
        return Err(ExportError::Conversion(format!(
            "inline image data is {} bytes of base64; the limit is {MAX_INLINE_BASE64}",
            data_base64.len()
        )));
    }
    let file_name = sanitize_name(name);
    let id = AssetId::new_v4();
    let asset = ImageAssetMeta {
        id,
        name: file_name.clone(),
        hash: sha256_hex(bytes),
        width,
        height,
        mime: mime.to_owned(),
        file: format!("{id}.{extension}"),
        data_base64: Some(data_base64),
    };
    let mut warnings = Vec::new();
    if format == image::ImageFormat::Gif {
        warnings.push("only the first frame of the GIF is embedded".to_owned());
    }
    Ok(ImportedImage {
        asset,
        extension: extension.to_owned(),
        file_name,
        warnings,
    })
}

/// Imports an encoded raster image from disk.
pub fn import_image_file(path: impl AsRef<std::path::Path>) -> Result<ImportedImage, ExportError> {
    let path = path.as_ref();
    let metadata = std::fs::metadata(path)
        .map_err(|error| ExportError::Io(format!("{}: {error}", path.display())))?;
    if metadata.len() > MAX_IMAGE_BYTES as u64 {
        return Err(ExportError::Conversion(format!(
            "image file is {} bytes; the limit is {} bytes",
            metadata.len(),
            MAX_IMAGE_BYTES
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|error| ExportError::Io(format!("{}: {error}", path.display())))?;
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_owned());
    import_image_bytes(&bytes, &name)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn sanitize_name(name: &str) -> String {
    let trimmed = name.trim();
    let mut out: String = trimmed
        .chars()
        .map(|character| {
            if character.is_control() || character == '<' || character == '>' {
                '_'
            } else {
                character
            }
        })
        .collect();
    if out.is_empty() {
        out = "Image".to_owned();
    }
    if out.chars().count() > 200 {
        out = out.chars().take(200).collect();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("png encodes");
        bytes
    }

    #[test]
    fn imports_png_into_inline_asset() {
        let bytes = png_bytes(4, 3);
        let imported = import_image_bytes(&bytes, "  hero shot  ").expect("imports");
        assert_eq!(imported.asset.mime, "image/png");
        assert_eq!(imported.extension, "png");
        assert_eq!(imported.asset.width, 4);
        assert_eq!(imported.asset.height, 3);
        assert_eq!(imported.file_name, "hero shot");
        assert!(
            imported
                .asset
                .file
                .starts_with(&imported.asset.id.to_string())
        );
        assert_eq!(imported.asset.hash.len(), 64);
        let data = imported.asset.data_base64.expect("inline data");
        assert!(data.starts_with("iVBOR"), "PNG base64 prefix");
        assert!(imported.warnings.is_empty());
    }

    #[test]
    fn rejects_empty_and_corrupt_data() {
        assert!(import_image_bytes(&[], "empty").is_err());
        assert!(import_image_bytes(b"not an image at all", "junk").is_err());
    }

    #[test]
    fn rejects_unsupported_format() {
        // A valid BMP signature with junk payload: the whitelist must reject it
        // before attempting to decode.
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0u8; 16]);
        let error = import_image_bytes(&bmp, "bmp").expect_err("bmp unsupported");
        assert!(
            error.to_string().contains("unsupported image format"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_huge_dimensions() {
        let bytes = png_bytes(MAX_IMAGE_EDGE + 1, 1);
        let error = import_image_bytes(&bytes, "wide").expect_err("too wide");
        assert!(error.to_string().contains("limit"));
    }
}
