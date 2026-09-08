//! Floating Quick Access panel shown after every capture.
//!
//! Keyboard: while the pointer is over a card the layer takes keyboard focus
//! and plain keys act on that card (`c` copy, `e` edit, `o` open, `Delete`,
//! `Escape`). While any card is visible the daemon also registers
//! Super+E / Super+D / Super+Delete with Hyprland so the newest card can be
//! handled without touching the mouse; those binds are removed the moment the
//! last card goes away.

use crate::app::Omashot;
use crate::capture::Frame;
use crate::config::Corner;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum QaAction {
    /// Copy the newest capture to the clipboard and dismiss its card.
    Copy,
    /// Open the newest capture in the editor.
    Edit,
    /// Open the newest capture with the default application.
    Open,
    /// Delete the newest capture's file and card.
    Delete,
    /// Dismiss the newest card.
    Dismiss,
}

struct CardActions {
    copy: Box<dyn Fn()>,
    edit: Box<dyn Fn()>,
    open: Box<dyn Fn()>,
    delete: Box<dyn Fn()>,
    dismiss: Box<dyn Fn()>,
}

impl CardActions {
    fn run(&self, action: QaAction) {
        match action {
            QaAction::Copy => (self.copy)(),
            QaAction::Edit => (self.edit)(),
            QaAction::Open => (self.open)(),
            QaAction::Delete => (self.delete)(),
            QaAction::Dismiss => (self.dismiss)(),
        }
    }
}

struct Card {
    widget: gtk::Widget,
    picture: gtk::Picture,
    path: PathBuf,
    width: i32,
    actions: Rc<CardActions>,
    hovered: Rc<Cell<bool>>,
}

pub struct QuickAccessPanel {
    window: Option<gtk::Window>,
    stack: Option<gtk::Box>,
    cards: Vec<Card>,
    global_binds_active: bool,
}

impl Default for QuickAccessPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl QuickAccessPanel {
    pub fn new() -> Self {
        Self { window: None, stack: None, cards: Vec::new(), global_binds_active: false }
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
        // Keyboard is taken only while a card is hovered (see build_card).
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

        // One key controller for the surface: keys go to the hovered card, else the newest.
        let keys = gtk::EventControllerKey::new();
        let gb_keys = gb.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            let sc = gb_keys.config.get().quick_access.shortcuts;
            let matches = |name: &str| {
                !name.trim().is_empty() && gdk::Key::from_name(name.trim()).map(|k| k.to_lower() == key.to_lower()).unwrap_or(false)
            };
            let action = if matches(&sc.hover_copy) {
                QaAction::Copy
            } else if matches(&sc.hover_edit) {
                QaAction::Edit
            } else if matches(&sc.hover_open) {
                QaAction::Open
            } else if matches(&sc.hover_delete) {
                QaAction::Delete
            } else if matches(&sc.hover_dismiss) {
                QaAction::Dismiss
            } else {
                return glib::Propagation::Proceed;
            };
            let actions = {
                let qa = gb_keys.quick_access.borrow();
                qa.cards.iter().find(|c| c.hovered.get()).or_else(|| qa.cards.last()).map(|c| c.actions.clone())
            };
            if let Some(a) = actions {
                a.run(action);
            }
            glib::Propagation::Stop
        });
        window.add_controller(keys);

        self.window = Some(window);
        self.stack = Some(stack.clone());
        stack
    }

    pub fn push(&mut self, gb: &Rc<Omashot>, frame: Frame, path: PathBuf, is_saved: bool) {
        let cfg = gb.config.get().quick_access;
        let stack = self.ensure_window(gb);
        while self.cards.len() >= cfg.max_cards.max(1) {
            let old = self.cards.remove(0);
            stack.remove(&old.widget);
        }
        let width = cfg.thumbnail_width.clamp(60, 2000);
        let hovered = Rc::new(Cell::new(false));
        let (card, picture, actions) = build_card(gb, frame, path.clone(), is_saved, width, cfg.auto_dismiss_secs, hovered.clone());
        stack.append(&card);
        self.cards.push(Card { widget: card.clone().upcast(), picture, path, width, actions, hovered });
        if let Some(w) = &self.window {
            w.present();
        }
        if cfg.global_shortcuts && !self.global_binds_active {
            self.global_binds_active = register_global_binds(&cfg.shortcuts);
        }
    }

    /// Update the thumbnail of the card showing `path`, if one is on screen.
    /// Saving from the editor refreshes an existing card instead of adding another.
    pub fn refresh(&mut self, path: &std::path::Path, frame: &Frame) -> bool {
        let Some(card) = self.cards.iter().find(|c| c.path == path) else { return false };
        let (thumb, w, h) = thumbnail(frame, card.width);
        card.picture.set_paintable(Some(&thumb));
        card.picture.set_size_request(w, h);
        true
    }

    /// Run an action on the newest card (hotkeys, `omashot qa <action>`, IPC).
    pub fn act(&self, action: QaAction) -> bool {
        match self.cards.last() {
            Some(card) => {
                let actions = card.actions.clone();
                // Defer: the action may remove the card, which needs the panel borrow released.
                glib::idle_add_local_once(move || actions.run(action));
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, card: &gtk::Widget) {
        if let Some(pos) = self.cards.iter().position(|c| &c.widget == card) {
            self.cards.remove(pos);
        }
        if let Some(stack) = &self.stack {
            if card.parent().as_ref() == Some(stack.upcast_ref()) {
                stack.remove(card);
            }
        }
        if self.cards.is_empty() {
            if let Some(w) = self.window.take() {
                // Closing synchronously fires pointer-leave into handlers that
                // borrow this panel, so let the caller's borrow end first.
                glib::idle_add_local_once(move || w.close());
            }
            self.stack = None;
            if self.global_binds_active {
                unregister_global_binds();
                self.global_binds_active = false;
            }
        }
    }

    fn set_keyboard(&self, exclusive: bool) {
        if let Some(w) = &self.window {
            w.set_keyboard_mode(if exclusive { KeyboardMode::Exclusive } else { KeyboardMode::None });
        }
    }
}

/// Global chords, alive only while a card is visible. Registered through
/// Hyprland's Lua runtime so nothing is written to config.
fn register_global_binds(sc: &crate::config::QuickAccessShortcuts) -> bool {
    if !crate::capture::hypr::is_hyprland() {
        return false;
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return false,
    };
    let mut entries = Vec::new();
    for (chord, action, label) in [
        (&sc.global_edit, "edit", "edit last capture"),
        (&sc.global_copy, "copy", "copy and dismiss last capture"),
        (&sc.global_delete, "delete", "delete last capture"),
        (&sc.global_open, "open", "open last capture"),
    ] {
        let chord = chord.trim();
        if chord.is_empty() || crate::config::validate_chord(chord).is_err() {
            continue;
        }
        entries.push(format!("  hl.bind(\"{chord}\", hl.dsp.exec_cmd(\"{exe} qa {action}\"), {{ description = \"Omashot: {label}\" }}),"));
    }
    if entries.is_empty() {
        return false;
    }
    let lua = format!(
        "if omashot_qa_binds then for _, b in ipairs(omashot_qa_binds) do pcall(function() b:unbind() end) end end\nomashot_qa_binds = {{\n{}\n}}\nreturn \"ok\"",
        entries.join("\n")
    );
    let ok = std::process::Command::new("hyprctl")
        .args(["eval", &lua])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        tracing::warn!("could not register Quick Access shortcuts with Hyprland");
    }
    ok
}

