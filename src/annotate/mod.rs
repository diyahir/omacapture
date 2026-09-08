pub mod canvas;
pub mod effects;
pub mod icons;
pub mod model;
pub mod preferences;
pub mod redact;
pub mod render;
pub mod session;
pub mod window;

use crate::app::Omacapture;
use crate::capture::Frame;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub fn open(gb: &Rc<Omacapture>, frame: Frame, source: Option<PathBuf>) {
    window::EditorWindow::open(gb, frame, source);
}

pub fn open_file(gb: &Rc<Omacapture>, path: &Path) {
    match image::open(path) {
        Ok(img) => open(gb, Frame { image: img.to_rgba8(), scale: 1.0 }, Some(path.to_path_buf())),
        Err(e) => tracing::error!("cannot open {}: {e}", path.display()),
    }
}
