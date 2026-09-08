//! Fullscreen layer-shell overlay for region / window selection.
//!
//! The screen is frozen first (one grim capture per output), then a
//! transparent overlay per monitor draws the frozen image, a dimming veil,
//! and the live selection. Confirming crops from the frozen frames so the
//! overlay itself never shows up in the result.

use super::{hypr, Frame, Rect};
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMode {
    Region,
    Window,
}

pub struct Selection {
    pub rect: Rect,
    pub frame: Frame,
}

struct MonitorView {
    logical: Rect,
    frame: Frame,
    window: gtk::Window,
    area: gtk::DrawingArea,
    surface: cairo::ImageSurface,
}

struct State {
    mode: PickMode,
    drag_start: Option<(f64, f64)>,
    /// Current selection in global logical coordinates.
    selection: Option<Rect>,
    /// Committed rectangle awaiting Enter (after a drag finished with adjust mode).
    cursor: (f64, f64),
    windows: Vec<hypr::Client>,
    hovered_window: Option<Rect>,
    remembered: Option<Rect>,
    views: Vec<MonitorView>,
    finished: bool,
    on_done: Option<Box<dyn FnOnce(Option<Selection>)>>,
}

const HANDLE: f64 = 6.0;

fn all_monitors() -> Vec<gdk::Monitor> {
    let display = gdk::Display::default().expect("no display");
    let list = display.monitors();
    (0..list.n_items()).filter_map(|i| list.item(i).and_downcast::<gdk::Monitor>()).collect()
}

/// Freeze the screen and start an interactive pick. `on_done` receives `None` on cancel.
pub fn pick(
    app: &impl IsA<gtk::Application>,
    mode: PickMode,
    include_cursor: bool,
    remembered: Option<Rect>,
    on_done: impl FnOnce(Option<Selection>) + 'static,
) {
    let monitors = all_monitors();
    let cursor = hypr::cursor_pos().map(|c| (c.x as f64, c.y as f64)).unwrap_or((0.0, 0.0));
    let windows = if hypr::is_hyprland() { hypr::visible_windows().unwrap_or_default() } else { Vec::new() };

    let state = Rc::new(RefCell::new(State {
        mode,
        drag_start: None,
        selection: None,
        cursor,
        windows,
        hovered_window: None,
        remembered,
        views: Vec::new(),
        finished: false,
        on_done: Some(Box::new(on_done)),
    }));

    let mut views = Vec::new();
    for monitor in &monitors {
        let geo = monitor.geometry();
        let logical = Rect::new(geo.x(), geo.y(), geo.width(), geo.height());
        let name = monitor.connector().map(|s| s.to_string()).unwrap_or_default();
        let frame = match super::grim::capture_output(&name, 1.0, include_cursor) {
            Ok(mut f) => {
                f.scale = f.width() as f64 / logical.w.max(1) as f64;
                f
            }
            Err(e) => {
                tracing::error!("capture of {name} failed: {e}");
                let mut s = state.borrow_mut();
                if let Some(cb) = s.on_done.take() {
                    cb(None);
                }
                return;
            }
        };
        let surface = frame.to_cairo_surface();

        let window = gtk::Window::new();
        window.set_application(Some(app));
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("omashot-overlay"));
        window.set_monitor(Some(monitor));
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        window.set_decorated(false);
        window.add_css_class("omashot-overlay");

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_cursor_from_name(Some("crosshair"));
        area.set_can_focus(true);
        area.set_focusable(true);
        window.set_child(Some(&area));

        views.push(MonitorView { logical, frame, window, area, surface });
    }
    state.borrow_mut().views = views;

    let n = state.borrow().views.len();
    for i in 0..n {
        let (area, window, logical) = {
            let s = state.borrow();
            (s.views[i].area.clone(), s.views[i].window.clone(), s.views[i].logical)
        };
        install_draw(&area, &state, i);
        install_input(&area, &window, &state, logical);
        window.present();
    }
    // Refresh the hover state under the initial cursor position.
    update_hover(&state);
    redraw_all(&state);
}

fn redraw_all(state: &Rc<RefCell<State>>) {
    for v in &state.borrow().views {
        v.area.queue_draw();
    }
}

fn update_hover(state: &Rc<RefCell<State>>) {
    let mut s = state.borrow_mut();
    let (cx, cy) = s.cursor;
    s.hovered_window = s.windows.iter().find(|w| w.rect().contains(cx, cy)).map(|w| w.rect());
}