fn unregister_global_binds() {
    let _ = std::process::Command::new("hyprctl")
        .args([
            "eval",
            "if omashot_qa_binds then for _, b in ipairs(omashot_qa_binds) do pcall(function() b:unbind() end) end; omashot_qa_binds = nil end; return \"ok\"",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Downscale for the card so the layer surface sizes to the thumbnail, not the capture.
fn thumbnail(frame: &Frame, width: i32) -> (gtk::gdk::Texture, i32, i32) {
    let aspect = frame.height() as f64 / frame.width().max(1) as f64;
    let thumb_h = ((width as f64 * aspect).round() as i32).clamp(60, 260);
    let thumb = image::imageops::thumbnail(&frame.image, width as u32, thumb_h as u32);
    (Frame { image: thumb, scale: 1.0 }.to_texture(), width, thumb_h)
}

fn build_card(
    gb: &Rc<Omashot>,
    frame: Frame,
    path: PathBuf,
    is_saved: bool,
    width: i32,
    auto_dismiss_secs: u32,
    hovered: Rc<Cell<bool>>,
) -> (gtk::Box, gtk::Picture, Rc<CardActions>) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("qa-card");
    card.set_overflow(gtk::Overflow::Hidden);

    let (texture, width, thumb_h) = thumbnail(&frame, width);
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

    let sc = gb.config.get().quick_access.shortcuts;
    let short = |chord: &str| chord.replace("SUPER", "Super").replace(" + ", "+").replace("DELETE", "Del");
    let mut parts = Vec::new();
    let hover: Vec<&str> = [&sc.hover_copy, &sc.hover_edit, &sc.hover_open, &sc.hover_delete]
        .iter()
        .map(|k| k.as_str().trim())
        .filter(|k| !k.is_empty())
        .collect();
    if !hover.is_empty() {
        parts.push(format!("hover: {}", hover.join(" ")));
    }
    if !sc.global_edit.trim().is_empty() {
        parts.push(format!("{} edit", short(&sc.global_edit)));
    }
    if !sc.global_copy.trim().is_empty() {
        parts.push(format!("{} done", short(&sc.global_copy)));
    }
    let hint = gtk::Label::new(Some(&parts.join(" · ")));
    hint.set_visible(!parts.is_empty());
    hint.add_css_class("qa-hint");
    hint.set_halign(gtk::Align::End);
    hint.set_valign(gtk::Align::Start);
    hint.set_margin_top(6);
    hint.set_margin_end(6);
    overlay.add_overlay(&hint);
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
    let tip = |what: &str, hover: &str, global: &str| {
        let keys: Vec<String> = [hover.trim().to_string(), short(global)].into_iter().filter(|k| !k.is_empty()).collect();
        if keys.is_empty() {
            what.to_string()
        } else {
            format!("{what} ({})", keys.join(", "))
        }
    };
    let copy = mk("edit-copy-symbolic", "Copy", &tip("Copy to clipboard and dismiss", &sc.hover_copy, &sc.global_copy));
    let edit = mk("document-edit-symbolic", "Edit", &tip("Annotate", &sc.hover_edit, &sc.global_edit));
    let openb = mk("folder-open-symbolic", "Open", &tip("Open with default app", &sc.hover_open, &sc.global_open));
    let del = mk("user-trash-symbolic", "Delete", &tip("Delete", &sc.hover_delete, &sc.global_delete));
    for b in [&copy, &edit, &openb, &del] {
        bar.append(b);
    }
    card.append(&bar);

    let dismiss: Rc<dyn Fn()> = {
        let gb = gb.clone();
        let card = card.clone();
        Rc::new(move || gb.quick_access.borrow_mut().remove(card.upcast_ref()))
    };
    let actions = Rc::new(CardActions {
        copy: {
            let frame = frame.clone();
            let d = dismiss.clone();
            Box::new(move || {
                if let Ok(bytes) = crate::export::encode_png(&frame.image) {
                    let _ = crate::clipboard::copy_png(&bytes);
                }
                d();
            })
        },
        edit: {
            let gb = gb.clone();
            let frame = frame.clone();
            let saved = if is_saved { Some(path.clone()) } else { None };
            let d = dismiss.clone();
            Box::new(move || {
                crate::annotate::open(&gb, frame.clone(), saved.clone());
                d();
            })
        },
        open: {
            let path = path.clone();
            let d = dismiss.clone();
            Box::new(move || {
                let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                d();
            })
        },
        delete: {
            let gb = gb.clone();
            let path = path.clone();
            let d = dismiss.clone();
            Box::new(move || {
                let _ = std::fs::remove_file(&path);
                let _ = gb.history.borrow().delete_by_path(&path);
                crate::annotate::session::delete(&path);
                d();
            })
        },
        dismiss: {
            let d = dismiss.clone();
            Box::new(move || d())
        },
    });
    {
        let a = actions.clone();
        copy.connect_clicked(move |_| (a.copy)());
        let a = actions.clone();
        edit.connect_clicked(move |_| (a.edit)());
        let a = actions.clone();
        openb.connect_clicked(move |_| (a.open)());
        let a = actions.clone();
        del.connect_clicked(move |_| (a.delete)());
        let a = actions.clone();
        close.connect_clicked(move |_| (a.dismiss)());
    }

    // Hover: take the keyboard so plain keys reach the card; pause auto-dismiss.
    let motion = gtk::EventControllerMotion::new();
    {
        let h = hovered.clone();
        let gb_enter = gb.clone();
        let card = card.clone();
        motion.connect_enter(move |_, _, _| {
            h.set(true);
            if let Ok(qa) = gb_enter.quick_access.try_borrow() {
                qa.set_keyboard(true);
            }
            card.grab_focus();
        });
        let h = hovered.clone();
        let gb = gb.clone();
        motion.connect_leave(move |_| {
            h.set(false);
            if let Ok(qa) = gb.quick_access.try_borrow() {
                qa.set_keyboard(false);
            }
        });
    }
    card.add_controller(motion);
    card.set_focusable(true);

    if auto_dismiss_secs > 0 {
        let remaining = Rc::new(RefCell::new(auto_dismiss_secs as i64 * 10));
        let a = actions.clone();
        let card_weak = card.downgrade();
        let h = hovered.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if card_weak.upgrade().map(|c| c.parent().is_none()).unwrap_or(true) {
                return glib::ControlFlow::Break;
            }
            if !h.get() {
                *remaining.borrow_mut() -= 1;
            }
            if *remaining.borrow() <= 0 {
                (a.dismiss)();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }
    (card, picture, actions)
}
