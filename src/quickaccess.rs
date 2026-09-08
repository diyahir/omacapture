//! Floating Quick Access panel shown after every capture.

use crate::app::Omashot;
use crate::capture::Frame;
use crate::config::Corner;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub struct QuickAccessPanel {
    window: Option<gtk::Window>,
    stack: Option<gtk::Box>,
    cards: Vec<gtk::Widget>,
}

impl QuickAccessPanel {
    pub fn new() -> Self {
        Self { window: None, stack: None, cards: Vec::new() }
    }

    fn ensure_window(&mut self, gb: &Rc<Omashot>) -> gtk::Box {
        if let Some(s) = &self.stack {
            return s.clone();
        }
        let cfg = gb.config.get().quick_access;
        let window = gtk::Window::new();
        window.set_application(Some(&gb.app));
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("omashot-quickaccess"));
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_exclusive_zone(0);
        let (v, h) = match cfg.corner {
            Corner::TopLeft => (Edge::Top, Edge::Left),
            Corner::TopRight => (Edge::Top, Edge::Right),
            Corner::BottomLeft => (Edge::Bottom, Edge::Left),
            Corner::BottomRight => (Edge::Bottom, Edge::Right),
        };
        window.set_anchor(v, true);
        window.set_anchor(h, true);
        window.set_margin(v, 16);
        window.set_margin(h, 16);
        window.add_css_class("omashot-quickaccess");
        let stack = gtk::Box::new(gtk::Orientation::Vertical, 10);
        stack.set_valign(if matches!(v, Edge::Bottom) { gtk::Align::End } else { gtk::Align::Start });
        window.set_child(Some(&stack));
        self.window = Some(window);
        self.stack = Some(stack.clone());
        stack
    }

    pub fn push(&mut self, gb: &Rc<Omashot>, frame: Frame, path: PathBuf, is_saved: bool) {
        let cfg = gb.config.get().quick_access;
        let stack = self.ensure_window(gb);
        while self.cards.len() >= cfg.max_cards.max(1) {
            let old = self.cards.remove(0);
            stack.remove(&old);
        }
        let card = build_card(gb, frame, path, is_saved, cfg.thumbnail_width, cfg.auto_dismiss_secs);
        stack.append(&card);
        self.cards.push(card.clone().upcast());
        if let Some(w) = &self.window {
            w.present();
        }
    }

    pub fn remove(&mut self, card: &gtk::Widget) {
        if let Some(pos) = self.cards.iter().position(|c| c == card) {
            self.cards.remove(pos);
        }
        if let Some(stack) = &self.stack {
            if card.parent().as_ref() == Some(stack.upcast_ref()) {
                stack.remove(card);
            }
        }
        if self.cards.is_empty() {
            if let Some(w) = self.window.take() {
                w.close();
            }
            self.stack = None;
        }
    }
}

