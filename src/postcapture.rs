//! Routes a finished capture through the configured action matrix.

use crate::app::Omashot;
use crate::capture::{CaptureMode, Frame};
use crate::config::AfterCapture;
use gtk::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;

#[allow(dead_code)]
pub struct Outcome {
    pub saved_path: Option<PathBuf>,
}

pub fn handle(gb: &Rc<Omashot>, frame: Frame, mode: CaptureMode) -> Outcome {
    let cfg = gb.config.get();
    let actions: AfterCapture = match mode {
        CaptureMode::Fullscreen => cfg.post_capture.fullscreen,
        CaptureMode::Area => cfg.post_capture.area,
        CaptureMode::Window => cfg.post_capture.window,
        CaptureMode::AnnotateExport => cfg.post_capture.annotate_export,
    };
    run_actions(gb, frame, actions, mode != CaptureMode::AnnotateExport)
}

pub fn run_actions(gb: &Rc<Omashot>, frame: Frame, actions: AfterCapture, allow_annotate: bool) -> Outcome {
    let cfg = gb.config.get();
    let mut saved_path = None;

    if actions.save {
        match crate::export::save(&frame.image, &cfg) {
            Ok(p) => {
                tracing::info!("saved {}", p.display());
                if cfg.history.enabled {
                    let _ = gb.history.borrow().insert(&p, frame.width(), frame.height(), None);
                }
                saved_path = Some(p);
            }
            Err(e) => tracing::error!("save failed: {e}"),
        }
    }
    if actions.copy {
        match crate::export::encode_png(&frame.image) {
            Ok(bytes) => {
                if let Err(e) = crate::clipboard::copy_png(&bytes) {
                    tracing::error!("copy failed: {e}");
                }
            }
            Err(e) => tracing::error!("encode failed: {e}"),
        }
    }
    if cfg.general.sound {
        crate::notify::play_shutter();
    }

    // Quick Access needs a file on disk to drag or open; fall back to a temp file.
    let file_for_panel = saved_path.clone().or_else(|| {
        if actions.quick_access || (actions.annotate && allow_annotate) || gb.wait_mode.get() {
            crate::export::write_temp_png(&frame.image).ok()
        } else {
            None
        }
    });

    if actions.annotate && allow_annotate {
        crate::annotate::open(gb, frame.clone(), saved_path.clone());
    } else if gb.wait_mode.get() {
        crate::app::report_wait_result(gb, file_for_panel.as_deref(), frame.width(), frame.height());
        return Outcome { saved_path };
    }
    if actions.quick_access && cfg.quick_access.enabled {
        if let Some(path) = file_for_panel.clone() {
            gb.quick_access.borrow_mut().push(gb, frame.clone(), path, saved_path.is_some());
        }
    } else if cfg.general.notifications {
        let body = match &saved_path {
            Some(p) => format!("Saved to {}", p.display()),
            None if actions.copy => "Copied to clipboard".to_string(),
            None => "Capture finished".to_string(),
        };
        crate::notify::send(gb.app.upcast_ref::<gtk::Application>(), "capture", "Screenshot captured", &body, file_for_panel.as_deref());
    }

    Outcome { saved_path }
}