fn finish(state: &Rc<RefCell<State>>, rect: Option<Rect>) {
    let mut s = state.borrow_mut();
    if s.finished {
        return;
    }
    s.finished = true;
    let result = rect.and_then(|r| composite(&s.views, r).map(|frame| Selection { rect: r, frame }));
    let views = std::mem::take(&mut s.views);
    let cb = s.on_done.take();
    drop(s);
    for v in views {
        v.window.close();
    }
    // Let the compositor drop the overlay before handing control back.
    glib::timeout_add_local_once(std::time::Duration::from_millis(40), move || {
        if let Some(cb) = cb {
            cb(result);
        }
    });
}

/// Assemble the pixels for a logical rectangle from the frozen per-monitor frames.
fn composite(views: &[MonitorView], rect: Rect) -> Option<Frame> {
    let hits: Vec<&MonitorView> = views.iter().filter(|v| v.logical.intersect(&rect).is_some()).collect();
    if hits.is_empty() {
        return None;
    }
    if hits.len() == 1 {
        let v = hits[0];
        let sub = v.logical.intersect(&rect)?;
        let s = v.frame.scale;
        let px = |n: i32| (n as f64 * s).round() as u32;
        return Some(v.frame.crop_px(px(sub.x - v.logical.x), px(sub.y - v.logical.y), px(sub.w), px(sub.h)));
    }
    let scale = hits.iter().map(|v| v.frame.scale).fold(1.0_f64, f64::max);
    let w = (rect.w as f64 * scale).round() as u32;
    let h = (rect.h as f64 * scale).round() as u32;
    let mut out = image::RgbaImage::new(w.max(1), h.max(1));
    for v in hits {
        let sub = v.logical.intersect(&rect).unwrap();
        let s = v.frame.scale;
        let px = |n: i32| (n as f64 * s).round() as u32;
        let part = v.frame.crop_px(px(sub.x - v.logical.x), px(sub.y - v.logical.y), px(sub.w), px(sub.h));
        let target_w = (sub.w as f64 * scale).round() as u32;
        let target_h = (sub.h as f64 * scale).round() as u32;
        let resized = if (part.width(), part.height()) != (target_w, target_h) {
            image::imageops::resize(&part.image, target_w.max(1), target_h.max(1), image::imageops::FilterType::Lanczos3)
        } else {
            part.image
        };
        let dx = ((sub.x - rect.x) as f64 * scale).round() as i64;
        let dy = ((sub.y - rect.y) as f64 * scale).round() as i64;
        image::imageops::overlay(&mut out, &resized, dx, dy);
    }
    Some(Frame { image: out, scale })
}