fn build_card(
    gb: &Rc<Omashot>,
    frame: Frame,
    path: PathBuf,
    is_saved: bool,
    width: i32,
    auto_dismiss_secs: u32,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("qa-card");
    card.set_overflow(gtk::Overflow::Hidden);

    let aspect = frame.height() as f64 / frame.width().max(1) as f64;
    let thumb_h = ((width as f64 * aspect).round() as i32).clamp(60, 260);
    // Downscale for the card so the layer surface sizes to the thumbnail, not the capture.
    let thumb = image::imageops::thumbnail(&frame.image, width as u32, thumb_h as u32);
    let texture = Frame { image: thumb, scale: 1.0 }.to_texture();
    let picture = gtk::Picture::for_paintable(&texture);
    picture.set_size_request(width, thumb_h);
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_can_shrink(true);
    picture.add_css_class("qa-thumb");

    // Drag out to any app as a file.
    let drag = gtk::DragSource::new();
    drag.set_actions(gdk::DragAction::COPY);
    {
        let path = path.clone();
        let texture = texture.clone();
        drag.connect_prepare(move |src, _, _| {
            src.set_icon(Some(&texture), 0, 0);
            let file = gio::File::for_path(&path);
            let uri = format!("{}\r\n", file.uri());
            let uris = gdk::ContentProvider::for_bytes("text/uri-list", &glib::Bytes::from_owned(uri.into_bytes()));
            let files = gdk::ContentProvider::for_value(&gdk::FileList::from_array(&[file]).to_value());
            Some(gdk::ContentProvider::new_union(&[files, uris]))
        });
    }
    let card_ref = card.clone();
    let gb_drag = gb.clone();
    let keep_editing = gb.config.get().quick_access.keep_editing_after_drag;
    drag.connect_drag_end(move |_, _, _| {
        if !keep_editing {
            gb_drag.quick_access.borrow_mut().remove(card_ref.upcast_ref());
        }
    });
    picture.add_controller(drag);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&picture));

    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("qa-close");
    close.add_css_class("circular");
    close.set_halign(gtk::Align::Start);
    close.set_valign(gtk::Align::Start);
    close.set_margin_top(6);
    close.set_margin_start(6);
    overlay.add_overlay(&close);
    card.append(&overlay);

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    bar.add_css_class("qa-bar");
    bar.set_halign(gtk::Align::Fill);
    bar.set_homogeneous(true);
    let mk = |icon: &str, label: &str, tip: &str| {
        let b = gtk::Button::new();
        let content = adw::ButtonContent::new();
        content.set_icon_name(icon);
        content.set_label(label);
        b.set_child(Some(&content));
        b.add_css_class("flat");
        b.set_tooltip_text(Some(tip));
        b
    };
    let copy = mk("edit-copy-symbolic", "Copy", "Copy to clipboard (C)");
    let edit = mk("document-edit-symbolic", "Edit", "Annotate (E)");
    let openb = mk("folder-open-symbolic", "Open", "Open with default app (O)");
    let del = mk("user-trash-symbolic", "Delete", "Delete (Del)");
    for b in [&copy, &edit, &openb, &del] {
        bar.append(b);
    }
    card.append(&bar);

    let dismiss = {
        let gb = gb.clone();
        let card = card.clone();
        Rc::new(move || gb.quick_access.borrow_mut().remove(card.upcast_ref()))
    };

    {
        let frame = frame.clone();
        let d = dismiss.clone();
        copy.connect_clicked(move |_| {
            if let Ok(bytes) = crate::export::encode_png(&frame.image) {
                let _ = crate::clipboard::copy_png(&bytes);
            }
            d();
        });
    }
    {
        let gb = gb.clone();
        let frame = frame.clone();
        let saved = if is_saved { Some(path.clone()) } else { None };
        let d = dismiss.clone();
        edit.connect_clicked(move |_| {
            crate::annotate::open(&gb, frame.clone(), saved.clone());
            d();
        });
    }
    {
        let path = path.clone();
        let d = dismiss.clone();
        openb.connect_clicked(move |_| {
            let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
            d();
        });
    }
    {
        let gb = gb.clone();
        let path = path.clone();
        let d = dismiss.clone();
        del.connect_clicked(move |_| {
            let _ = std::fs::remove_file(&path);
            let _ = gb.history.borrow().delete_by_path(&path);
            d();
        });
    }
    {
        let d = dismiss.clone();
        close.connect_clicked(move |_| d());
    }

    // Auto-dismiss, paused while hovered.
    if auto_dismiss_secs > 0 {
        let hovered = Rc::new(RefCell::new(false));
        let motion = gtk::EventControllerMotion::new();
        {
            let h = hovered.clone();
            motion.connect_enter(move |_, _, _| *h.borrow_mut() = true);
        }
        {
            let h = hovered.clone();
            motion.connect_leave(move |_| *h.borrow_mut() = false);
        }
        card.add_controller(motion);
        let remaining = Rc::new(RefCell::new(auto_dismiss_secs as i64 * 10));
        let d = dismiss.clone();
        let card_weak = card.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if card_weak.upgrade().map(|c| c.parent().is_none()).unwrap_or(true) {
                return glib::ControlFlow::Break;
            }
            if !*hovered.borrow() {
                *remaining.borrow_mut() -= 1;
            }
            if *remaining.borrow() <= 0 {
                d();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    // Hover keyboard shortcuts: c / e / o / Delete.
    let keys = gtk::EventControllerKey::new();
    {
        let copy = copy.clone();
        let edit = edit.clone();
        let openb = openb.clone();
        let del = del.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            match key {
                gdk::Key::c => copy.emit_clicked(),
                gdk::Key::e => edit.emit_clicked(),
                gdk::Key::o => openb.emit_clicked(),
                gdk::Key::Delete | gdk::Key::BackSpace => del.emit_clicked(),
                gdk::Key::Escape => close.emit_clicked(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
    }
    card.add_controller(keys);
    card
}
