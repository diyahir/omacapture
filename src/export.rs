//! Encoding frames to files and bytes.

use crate::config::{Config, ImageFormat};
use anyhow::{Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::{ImageEncoder, RgbaImage};
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub fn encode(img: &RgbaImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    match format {
        ImageFormat::Png => {
            img.write_to(&mut buf, image::ImageFormat::Png)?;
        }
        ImageFormat::Jpg => {
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            let enc = JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
            enc.write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)?;
        }
        ImageFormat::Webp => {
            // The pure-Rust encoder is lossless only; that is fine for screenshots.
            img.write_to(&mut buf, image::ImageFormat::WebP)?;
        }
    }
    Ok(buf.into_inner())
}

pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    encode(img, ImageFormat::Png, 100)
}

/// Build a fresh, non-colliding path in the configured save folder.
pub fn next_save_path(cfg: &Config) -> PathBuf {
    let folder = &cfg.general.save_folder;
    let _ = std::fs::create_dir_all(folder);
    let stem = chrono::Local::now().format(&cfg.general.filename_pattern).to_string();
    let ext = cfg.general.format.extension();
    let mut path = folder.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    path
}

pub fn save(img: &RgbaImage, cfg: &Config) -> Result<PathBuf> {
    let path = next_save_path(cfg);
    let bytes = encode(img, cfg.general.format, cfg.general.quality)?;
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

pub fn save_to(img: &RgbaImage, path: &Path, quality: u8) -> Result<()> {
    let format = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg") | Some("jpeg") => ImageFormat::Jpg,
        Some("webp") => ImageFormat::Webp,
        _ => ImageFormat::Png,
    };
    std::fs::write(path, encode(img, format, quality)?)?;
    Ok(())
}

/// Write a PNG into the temp dir so it can be dragged or shared by URI.
pub fn write_temp_png(img: &RgbaImage) -> Result<PathBuf> {
    crate::paths::ensure_dirs();
    let name = format!("grabbit-{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S-%3f"));
    let path = crate::paths::temp_dir().join(name);
    std::fs::write(&path, encode_png(img)?)?;
    Ok(path)
}
