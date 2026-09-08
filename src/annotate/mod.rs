pub mod preferences;

use crate::app::Grabbit;
use crate::capture::Frame;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub fn open(gb: &Rc<Grabbit>, frame: Frame, source: Option<PathBuf>) {
    let _ = (gb, frame, source);
    tracing::warn!("annotate editor not implemented yet");
}

pub fn open_file(gb: &Rc<Grabbit>, path: &Path) {
    match image::open(path) {
        Ok(img) => open(gb, Frame { image: img.to_rgba8(), scale: 1.0 }, Some(path.to_path_buf())),
        Err(e) => tracing::error!("cannot open {}: {e}", path.display()),
    }
}
