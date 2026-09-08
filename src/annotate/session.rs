//! Editable sessions: annotations persisted next to the saved file so it can be re-opened.

use super::model::Sheet;
use crate::capture::Frame;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    file: PathBuf,
    signature: String,
    scale: f64,
    sheet: Sheet,
}

fn key_for(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut h);
    format!("{:016x}", h.finish())
}

fn signature(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            format!("{}:{}", m.len(), mtime)
        }
        Err(_) => String::new(),
    }
}

fn base(path: &Path) -> PathBuf {
    crate::paths::sessions_dir().join(key_for(path))
}

/// Persist the sheet and original pixels for `file` (call after writing the file).
pub fn save(file: &Path, source: &Frame, sheet: &Sheet) -> Result<PathBuf> {
    crate::paths::ensure_dirs();
    let b = base(file);
    let original = b.with_extension("png");
    std::fs::write(&original, crate::export::encode_png(&source.image)?)?;
    let manifest = Manifest { version: 1, file: file.to_path_buf(), signature: signature(file), scale: source.scale, sheet: sheet.clone() };
    let manifest_path = b.with_extension("json");
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest)?).context("writing session manifest")?;
    Ok(manifest_path)
}

/// Load a session for `file` if one exists and the file still matches.
pub fn load(file: &Path) -> Option<(Frame, Sheet)> {
    let b = base(file);
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(b.with_extension("json")).ok()?).ok()?;
    if manifest.signature != signature(file) {
        return None;
    }
    let img = image::open(b.with_extension("png")).ok()?.to_rgba8();
    Some((Frame { image: img, scale: manifest.scale }, manifest.sheet))
}

pub fn delete(file: &Path) {
    let b = base(file);
    let _ = std::fs::remove_file(b.with_extension("json"));
    let _ = std::fs::remove_file(b.with_extension("png"));
}