fn install_draw(area: &gtk::DrawingArea, state: &Rc<RefCell<State>>, idx: usize) {
    let state = state.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let s = state.borrow();
        let Some(view) = s.views.get(idx) else { return };
        let logical = view.logical;
        let scale = view.frame.scale;

        // Frozen screen.
        cr.save().ok();
        cr.scale(1.0 / scale, 1.0 / scale);
        cr.set_source_surface(&view.surface, 0.0, 0.0).ok();
        cr.paint().ok();
        cr.restore().ok();

        // Dim everything, then punch out the selection.
        let sel_local = s.selection.and_then(|r| r.intersect(&logical)).map(|r| local(r, logical));
        let hover_local = if s.mode == PickMode::Window && s.selection.is_none() {
            s.hovered_window.and_then(|r| r.intersect(&logical)).map(|r| local(r, logical))
        } else {
            None
        };
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        if let Some(r) = sel_local.or(hover_local) {
            cr.set_fill_rule(cairo::FillRule::EvenOdd);
            cr.rectangle(r.x as f64, r.y as f64, r.w as f64, r.h as f64);
        }
        cr.fill().ok();
        cr.set_fill_rule(cairo::FillRule::Winding);

        // Remembered area hint.
        if s.selection.is_none() && s.mode == PickMode::Region {
            if let Some(r) = s.remembered.and_then(|r| r.intersect(&logical)).map(|r| local(r, logical)) {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.6);
                cr.set_dash(&[6.0, 4.0], 0.0);
                cr.set_line_width(1.0);
                cr.rectangle(r.x as f64 + 0.5, r.y as f64 + 0.5, r.w as f64 - 1.0, r.h as f64 - 1.0);
                cr.stroke().ok();
                cr.set_dash(&[], 0.0);
            }
        }

        if let Some(r) = hover_local {
            let (ar, ag, ab) = crate::theme::accent_rgb();
            cr.set_source_rgba(ar, ag, ab, 1.0);
            cr.set_line_width(2.0);
            cr.rectangle(r.x as f64 + 1.0, r.y as f64 + 1.0, r.w as f64 - 2.0, r.h as f64 - 2.0);
            cr.stroke().ok();
        }

        if let Some(r) = sel_local {
            let (x, y, rw, rh) = (r.x as f64, r.y as f64, r.w as f64, r.h as f64);
            let (ar, ag, ab) = crate::theme::accent_rgb();
            cr.set_source_rgba(ar, ag, ab, 1.0);
            cr.set_line_width(1.0);
            cr.rectangle(x + 0.5, y + 0.5, rw - 1.0, rh - 1.0);
            cr.stroke().ok();
            // Corner + edge handles.
            for (hx, hy) in [
                (x, y), (x + rw / 2.0, y), (x + rw, y),
                (x, y + rh / 2.0), (x + rw, y + rh / 2.0),
                (x, y + rh), (x + rw / 2.0, y + rh), (x + rw, y + rh),
            ] {
                cr.rectangle(hx - HANDLE / 2.0, hy - HANDLE / 2.0, HANDLE, HANDLE);
                cr.fill().ok();
            }
            // Size readout.
            if let Some(full) = s.selection {
                let px_w = (full.w as f64 * scale).round() as i32;
                let px_h = (full.h as f64 * scale).round() as i32;
                let label = if (scale - 1.0).abs() < 0.01 {
                    format!("{} × {}", full.w, full.h)
                } else {
                    format!("{} × {}  ({}×{} px)", full.w, full.h, px_w, px_h)
                };
                draw_label(cr, &label, x, y + rh + 8.0, w as f64, h as f64);
            }
        }

        // Crosshair + magnifier only on the monitor holding the cursor.
        if s.drag_start.is_none() || s.selection.is_some() {
            let (cx, cy) = s.cursor;
            if logical.contains(cx, cy) {
                let (lx, ly) = (cx - logical.x as f64, cy - logical.y as f64);
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
                cr.set_line_width(1.0);
                cr.move_to(lx + 0.5, 0.0);
                cr.line_to(lx + 0.5, h as f64);
                cr.move_to(0.0, ly + 0.5);
                cr.line_to(w as f64, ly + 0.5);
                cr.stroke().ok();
                draw_magnifier(cr, &view.surface, scale, lx, ly, w as f64, h as f64);
            }
        }

        // Mode hint.
        let hint = match s.mode {
            PickMode::Region => "Drag to select  ·  A: window mode  ·  Enter: last area  ·  Esc: cancel",
            PickMode::Window => "Click a window  ·  A: region mode  ·  Esc: cancel",
        };
        draw_label(cr, hint, w as f64 / 2.0 - 220.0, 16.0, w as f64, h as f64);
    });
}

fn local(r: Rect, m: Rect) -> Rect {
    Rect::new(r.x - m.x, r.y - m.y, r.w, r.h)
}

fn draw_label(cr: &cairo::Context, text: &str, x: f64, y: f64, w: f64, h: f64) {
    let layout = pangocairo::functions::create_layout(cr);
    let font = pango::FontDescription::from_string("Sans 10");
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    let (tw, th) = layout.pixel_size();
    let pad = 6.0;
    let bw = tw as f64 + pad * 2.0;
    let bh = th as f64 + pad * 2.0;
    let bx = x.clamp(4.0, (w - bw - 4.0).max(4.0));
    let by = if y + bh > h - 4.0 { (y - bh - 16.0).max(4.0) } else { y };
    cr.rectangle(bx, by, bw, bh);
    cr.set_source_rgba(0.08, 0.08, 0.08, 0.9);
    cr.fill_preserve().ok();
    let (ar, ag, ab) = crate::theme::accent_rgb();
    cr.set_source_rgba(ar, ag, ab, 0.9);
    cr.set_line_width(1.0);
    cr.stroke().ok();
    cr.move_to(bx + pad, by + pad);
    cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    pangocairo::functions::show_layout(cr, &layout);
}

pub fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

fn draw_magnifier(cr: &cairo::Context, surface: &cairo::ImageSurface, scale: f64, lx: f64, ly: f64, w: f64, h: f64) {
    const SIZE: f64 = 130.0;
    const ZOOM: f64 = 8.0;
    let mut mx = lx + 24.0;
    let mut my = ly + 24.0;
    if mx + SIZE > w {
        mx = lx - 24.0 - SIZE;
    }
    if my + SIZE > h {
        my = ly - 24.0 - SIZE;
    }
    cr.save().ok();
    cr.rectangle(mx, my, SIZE, SIZE);
    cr.clip();
    cr.set_source_rgb(0.1, 0.1, 0.1);
    cr.paint().ok();
    // Sample the frozen image around the cursor in physical pixels.
    let px = (lx * scale).floor();
    let py = (ly * scale).floor();
    cr.translate(mx + SIZE / 2.0, my + SIZE / 2.0);
    cr.scale(ZOOM, ZOOM);
    cr.translate(-px - 0.5, -py - 0.5);
    cr.set_source_surface(surface, 0.0, 0.0).ok();
    cr.source().set_filter(cairo::Filter::Nearest);
    cr.paint().ok();
    cr.restore().ok();
    // Center pixel marker and border.
    cr.set_source_rgba(1.0, 0.3, 0.3, 0.9);
    cr.set_line_width(1.0);
    cr.rectangle(mx + SIZE / 2.0 - ZOOM / 2.0, my + SIZE / 2.0 - ZOOM / 2.0, ZOOM, ZOOM);
    cr.stroke().ok();
    let (ar, ag, ab) = crate::theme::accent_rgb();
    cr.set_source_rgba(ar, ag, ab, 0.95);
    cr.rectangle(mx + 0.5, my + 0.5, SIZE - 1.0, SIZE - 1.0);
    cr.stroke().ok();
}

