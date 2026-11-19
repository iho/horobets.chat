//! Decode bounded raster images and store server-generated PNG thumbnails.
use crate::{Error, Result};
use std::io::Cursor;
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::Gif => Some("image/gif"),
        image::ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}
pub fn thumbnail(bytes: &[u8], avatar: bool) -> Result<Vec<u8>> {
    if image_mime(bytes).is_none() {
        return Err(Error::Bad("Choose a PNG, JPEG, GIF, or WebP image".into()));
    }
    let bad = |_: image::ImageError| {
        Error::Bad("Image is invalid or exceeds the 8192px / 64 MiB decode limit".into())
    };
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Error::Bad("Invalid image".into()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(bad)?;
    let resized = if avatar {
        decoded.resize_to_fill(256, 256, image::imageops::FilterType::Lanczos3)
    } else {
        decoded.thumbnail(1024, 768)
    };
    let mut output = Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(bad)?;
    Ok(output.into_inner())
}