fn install_input(area: &gtk::DrawingArea, window: &gtk::Window, state: &Rc<RefCell<State>>, logical: Rect) {
    let to_global = move |x: f64, y: f64| (x + logical.x as f64, y + logical.y as f64);

    let motion = gtk::EventControllerMotion::new();
    {
        let state = state.clone();
        motion.connect_motion(move |_, x, y| {
            state.borrow_mut().cursor = to_global(x, y);
            update_hover(&state);
            redraw_all(&state);
        });
    }
    area.add_controller(motion);

    let drag = gtk::GestureDrag::new();
    drag.set_button(1);
    {
        let state = state.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let g = to_global(x, y);
            let mut s = state.borrow_mut();
            s.drag_start = Some(g);
            s.cursor = g;
            s.selection = None;
        });
    }
    {
        let state = state.clone();
        drag.connect_drag_update(move |gesture, dx, dy| {
            let mut s = state.borrow_mut();
            let Some((sx, sy)) = s.drag_start else { return };
            let (cx, cy) = (sx + dx, sy + dy);
            s.cursor = (cx, cy);
            let shift = gesture.current_event_state().contains(gdk::ModifierType::SHIFT_MASK);
            let mut r = Rect::from_points(sx, sy, cx, cy);
            if shift {
                let side = r.w.max(r.h);
                r = Rect::from_points(sx, sy, sx + side as f64 * (cx - sx).signum(), sy + side as f64 * (cy - sy).signum());
            }
            if r.w > 2 || r.h > 2 {
                s.selection = Some(r);
            }
            drop(s);
            redraw_all(&state);
        });
    }
    {
        let state = state.clone();
        drag.connect_drag_end(move |_, _, _| {
            let (mode, sel, hovered) = {
                let mut s = state.borrow_mut();
                s.drag_start = None;
                (s.mode, s.selection, s.hovered_window)
            };
            match (mode, sel) {
                (_, Some(r)) if r.w >= 3 && r.h >= 3 => finish(&state, Some(r)),
                // A plain click captures the window under the cursor in either mode.
                (_, _) => {
                    if let Some(r) = hovered {
                        finish(&state, Some(r));
                    } else {
                        state.borrow_mut().selection = None;
                        redraw_all(&state);
                    }
                }
            }
        });
    }
    area.add_controller(drag);

    let right = gtk::GestureClick::new();
    right.set_button(3);
    {
        let state = state.clone();
        right.connect_pressed(move |_, _, _, _| finish(&state, None));
    }
    area.add_controller(right);

    let keys = gtk::EventControllerKey::new();
    {
        let state = state.clone();
        keys.connect_key_pressed(move |_, key, _, mods| {
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            let step = if shift { 10 } else { 1 };
            match key {
                gdk::Key::Escape => finish(&state, None),
                gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                    let r = {
                        let s = state.borrow();
                        s.selection.or(s.remembered).or(if s.mode == PickMode::Window { s.hovered_window } else { None })
                    };
                    if r.is_some() {
                        finish(&state, r);
                    }
                }
                gdk::Key::a | gdk::Key::A => {
                    {
                        let mut s = state.borrow_mut();
                        s.mode = if s.mode == PickMode::Region { PickMode::Window } else { PickMode::Region };
                        s.selection = None;
                    }
                    redraw_all(&state);
                }
                gdk::Key::Left | gdk::Key::Right | gdk::Key::Up | gdk::Key::Down => {
                    let mut s = state.borrow_mut();
                    if let Some(r) = s.selection.as_mut() {
                        match key {
                            gdk::Key::Left => r.x -= step,
                            gdk::Key::Right => r.x += step,
                            gdk::Key::Up => r.y -= step,
                            _ => r.y += step,
                        }
                    }
                    drop(s);
                    redraw_all(&state);
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(keys);
}
