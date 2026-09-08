//! Interactive drawing surface for the editor.

use super::model::*;
use super::render::{self, item_bounds, DrawOptions, Renderer};
use crate::capture::Frame;
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Crop,
    Rect,
    FilledRect,
    Oval,
    Arrow,
    Line,
    Text,
    Highlight,
    Blur,
    Spotlight,
    Counter,
    Watermark,
    Pencil,
}

impl Tool {
    pub const ALL: [Tool; 14] = [
        Tool::Select,
        Tool::Crop,
        Tool::Rect,
        Tool::FilledRect,
        Tool::Oval,
        Tool::Arrow,
        Tool::Line,
        Tool::Text,
        Tool::Highlight,
        Tool::Blur,
        Tool::Spotlight,
        Tool::Counter,
        Tool::Watermark,
        Tool::Pencil,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Crop => "Crop",
            Tool::Rect => "Rectangle",
            Tool::FilledRect => "Filled rectangle",
            Tool::Oval => "Oval",
            Tool::Arrow => "Arrow",
            Tool::Line => "Line",
            Tool::Text => "Text",
            Tool::Highlight => "Highlighter",
            Tool::Blur => "Blur",
            Tool::Spotlight => "Spotlight",
            Tool::Counter => "Counter",
            Tool::Watermark => "Watermark",
            Tool::Pencil => "Pencil",
        }
    }
    pub fn key(self) -> char {
        match self {
            Tool::Select => 'v',
            Tool::Crop => 'c',
            Tool::Rect => 'r',
            Tool::FilledRect => 'f',
            Tool::Oval => 'o',
            Tool::Arrow => 'a',
            Tool::Line => 'l',
            Tool::Text => 't',
            Tool::Highlight => 'h',
            Tool::Blur => 'b',
            Tool::Spotlight => 's',
            Tool::Counter => 'n',
            Tool::Watermark => 'w',
            Tool::Pencil => 'p',
        }
    }
}

/// Tool option state that is not part of the shared `Style`.
#[derive(Debug, Clone)]
pub struct ToolOptions {
    pub arrow_style: ArrowStyle,
    pub arrow_type: ArrowType,
    pub head_start: Head,
    pub head_end: Head,
    pub text_presentation: TextPresentation,
    pub blur_effect: BlurEffect,
    pub blur_strength: f64,
    pub counter_size: f64,
    pub watermark_text: String,
    pub watermark_style: WatermarkStyle,
    pub text_snap: bool,
    pub crop_snap: bool,
    pub crop_aspect: Option<(f64, f64)>,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            arrow_style: ArrowStyle::Straight,
            arrow_type: ArrowType::Classic,
            head_start: Head::None,
            head_end: Head::Arrow,
            text_presentation: TextPresentation::Plain,
            blur_effect: BlurEffect::Pixelate,
            blur_strength: 6.0,
            counter_size: 5.0,
            watermark_text: String::new(),
            watermark_style: WatermarkStyle::Diagonal,
            text_snap: true,
            crop_snap: true,
            crop_aspect: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Drag {
    None,
    Create { id: u64, start: Pt },
    Freehand { id: u64 },
    Move { ids: Vec<u64>, originals: Vec<Item>, last: Pt, created: bool },
    Resize { id: u64, handle: usize, orig: RectF },
    Endpoint { id: u64, which: u8 },
    Tail { id: u64 },
    Marquee { start: Pt, current: Pt },
    Pan { start_pan: (f64, f64), start: (f64, f64) },
    CropCreate { start: Pt },
    CropMove { last: Pt },
    CropResize { handle: usize, orig: RectF },
}

#[derive(Debug, Clone)]
pub struct CropState {
    pub rect: RectF,
    pub before: Option<RectF>,
}

struct TextEdit {
    id: u64,
    view: gtk::TextView,
}

/// A line of OCR words used for highlighter snapping.
#[derive(Debug, Clone)]
pub struct TextLine {
    pub y: f64,
    pub height: f64,
    pub words: Vec<(f64, f64)>,
}

pub struct EditorState {
    pub doc: Document,
    pub renderer: Renderer,
    pub tool: Tool,
    pub style: Style,
    pub options: ToolOptions,
    pub selection: Vec<u64>,
    pub zoom: f64,
    pub pan: (f64, f64),
    pub crop: Option<CropState>,
    pub text_lines: Option<Vec<TextLine>>,
    drag: Drag,
    text_edit: Option<TextEdit>,
    clipboard: Vec<Item>,
    paste_count: u32,
    pub space_down: bool,
    pub cursor: Pt,
    cursor_on_canvas: bool,
    last_checkpoint: Option<(String, std::time::Instant)>,
    viewport: (f64, f64),
    fitted: bool,
}

impl EditorState {
    pub fn to_screen(&self, p: Pt) -> (f64, f64) {
        (self.pan.0 + p.x * self.zoom, self.pan.1 + p.y * self.zoom)
    }
    pub fn to_image(&self, x: f64, y: f64) -> Pt {
        Pt::new((x - self.pan.0) / self.zoom, (y - self.pan.1) / self.zoom)
    }
    pub fn selected_items(&self) -> Vec<&Item> {
        self.doc.sheet.items.iter().filter(|i| self.selection.contains(&i.id)).collect()
    }

    /// Checkpoint unless the previous one had the same key very recently (slider drags).
    pub fn checkpoint_coalesced(&mut self, key: &str) {
        let now = std::time::Instant::now();
        if let Some((k, t)) = &self.last_checkpoint {
            if k == key && now.duration_since(*t).as_millis() < 800 {
                self.last_checkpoint = Some((key.to_string(), now));
                self.doc.dirty = true;
                return;
            }
        }
        self.doc.checkpoint();
        self.last_checkpoint = Some((key.to_string(), now));
    }

    pub fn checkpoint(&mut self) {
        self.doc.checkpoint();
        self.last_checkpoint = None;
    }

    fn fit(&mut self) {
        let (vw, vh) = self.viewport;
        if vw < 10.0 || vh < 10.0 {
            return;
        }
        let r = self.visible_rect();
        let margin = 24.0;
        let z = ((vw - margin * 2.0) / r.w).min((vh - margin * 2.0) / r.h).clamp(0.05, 1.0);
        self.zoom = z;
        self.center_on(r);
    }

    fn visible_rect(&self) -> RectF {
        match &self.crop {
            Some(_) => self.doc.image_rect(),
            None => self.doc.crop_rect(),
        }
    }

    fn center_on(&mut self, r: RectF) {
        let (vw, vh) = self.viewport;
        self.pan = ((vw - r.w * self.zoom) / 2.0 - r.x * self.zoom, (vh - r.h * self.zoom) / 2.0 - r.y * self.zoom);
    }

    pub fn set_zoom(&mut self, z: f64, anchor: Option<(f64, f64)>) {
        self.fitted = true;
        let z = z.clamp(0.1, 16.0);
        let (ax, ay) = anchor.unwrap_or((self.viewport.0 / 2.0, self.viewport.1 / 2.0));
        let before = self.to_image(ax, ay);
        self.zoom = z;
        let (sx, sy) = self.to_screen(before);
        self.pan.0 += ax - sx;
        self.pan.1 += ay - sy;
    }
}

#[derive(Clone)]
pub struct Canvas {
    pub widget: gtk::Overlay,
    pub area: gtk::DrawingArea,
    pub fixed: gtk::Fixed,
    pub state: Rc<RefCell<EditorState>>,
    on_change: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

const HANDLE_PX: f64 = 9.0;

impl Canvas {
    pub fn new(frame: Frame, style: Style, options: ToolOptions) -> Self {
        let renderer = Renderer::new(&frame);
        let doc = Document::new(frame);
        let state = Rc::new(RefCell::new(EditorState {
            doc,
            renderer,
            tool: Tool::Select,
            style,
            options,
            selection: Vec::new(),
            zoom: 1.0,
            pan: (0.0, 0.0),
            crop: None,
            text_lines: None,
            drag: Drag::None,
            text_edit: None,
            clipboard: Vec::new(),
            paste_count: 0,
            space_down: false,
            cursor: Pt::default(),
            cursor_on_canvas: false,
            last_checkpoint: None,
            viewport: (0.0, 0.0),
            fitted: false,
        }));

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_focusable(true);
        area.add_css_class("annotate-canvas");
        // The text-editing layer must not eat pointer events when no editor is open,
        // otherwise every drag on the canvas is swallowed before it reaches the gestures.
        let fixed = gtk::Fixed::new();
        fixed.set_can_target(false);
        let widget = gtk::Overlay::new();
        widget.set_child(Some(&area));
        widget.add_overlay(&fixed);
        widget.set_clip_overlay(&fixed, true);

        let canvas = Canvas { widget, area, fixed, state, on_change: Rc::new(RefCell::new(None)) };
        canvas.install_draw();
        canvas.install_input();
        canvas
    }

    pub fn set_on_change(&self, f: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    pub fn changed(&self) {
        self.area.queue_draw();
        if let Some(f) = self.on_change.borrow().as_ref() {
            f();
        }
    }

    // ----- drawing -----

    fn install_draw(&self) {
        let state = self.state.clone();
        let on_change = self.on_change.clone();
        self.area.set_draw_func(move |_, cr, w, h| {
            let mut s = state.borrow_mut();
            let (w, h) = (w as f64, h as f64);
            if s.viewport != (w, h) {
                s.viewport = (w, h);
                // Keep fitting on resize until the user zooms or pans by hand.
                if !s.fitted {
                    s.fit();
                    let on_change = on_change.clone();
                    glib::idle_add_local_once(move || {
                        if let Some(f) = on_change.borrow().as_ref() {
                            f();
                        }
                    });
                }
            }
            // Checkerboard-ish neutral background comes from CSS; draw canvas backdrop when styled.
            let crop = s.doc.crop_rect();
            let canvas_plain = s.doc.sheet.canvas.is_plain();
            let cropping = s.crop.is_some();
            let zoom = s.zoom;
            let pan = s.pan;

            if !canvas_plain && !cropping {
                let pad = s.doc.sheet.canvas.padding;
                let mut bw = crop.w + pad * 2.0;
                let mut bh = crop.h + pad * 2.0;
                if let Some((aw, ah)) = s.doc.sheet.canvas.aspect {
                    let t = aw / ah;
                    if bw / bh < t {
                        bw = bh * t;
                    } else {
                        bh = bw / t;
                    }
                }
                let ox = crop.x - (bw - crop.w) / 2.0;
                let oy = crop.y - (bh - crop.h) / 2.0;
                cr.save().ok();
                cr.translate(pan.0 + ox * zoom, pan.1 + oy * zoom);
                cr.scale(zoom, zoom);
                let EditorState { renderer, doc, .. } = &mut *s;
                renderer.draw_background(cr, doc, bw, bh);
                let radius = doc.sheet.canvas.corner_radius;
                let shadow = doc.sheet.canvas.shadow;
                let (ix, iy) = (crop.x - ox, crop.y - oy);
                if shadow > 0.0 {
                    render::draw_shadow(cr, ix, iy, crop.w, crop.h, radius, shadow);
                }
                cr.restore().ok();
                cr.save().ok();
                cr.translate(pan.0, pan.1);
                cr.scale(zoom, zoom);
                if radius > 0.0 {
                    render::rounded_rect(cr, crop.x, crop.y, crop.w, crop.h, radius);
                    cr.clip();
                }
            } else {
                cr.save().ok();
                cr.translate(pan.0, pan.1);
                cr.scale(zoom, zoom);
                // Drop shadow under the image so it reads as a sheet.
                let vis = if cropping { s.doc.image_rect() } else { crop };
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
                cr.rectangle(vis.x + 2.0 / zoom, vis.y + 2.0 / zoom, vis.w, vis.h);
                cr.fill().ok();
            }
            let hidden: Vec<u64> = s.text_edit.as_ref().map(|t| vec![t.id]).unwrap_or_default();
            {
                let EditorState { renderer, doc, .. } = &mut *s;
                renderer.draw_scene(cr, doc, &DrawOptions { hidden: &hidden, show_full_image: cropping });
            }
            cr.restore().ok();

            // Overlays in screen space.
            if let Some(c) = &s.crop {
                draw_crop_overlay(cr, &s, c, w, h);
            } else {
                draw_selection(cr, &s);
            }
            if let Drag::Marquee { start, current } = &s.drag {
                let a = s.to_screen(*start);
                let b = s.to_screen(*current);
                let (ar, ag, ab) = crate::theme::accent_rgb();
                cr.set_source_rgba(ar, ag, ab, 0.15);
                cr.rectangle(a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs());
                cr.fill_preserve().ok();
                cr.set_source_rgba(ar, ag, ab, 0.9);
                cr.set_line_width(1.0);
                cr.stroke().ok();
            }
        });
    }

    // ----- input -----

    fn install_input(&self) {
        let motion = gtk::EventControllerMotion::new();
        {
            let c = self.clone();
            motion.connect_motion(move |_, x, y| {
                {
                    let mut s = c.state.borrow_mut();
                    s.cursor = s.to_image(x, y);
                    s.cursor_on_canvas = true;
                }
                c.update_cursor(x, y);
            });
            let c = self.clone();
            motion.connect_leave(move |_| c.state.borrow_mut().cursor_on_canvas = false);
        }
        self.area.add_controller(motion);

        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        {
            let c = self.clone();
            drag.connect_drag_begin(move |g, x, y| {
                c.area.grab_focus();
                let mods = g.current_event_state();
                c.begin_drag(x, y, mods);
            });
            let c = self.clone();
            drag.connect_drag_update(move |g, dx, dy| {
                if let Some((sx, sy)) = g.start_point() {
                    c.update_drag(sx + dx, sy + dy, g.current_event_state());
                }
            });
            let c = self.clone();
            drag.connect_drag_end(move |g, dx, dy| {
                if let Some((sx, sy)) = g.start_point() {
                    c.end_drag(sx + dx, sy + dy);
                }
            });
        }
        self.area.add_controller(drag);

        let click = gtk::GestureClick::new();
        click.set_button(1);
        {
            let c = self.clone();
            click.connect_pressed(move |_, n, x, y| {
                if n == 2 {
                    c.double_click(x, y);
                }
            });
        }
        self.area.add_controller(click);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        {
            let c = self.clone();
            scroll.connect_scroll(move |ctl, dx, dy| {
                let mods = ctl.current_event_state();
                let mut s = c.state.borrow_mut();
                if mods.contains(gdk::ModifierType::CONTROL_MASK) {
                    let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
                    let z = s.zoom * factor;
                    let anchor = if s.cursor_on_canvas { Some(s.to_screen(s.cursor)) } else { None };
                    s.set_zoom(z, anchor);
                } else if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                    s.fitted = true;
                    s.pan.0 -= dy * 40.0;
                } else {
                    s.fitted = true;
                    s.pan.0 -= dx * 40.0;
                    s.pan.1 -= dy * 40.0;
                }
                drop(s);
                c.changed();
                glib::Propagation::Stop
            });
        }
        self.area.add_controller(scroll);

        let pinch = gtk::GestureZoom::new();
        {
            let c = self.clone();
            let base = Rc::new(RefCell::new(1.0));
            let b2 = base.clone();
            pinch.connect_begin(move |_, _| *b2.borrow_mut() = c.state.borrow().zoom);
            let c = self.clone();
            pinch.connect_scale_changed(move |g, scale| {
                let center = g.bounding_box_center();
                let mut s = c.state.borrow_mut();
                let z = *base.borrow() * scale;
                s.set_zoom(z, center);
                drop(s);
                c.changed();
            });
        }
        self.area.add_controller(pinch);
    }

    fn update_cursor(&self, x: f64, y: f64) {
        let s = self.state.borrow();
        let name = if s.space_down || matches!(s.drag, Drag::Pan { .. }) {
            "grabbing"
        } else if s.crop.is_some() {
            match crop_handle_at(&s, x, y) {
                Some(h) => handle_cursor(h),
                None if s.crop.as_ref().map(|c| c.rect.contains(s.to_image(x, y))).unwrap_or(false) => "move",
                None => "crosshair",
            }
        } else {
            match s.tool {
                Tool::Select => {
                    if let Some((_, h)) = handle_at(&s, x, y) {
                        handle_cursor(h)
                    } else if s.doc.hit_test(s.to_image(x, y), 6.0 / s.zoom).is_some() {
                        "move"
                    } else {
                        "default"
                    }
                }
                Tool::Text | Tool::Counter => "crosshair",
                _ => "crosshair",
            }
        };
        self.area.set_cursor_from_name(Some(name));
    }

    pub(crate) fn begin_drag(&self, x: f64, y: f64, mods: gdk::ModifierType) {
        self.commit_text_edit();
        let mut s = self.state.borrow_mut();
        let p = s.to_image(x, y);
        if s.space_down {
            s.drag = Drag::Pan { start_pan: s.pan, start: (x, y) };
            return;
        }
        if let Some(c) = s.crop.clone() {
            let full = s.doc.image_rect();
            if let Some(h) = crop_handle_at(&s, x, y) {
                s.drag = Drag::CropResize { handle: h, orig: c.rect };
            } else if c.rect.contains(p) && c.rect.normalized() != full {
                s.drag = Drag::CropMove { last: p };
            } else {
                s.drag = Drag::CropCreate { start: p };
            }
            return;
        }
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let style = s.style.clone();
        let opts = s.options.clone();
        match s.tool {
            Tool::Select => {
                if let Some((id, h)) = handle_at(&s, x, y) {
                    let it = s.doc.item(id).cloned().unwrap();
                    s.checkpoint();
                    s.drag = match &it.kind {
                        Kind::Line { .. } | Kind::Arrow { .. } => Drag::Endpoint { id, which: h as u8 },
                        Kind::Text { .. } if h == 99 => Drag::Tail { id },
                        _ => Drag::Resize { id, handle: h, orig: it.bounds() },
                    };
                    return;
                }
                match s.doc.hit_test(p, 6.0 / s.zoom) {
                    Some(id) => {
                        if shift {
                            if let Some(pos) = s.selection.iter().position(|&i| i == id) {
                                s.selection.remove(pos);
                            } else {
                                s.selection.push(id);
                            }
                        } else if !s.selection.contains(&id) {
                            s.selection = vec![id];
                        }
                        let ids = s.selection.clone();
                        let originals = s.doc.sheet.items.iter().filter(|i| ids.contains(&i.id)).cloned().collect();
                        s.checkpoint();
                        s.drag = Drag::Move { ids, originals, last: p, created: false };
                    }
                    None => {
                        if !shift {
                            s.selection.clear();
                        }
                        s.drag = Drag::Marquee { start: p, current: p };
                    }
                }
            }
            Tool::Crop => {}
            Tool::Text => {
                s.checkpoint();
                let id = s.doc.add(Kind::Text { pos: p, text: String::new(), presentation: opts.text_presentation, tail: None }, style);
                s.selection = vec![id];
                drop(s);
                self.begin_text_edit(id);
            }
            Tool::Counter => {
                s.checkpoint();
                let n = s.doc.sheet.next_counter;
                s.doc.sheet.next_counter += 1;
                let id = s.doc.add(Kind::Counter { center: p, number: n, size: opts.counter_size }, style);
                s.selection = vec![id];
                s.drag = Drag::Move { ids: vec![id], originals: vec![s.doc.item(id).cloned().unwrap()], last: p, created: true };
            }
            Tool::Pencil | Tool::Highlight => {
                s.checkpoint();
                let kind = if s.tool == Tool::Pencil { Kind::Pencil { points: vec![p] } } else { Kind::Highlight { points: vec![p] } };
                let id = s.doc.add(kind, style);
                s.selection.clear();
                s.drag = Drag::Freehand { id };
            }
            tool => {
                s.checkpoint();
                let r = RectF::new(p.x, p.y, 0.0, 0.0);
                let kind = match tool {
                    Tool::Rect => Kind::Rect { rect: r, filled: false },
                    Tool::FilledRect => Kind::Rect { rect: r, filled: true },
                    Tool::Oval => Kind::Oval { rect: r },
                    Tool::Line => Kind::Line { a: p, b: p },
                    Tool::Arrow => Kind::Arrow {
                        a: p,
                        b: p,
                        ctrl: None,
                        style: opts.arrow_style,
                        kind: opts.arrow_type,
                        head_start: opts.head_start,
                        head_end: opts.head_end,
                    },
                    Tool::Blur => Kind::Blur { rect: r, effect: opts.blur_effect, strength: opts.blur_strength },
                    Tool::Spotlight => Kind::Spotlight { rect: r },
                    Tool::Watermark => Kind::Watermark { rect: r, text: opts.watermark_text.clone(), style: opts.watermark_style },
                    _ => unreachable!(),
                };
                let id = s.doc.add(kind, style);
                s.selection.clear();
                s.drag = Drag::Create { id, start: p };
            }
        }
    }

    pub(crate) fn update_drag(&self, x: f64, y: f64, mods: gdk::ModifierType) {
        let mut s = self.state.borrow_mut();
        let p = s.to_image(x, y);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        s.cursor = p;
        let drag = s.drag.clone();
        match drag {
            Drag::None => return,
            Drag::Pan { start_pan, start } => {
                s.fitted = true;
                s.pan = (start_pan.0 + x - start.0, start_pan.1 + y - start.1);
            }
            Drag::Create { id, start } => {
                let mut end = p;
                if let Some(it) = s.doc.item_mut(id) {
                    match &mut it.kind {
                        Kind::Line { a, b } => {
                            if shift {
                                end = snap_angle(*a, end);
                            }
                            *b = end;
                        }
                        Kind::Arrow { a, b, ctrl, style, .. } => {
                            if shift {
                                end = snap_angle(*a, end);
                            }
                            *b = end;
                            *ctrl = curve_ctrl(*a, *b, *style);
                        }
                        Kind::Rect { rect, .. }
                        | Kind::Oval { rect }
                        | Kind::Blur { rect, .. }
                        | Kind::Spotlight { rect }
                        | Kind::Watermark { rect, .. } => {
                            if shift {
                                let side = (end.x - start.x).abs().max((end.y - start.y).abs());
                                end = Pt::new(start.x + side * (end.x - start.x).signum(), start.y + side * (end.y - start.y).signum());
                            }
                            *rect = RectF::from_points(start, end);
                        }
                        _ => {}
                    }
                }
            }
            Drag::Freehand { id } => {
                if let Some(it) = s.doc.item_mut(id) {
                    if let Kind::Pencil { points } | Kind::Highlight { points } = &mut it.kind {
                        if points.last().map(|l| l.dist(p) > 1.0).unwrap_or(true) {
                            points.push(p);
                        }
                    }
                }
            }
            Drag::Move { ids, originals, last, created } => {
                let (dx, dy) = (p.x - last.x, p.y - last.y);
                for id in &ids {
                    if let Some(it) = s.doc.item_mut(*id) {
                        it.translate(dx, dy);
                    }
                }
                s.drag = Drag::Move { ids, originals, last: p, created };
            }
            Drag::Resize { id, handle, orig } => {
                let mut r = resize_rect(orig, handle, p, shift);
                if r.w < 1.0 {
                    r.w = 1.0;
                }
                if r.h < 1.0 {
                    r.h = 1.0;
                }
                if let Some(it) = s.doc.item_mut(id) {
                    it.set_rect(r);
                }
            }
            Drag::Endpoint { id, which } => {
                if let Some(it) = s.doc.item_mut(id) {
                    match &mut it.kind {
                        Kind::Line { a, b } => {
                            if which == 0 {
                                *a = if shift { snap_angle(*b, p) } else { p };
                            } else {
                                *b = if shift { snap_angle(*a, p) } else { p };
                            }
                        }
                        Kind::Arrow { a, b, ctrl, .. } => match which {
                            0 => *a = if shift { snap_angle(*b, p) } else { p },
                            1 => *b = if shift { snap_angle(*a, p) } else { p },
                            _ => *ctrl = Some(p),
                        },
                        _ => {}
                    }
                }
            }
            Drag::Tail { id } => {
                if let Some(it) = s.doc.item_mut(id) {
                    if let Kind::Text { tail, .. } = &mut it.kind {
                        *tail = Some(p);
                    }
                }
            }
            Drag::Marquee { start, .. } => {
                s.drag = Drag::Marquee { start, current: p };
                let r = RectF::from_points(start, p);
                s.selection = s.doc.sheet.items.iter().filter(|i| r.intersects(&item_bounds(i))).map(|i| i.id).collect();
            }
            Drag::CropCreate { start } => {
                let mut r = RectF::from_points(start, p);
                if let Some((aw, ah)) = s.options.crop_aspect {
                    r = constrain_aspect(start, p, aw / ah);
                } else if s.options.crop_snap && !ctrl {
                    r = snap_to_content(&s.doc.source.image, r, s.zoom);
                }
                if let Some(c) = s.crop.as_mut() {
                    c.rect = r;
                }
            }
            Drag::CropMove { last } => {
                let (dx, dy) = (p.x - last.x, p.y - last.y);
                let full = s.doc.image_rect();
                if let Some(c) = s.crop.as_mut() {
                    let r = c.rect.translate(dx, dy);
                    c.rect = RectF::new(r.x.clamp(0.0, (full.w - r.w).max(0.0)), r.y.clamp(0.0, (full.h - r.h).max(0.0)), r.w, r.h);
                }
                s.drag = Drag::CropMove { last: p };
            }
            Drag::CropResize { handle, orig } => {
                let aspect = s.options.crop_aspect;
                let mut r = match aspect {
                    Some((aw, ah)) => resize_rect_aspect(orig, handle, p, aw / ah),
                    None => resize_rect(orig, handle, p, shift),
                };
                if aspect.is_none() && s.options.crop_snap && !ctrl && !shift {
                    r = snap_to_content(&s.doc.source.image, r, s.zoom);
                }
                r.w = r.w.max(4.0);
                r.h = r.h.max(4.0);
                if let Some(c) = s.crop.as_mut() {
                    c.rect = r;
                }
            }
        }
        drop(s);
        self.changed();
    }

    pub(crate) fn end_drag(&self, x: f64, y: f64) {
        let mut s = self.state.borrow_mut();
        let p = s.to_image(x, y);
        let drag = std::mem::replace(&mut s.drag, Drag::None);
        match drag {
            Drag::Create { id, start } => {
                // Shapes need a real drag; a click leaves nothing behind.
                if start.dist(p) < 3.0 / s.zoom {
                    s.doc.remove(id);
                    s.doc.undo();
                } else {
                    s.selection = vec![id];
                }
            }
            Drag::Freehand { id } => {
                let snap = s.options.text_snap && s.tool == Tool::Highlight;
                let pts = match s.doc.item(id) {
                    Some(Item { kind: Kind::Highlight { points }, .. }) => points.clone(),
                    Some(Item { kind: Kind::Pencil { points }, .. }) => points.clone(),
                    _ => Vec::new(),
                };
                if pts.len() < 2 {
                    s.doc.remove(id);
                    s.doc.undo();
                } else if s.tool == Tool::Highlight {
                    let bounds = points_bounds(&pts);
                    let mut snapped = false;
                    if snap {
                        if let Some(lines) = s.text_lines.clone() {
                            let bars = snap_highlight(&lines, &pts);
                            if !bars.is_empty() {
                                let style = s.doc.item(id).unwrap().style.clone();
                                s.doc.remove(id);
                                for (y, h, x0, x1) in bars {
                                    let mut st = style.clone();
                                    st.width = (h * 1.15 / 3.0).max(2.0);
                                    s.doc.add(Kind::Highlight { points: vec![Pt::new(x0, y), Pt::new(x1, y)] }, st);
                                }
                                snapped = true;
                            }
                        }
                    }
                    if !snapped && bounds.h < s.style.width * 3.0 && bounds.w > 12.0 {
                        // Near-straight strokes become straight bars.
                        let y = bounds.y + bounds.h / 2.0;
                        if let Some(it) = s.doc.item_mut(id) {
                            it.kind = Kind::Highlight { points: vec![Pt::new(bounds.x, y), Pt::new(bounds.right(), y)] };
                        }
                    }
                }
            }
            Drag::Move { ids, originals, created, .. } => {
                // A click without movement should not leave an undo entry.
                let unchanged = ids.iter().all(|id| s.doc.item(*id) == originals.iter().find(|o| o.id == *id));
                if unchanged && !created {
                    s.doc.undo();
                }
            }
            Drag::Marquee { .. } => {}
            Drag::CropCreate { start } if start.dist(p) < 3.0 / s.zoom => {
                if let Some(c) = s.crop.as_mut() {
                    c.rect = RectF::new(0.0, 0.0, 0.0, 0.0);
                }
                let full = s.doc.image_rect();
                if let Some(c) = s.crop.as_mut() {
                    c.rect = full;
                }
            }
            _ => {}
        }
        drop(s);
        self.changed();
    }

    fn double_click(&self, x: f64, y: f64) {
        let id = {
            let s = self.state.borrow();
            if s.crop.is_some() {
                return;
            }
            let p = s.to_image(x, y);
            s.doc.hit_test(p, 6.0 / s.zoom).filter(|id| matches!(s.doc.item(*id).map(|i| &i.kind), Some(Kind::Text { .. })))
        };
        if let Some(id) = id {
            self.state.borrow_mut().drag = Drag::None;
            self.state.borrow_mut().checkpoint();
            self.begin_text_edit(id);
        }
    }

    // ----- text editing -----

    #[allow(deprecated)]
    pub fn begin_text_edit(&self, id: u64) {
        self.commit_text_edit();
        let (pos, text, style, zoom, presentation) = {
            let s = self.state.borrow();
            let Some(it) = s.doc.item(id) else { return };
            let Kind::Text { pos, text, presentation, .. } = &it.kind else {
                return;
            };
            (s.to_screen(*pos), text.clone(), it.style.clone(), s.zoom, *presentation)
        };
        let view = gtk::TextView::new();
        view.set_wrap_mode(gtk::WrapMode::None);
        view.buffer().set_text(&text);
        view.add_css_class("annotate-text-edit");
        let pad = render::text_padding(&style, presentation) * zoom;
        let css = gtk::CssProvider::new();
        let color = match presentation {
            TextPresentation::Plain => style.color.to_hex(),
            _ => style.color.contrast().to_hex(),
        };
        let bg = match presentation {
            TextPresentation::Plain => "transparent".to_string(),
            _ => style.color.to_hex(),
        };
        css.load_from_string(&format!(
            "textview, textview text {{ font-family: \"{}\"; font-size: {}px; color: {color}; background: {bg}; caret-color: {color}; }} textview {{ padding: {pad}px; border-radius: {}px; }}",
            style.font_family,
            (style.font_size * zoom).max(4.0),
            style.corner_radius.max(if presentation == TextPresentation::Plain { 0.0 } else { 4.0 }) * zoom
        ));
        view.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        view.set_size_request(20, -1);
        self.fixed.set_can_target(true);
        self.fixed.put(&view, pos.0, pos.1);
        self.state.borrow_mut().text_edit = Some(TextEdit { id, view: view.clone() });

        let keys = gtk::EventControllerKey::new();
        {
            let c = self.clone();
            keys.connect_key_pressed(move |_, key, _, mods| {
                let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
                match key {
                    gdk::Key::Escape => {
                        c.commit_text_edit();
                        c.area.grab_focus();
                        glib::Propagation::Stop
                    }
                    gdk::Key::Return | gdk::Key::KP_Enter if ctrl => {
                        c.commit_text_edit();
                        c.area.grab_focus();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            });
        }
        view.add_controller(keys);
        {
            let c = self.clone();
            view.buffer().connect_changed(move |buf| {
                let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();
                let mut s = c.state.borrow_mut();
                if let Some(it) = s.doc.item_mut(id) {
                    if let Kind::Text { text: t, .. } = &mut it.kind {
                        *t = text;
                    }
                }
            });
        }
        let focus = gtk::EventControllerFocus::new();
        {
            let c = self.clone();
            focus.connect_leave(move |_| {
                let c = c.clone();
                glib::idle_add_local_once(move || c.commit_text_edit());
            });
        }
        view.add_controller(focus);
        view.grab_focus();
        self.changed();
    }

    pub fn commit_text_edit(&self) {
        let edit = self.state.borrow_mut().text_edit.take();
        let Some(edit) = edit else { return };
        let text = {
            let b = edit.view.buffer();
            b.text(&b.start_iter(), &b.end_iter(), true).to_string()
        };
        self.fixed.remove(&edit.view);
        self.fixed.set_can_target(false);
        let mut s = self.state.borrow_mut();
        if text.trim().is_empty() {
            s.doc.remove(edit.id);
            s.selection.retain(|&i| i != edit.id);
        } else if let Some(it) = s.doc.item_mut(edit.id) {
            if let Kind::Text { text: t, .. } = &mut it.kind {
                *t = text;
            }
        }
        if s.tool == Tool::Text {
            // Text is a one-shot placement, like the original.
            s.tool = Tool::Select;
        }
        drop(s);
        self.changed();
    }

    pub fn is_text_editing(&self) -> bool {
        self.state.borrow().text_edit.is_some()
    }

    // ----- commands used by the window -----

    pub fn set_tool(&self, tool: Tool) {
        self.commit_text_edit();
        {
            let mut s = self.state.borrow_mut();
            if tool == Tool::Crop {
                if s.crop.is_none() {
                    let r = s.doc.crop_rect();
                    let before = s.doc.sheet.crop;
                    s.crop = Some(CropState { rect: r, before });
                    s.selection.clear();
                }
            } else if s.crop.is_some() {
                s.crop = None;
            }
            s.tool = tool;
            if tool != Tool::Select {
                s.selection.clear();
            }
        }
        self.changed();
    }

    pub fn commit_crop(&self) {
        let mut s = self.state.borrow_mut();
        if let Some(c) = s.crop.take() {
            let full = s.doc.image_rect();
            let r = c.rect.normalized();
            let r = RectF::new(
                r.x.round().clamp(0.0, full.w - 1.0),
                r.y.round().clamp(0.0, full.h - 1.0),
                r.w.round().max(1.0),
                r.h.round().max(1.0),
            );
            s.checkpoint();
            s.doc.sheet.crop = if r == full { None } else { Some(r) };
            s.tool = Tool::Select;
            s.fitted = false;
        }
        drop(s);
        self.changed();
    }

    pub fn cancel_crop(&self) {
        let mut s = self.state.borrow_mut();
        if let Some(c) = s.crop.take() {
            s.doc.sheet.crop = c.before;
            s.tool = Tool::Select;
        }
        drop(s);
        self.changed();
    }

    pub fn auto_crop(&self) -> bool {
        let mut s = self.state.borrow_mut();
        let Some(r) = content_bounds(&s.doc.source.image) else {
            return false;
        };
        if let Some(c) = s.crop.as_mut() {
            c.rect = r;
        }
        drop(s);
        self.changed();
        true
    }

    pub fn set_crop_aspect(&self, aspect: Option<(f64, f64)>) {
        let mut s = self.state.borrow_mut();
        s.options.crop_aspect = aspect;
        if let (Some((aw, ah)), Some(c)) = (aspect, s.crop.as_mut()) {
            let t = aw / ah;
            let center = c.rect.center();
            let mut w = c.rect.w;
            let mut h = c.rect.h;
            if w / h > t {
                w = h * t;
            } else {
                h = w / t;
            }
            c.rect = RectF::new(center.x - w / 2.0, center.y - h / 2.0, w, h);
        }
        drop(s);
        self.changed();
    }

    pub fn delete_selection(&self) {
        self.commit_text_edit();
        let mut s = self.state.borrow_mut();
        if s.selection.is_empty() {
            return;
        }
        s.checkpoint();
        let sel = s.selection.clone();
        s.doc.sheet.items.retain(|i| !sel.contains(&i.id));
        s.selection.clear();
        drop(s);
        self.changed();
    }

    pub fn select_all(&self) {
        let mut s = self.state.borrow_mut();
        s.selection = s.doc.sheet.items.iter().map(|i| i.id).collect();
        drop(s);
        self.changed();
    }

    pub fn deselect(&self) {
        let mut s = self.state.borrow_mut();
        s.selection.clear();
        drop(s);
        self.changed();
    }

    pub fn undo(&self) {
        self.commit_text_edit();
        let mut s = self.state.borrow_mut();
        if s.doc.undo() {
            let ids: Vec<u64> = s.doc.sheet.items.iter().map(|i| i.id).collect();
            s.selection.retain(|i| ids.contains(i));
            s.renderer.invalidate();
        }
        drop(s);
        self.changed();
    }

    pub fn redo(&self) {
        self.commit_text_edit();
        let mut s = self.state.borrow_mut();
        if s.doc.redo() {
            let ids: Vec<u64> = s.doc.sheet.items.iter().map(|i| i.id).collect();
            s.selection.retain(|i| ids.contains(i));
            s.renderer.invalidate();
        }
        drop(s);
        self.changed();
    }

    pub fn nudge(&self, dx: f64, dy: f64) {
        let mut s = self.state.borrow_mut();
        if s.selection.is_empty() {
            return;
        }
        s.checkpoint_coalesced("nudge");
        let sel = s.selection.clone();
        for id in sel {
            if let Some(it) = s.doc.item_mut(id) {
                it.translate(dx, dy);
            }
        }
        drop(s);
        self.changed();
    }

    pub fn copy_items(&self) -> bool {
        let mut s = self.state.borrow_mut();
        if s.selection.is_empty() {
            return false;
        }
        let sel = s.selection.clone();
        s.clipboard = s.doc.sheet.items.iter().filter(|i| sel.contains(&i.id)).cloned().collect();
        s.paste_count = 0;
        true
    }

    pub fn paste_items(&self) {
        let mut s = self.state.borrow_mut();
        if s.clipboard.is_empty() {
            return;
        }
        s.checkpoint();
        s.paste_count += 1;
        let items = s.clipboard.clone();
        let bounds = items.iter().map(item_bounds).reduce(|a, b| a.union(&b)).unwrap();
        let (dx, dy) = if s.cursor_on_canvas {
            let c = s.cursor;
            (c.x - bounds.center().x, c.y - bounds.center().y)
        } else {
            let o = 10.0 * s.paste_count as f64;
            (o, o)
        };
        let mut new_ids = Vec::new();
        for mut it in items {
            it.translate(dx, dy);
            new_ids.push(s.doc.add(it.kind, it.style));
        }
        s.selection = new_ids;
        drop(s);
        self.changed();
    }

    pub fn duplicate(&self) {
        let mut s = self.state.borrow_mut();
        if s.selection.is_empty() {
            return;
        }
        s.checkpoint();
        let sel = s.selection.clone();
        s.selection = s.doc.duplicate(&sel, 10.0);
        drop(s);
        self.changed();
    }

    pub fn zoom_to(&self, z: f64) {
        self.state.borrow_mut().set_zoom(z, None);
        self.changed();
    }

    pub fn zoom_by(&self, factor: f64) {
        let mut s = self.state.borrow_mut();
        let z = s.zoom * factor;
        s.set_zoom(z, None);
        drop(s);
        self.changed();
    }

    pub fn zoom_fit(&self) {
        let mut s = self.state.borrow_mut();
        s.fit();
        drop(s);
        self.changed();
    }

    pub fn zoom_actual(&self) {
        let mut s = self.state.borrow_mut();
        s.fitted = true;
        s.zoom = 1.0;
        let r = s.visible_rect();
        s.center_on(r);
        drop(s);
        self.changed();
    }

    /// Apply a style change to the selection (and to tool defaults when nothing is selected).
    pub fn apply_style(&self, key: &str, f: impl Fn(&mut Style)) {
        let mut s = self.state.borrow_mut();
        f(&mut s.style);
        if !s.selection.is_empty() {
            s.checkpoint_coalesced(key);
            let sel = s.selection.clone();
            for id in sel {
                if let Some(it) = s.doc.item_mut(id) {
                    f(&mut it.style);
                }
            }
            if let Some(edit) = &s.text_edit {
                // Re-open the editor so the live widget picks up the new style.
                let id = edit.id;
                drop(s);
                self.begin_text_edit(id);
                return;
            }
        }
        drop(s);
        self.changed();
    }

    /// Mutate the kind of every selected item.
    pub fn apply_kind(&self, key: &str, f: impl Fn(&mut Kind)) {
        let mut s = self.state.borrow_mut();
        if s.selection.is_empty() {
            return;
        }
        s.checkpoint_coalesced(key);
        let sel = s.selection.clone();
        for id in sel {
            if let Some(it) = s.doc.item_mut(id) {
                f(&mut it.kind);
            }
        }
        s.renderer.invalidate();
        drop(s);
        self.changed();
    }

    pub fn set_spotlight_dim(&self, dim: f64) {
        let mut s = self.state.borrow_mut();
        s.checkpoint_coalesced("dim");
        s.doc.sheet.spotlight_dim = dim;
        drop(s);
        self.changed();
    }

    pub fn update_canvas(&self, key: &str, f: impl FnOnce(&mut super::model::Canvas)) {
        let mut s = self.state.borrow_mut();
        s.checkpoint_coalesced(key);
        f(&mut s.doc.sheet.canvas);
        drop(s);
        self.changed();
    }

    pub fn add_items(&self, items: Vec<(Kind, Style)>) {
        let mut s = self.state.borrow_mut();
        if items.is_empty() {
            return;
        }
        s.checkpoint();
        let mut ids = Vec::new();
        for (k, st) in items {
            ids.push(s.doc.add(k, st));
        }
        s.selection = ids;
        drop(s);
        self.changed();
    }

    pub fn render_export(&self) -> image::RgbaImage {
        self.commit_text_edit();
        let mut s = self.state.borrow_mut();
        let EditorState { renderer, doc, .. } = &mut *s;
        renderer.render_export(doc)
    }
}

// ----- geometry helpers -----

fn snap_angle(origin: Pt, p: Pt) -> Pt {
    let d = origin.dist(p);
    let a = (p.y - origin.y).atan2(p.x - origin.x);
    let step = std::f64::consts::PI / 4.0;
    let a = (a / step).round() * step;
    Pt::new(origin.x + d * a.cos(), origin.y + d * a.sin())
}

pub fn curve_ctrl(a: Pt, b: Pt, style: ArrowStyle) -> Option<Pt> {
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let (nx, ny) = (-dy / len, dx / len);
    let off = len * 0.3;
    match style {
        ArrowStyle::Straight => None,
        ArrowStyle::CurvedRight => Some(Pt::new(mid.x + nx * off, mid.y + ny * off)),
        ArrowStyle::CurvedLeft => Some(Pt::new(mid.x - nx * off, mid.y - ny * off)),
    }
}

fn handle_points(r: &RectF) -> [Pt; 8] {
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    [
        Pt::new(x, y),
        Pt::new(x + w / 2.0, y),
        Pt::new(x + w, y),
        Pt::new(x + w, y + h / 2.0),
        Pt::new(x + w, y + h),
        Pt::new(x + w / 2.0, y + h),
        Pt::new(x, y + h),
        Pt::new(x, y + h / 2.0),
    ]
}

fn handle_cursor(h: usize) -> &'static str {
    match h {
        0 | 4 => "nwse-resize",
        2 | 6 => "nesw-resize",
        1 | 5 => "ns-resize",
        3 | 7 => "ew-resize",
        _ => "grab",
    }
}

fn resize_rect(orig: RectF, handle: usize, p: Pt, keep_aspect: bool) -> RectF {
    let (mut x0, mut y0, mut x1, mut y1) = (orig.x, orig.y, orig.right(), orig.bottom());
    match handle {
        0 => {
            x0 = p.x;
            y0 = p.y;
        }
        1 => y0 = p.y,
        2 => {
            x1 = p.x;
            y0 = p.y;
        }
        3 => x1 = p.x,
        4 => {
            x1 = p.x;
            y1 = p.y;
        }
        5 => y1 = p.y,
        6 => {
            x0 = p.x;
            y1 = p.y;
        }
        7 => x0 = p.x,
        _ => {}
    }
    let mut r = RectF::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
    if keep_aspect && orig.w > 0.0 && orig.h > 0.0 {
        let t = orig.w / orig.h;
        r = resize_rect_aspect(orig, handle, p, t);
    }
    r
}

fn resize_rect_aspect(orig: RectF, handle: usize, p: Pt, t: f64) -> RectF {
    // Anchor the opposite corner/edge, size by the dominant axis.
    let (ax, ay) = match handle {
        0 => (orig.right(), orig.bottom()),
        1 => (orig.x + orig.w / 2.0, orig.bottom()),
        2 => (orig.x, orig.bottom()),
        3 => (orig.x, orig.y + orig.h / 2.0),
        4 => (orig.x, orig.y),
        5 => (orig.x + orig.w / 2.0, orig.y),
        6 => (orig.right(), orig.y),
        _ => (orig.right(), orig.y + orig.h / 2.0),
    };
    let (dx, dy) = ((p.x - ax).abs(), (p.y - ay).abs());
    let (w, h) = match handle {
        1 | 5 => (dy * t, dy),
        3 | 7 => (dx, dx / t),
        _ => {
            if dx / t > dy {
                (dx, dx / t)
            } else {
                (dy * t, dy)
            }
        }
    };
    let x = match handle {
        0 | 6 | 7 => ax - w,
        1 | 5 => ax - w / 2.0,
        _ => ax,
    };
    let y = match handle {
        0..=2 => ay - h,
        3 | 7 => ay - h / 2.0,
        _ => ay,
    };
    RectF::new(x, y, w, h)
}

fn constrain_aspect(start: Pt, p: Pt, t: f64) -> RectF {
    let (dx, dy) = (p.x - start.x, p.y - start.y);
    let (w, h) = if dx.abs() / t > dy.abs() { (dx.abs(), dx.abs() / t) } else { (dy.abs() * t, dy.abs()) };
    RectF::new(if dx < 0.0 { start.x - w } else { start.x }, if dy < 0.0 { start.y - h } else { start.y }, w, h)
}

/// Handle under a screen point for the current selection: (item id, handle index).
/// Handles 0-7 are rect corners/edges; for lines/arrows 0=a, 1=b, 2=ctrl; 99 = callout tail.
fn handle_at(s: &EditorState, x: f64, y: f64) -> Option<(u64, usize)> {
    let tol = HANDLE_PX;
    for it in s.selected_items() {
        match &it.kind {
            Kind::Line { a, b } => {
                for (i, p) in [a, b].iter().enumerate() {
                    let sp = s.to_screen(**p);
                    if (sp.0 - x).abs() <= tol && (sp.1 - y).abs() <= tol {
                        return Some((it.id, i));
                    }
                }
            }
            Kind::Arrow { a, b, ctrl, .. } => {
                let mut pts = vec![(*a, 0usize), (*b, 1usize)];
                if let Some(c) = ctrl {
                    pts.push((*c, 2));
                }
                for (p, i) in pts {
                    let sp = s.to_screen(p);
                    if (sp.0 - x).abs() <= tol && (sp.1 - y).abs() <= tol {
                        return Some((it.id, i));
                    }
                }
            }
            Kind::Text { presentation: TextPresentation::Callout, tail, pos, .. } => {
                let b = item_bounds(it);
                let t = tail.unwrap_or(Pt::new(pos.x + b.w / 2.0, pos.y + b.h + 24.0));
                let sp = s.to_screen(t);
                if (sp.0 - x).abs() <= tol && (sp.1 - y).abs() <= tol {
                    return Some((it.id, 99));
                }
            }
            _ if it.is_resizable() => {
                let r = it.bounds();
                for (i, p) in handle_points(&r).iter().enumerate() {
                    let sp = s.to_screen(*p);
                    if (sp.0 - x).abs() <= tol && (sp.1 - y).abs() <= tol {
                        return Some((it.id, i));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn crop_handle_at(s: &EditorState, x: f64, y: f64) -> Option<usize> {
    let c = s.crop.as_ref()?;
    for (i, p) in handle_points(&c.rect.normalized()).iter().enumerate() {
        let sp = s.to_screen(*p);
        if (sp.0 - x).abs() <= HANDLE_PX + 2.0 && (sp.1 - y).abs() <= HANDLE_PX + 2.0 {
            return Some(i);
        }
    }
    None
}

fn draw_handle(cr: &cairo::Context, x: f64, y: f64) {
    let (ar, ag, ab) = crate::theme::accent_rgb();
    cr.rectangle(x - HANDLE_PX / 2.0 + 0.5, y - HANDLE_PX / 2.0 + 0.5, HANDLE_PX - 1.0, HANDLE_PX - 1.0);
    cr.set_source_rgb(0.08, 0.08, 0.08);
    cr.fill_preserve().ok();
    cr.set_source_rgb(ar, ag, ab);
    cr.set_line_width(1.0);
    cr.stroke().ok();
}

fn draw_selection(cr: &cairo::Context, s: &EditorState) {
    for it in s.selected_items() {
        if s.text_edit.as_ref().map(|t| t.id == it.id).unwrap_or(false) {
            continue;
        }
        match &it.kind {
            Kind::Line { a, b } => {
                for p in [a, b] {
                    let (x, y) = s.to_screen(*p);
                    draw_handle(cr, x, y);
                }
            }
            Kind::Arrow { a, b, ctrl, .. } => {
                for p in [a, b] {
                    let (x, y) = s.to_screen(*p);
                    draw_handle(cr, x, y);
                }
                if let Some(c) = ctrl {
                    let (x, y) = s.to_screen(*c);
                    let (ar, ag, ab) = crate::theme::accent_rgb();
                    cr.set_source_rgba(ar, ag, ab, 0.6);
                    cr.set_dash(&[3.0, 3.0], 0.0);
                    let (ax, ay) = s.to_screen(*a);
                    let (bx, by) = s.to_screen(*b);
                    cr.move_to(ax, ay);
                    cr.line_to(x, y);
                    cr.line_to(bx, by);
                    cr.set_line_width(1.0);
                    cr.stroke().ok();
                    cr.set_dash(&[], 0.0);
                    draw_handle(cr, x, y);
                }
            }
            _ => {
                let r = item_bounds(it);
                let (x0, y0) = s.to_screen(Pt::new(r.x, r.y));
                let (x1, y1) = s.to_screen(Pt::new(r.right(), r.bottom()));
                let (ar, ag, ab) = crate::theme::accent_rgb();
                cr.set_source_rgba(ar, ag, ab, 0.9);
                cr.set_line_width(1.0);
                cr.set_dash(&[4.0, 3.0], 0.0);
                cr.rectangle(x0 - 2.5, y0 - 2.5, x1 - x0 + 5.0, y1 - y0 + 5.0);
                cr.stroke().ok();
                cr.set_dash(&[], 0.0);
                if it.is_resizable() {
                    for p in handle_points(&r) {
                        let (x, y) = s.to_screen(p);
                        draw_handle(cr, x, y);
                    }
                }
                if let Kind::Text { presentation: TextPresentation::Callout, tail, pos, .. } = &it.kind {
                    let t = tail.unwrap_or(Pt::new(pos.x + r.w / 2.0, pos.y + r.h + 24.0));
                    let (x, y) = s.to_screen(t);
                    draw_handle(cr, x, y);
                }
            }
        }
    }
}

fn draw_crop_overlay(cr: &cairo::Context, s: &EditorState, c: &CropState, w: f64, h: f64) {
    let r = c.rect.normalized();
    let (x0, y0) = s.to_screen(Pt::new(r.x, r.y));
    let (x1, y1) = s.to_screen(Pt::new(r.right(), r.bottom()));
    cr.set_fill_rule(cairo::FillRule::EvenOdd);
    cr.rectangle(0.0, 0.0, w, h);
    cr.rectangle(x0, y0, x1 - x0, y1 - y0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    cr.fill().ok();
    cr.set_fill_rule(cairo::FillRule::Winding);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    cr.set_line_width(1.0);
    cr.rectangle(x0 + 0.5, y0 + 0.5, x1 - x0 - 1.0, y1 - y0 - 1.0);
    cr.stroke().ok();
    // Rule of thirds.
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
    for i in 1..3 {
        let fx = x0 + (x1 - x0) * i as f64 / 3.0;
        let fy = y0 + (y1 - y0) * i as f64 / 3.0;
        cr.move_to(fx, y0);
        cr.line_to(fx, y1);
        cr.move_to(x0, fy);
        cr.line_to(x1, fy);
    }
    cr.stroke().ok();
    for p in handle_points(&r) {
        let (x, y) = s.to_screen(p);
        draw_handle(cr, x, y);
    }
    let label = format!("{} × {}", r.w.round() as i64, r.h.round() as i64);
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&pango::FontDescription::from_string("Sans 10")));
    layout.set_text(&label);
    let (tw, th) = layout.pixel_size();
    let (bx, by) = (x0 + 6.0, (y1 + 8.0).min(h - th as f64 - 12.0));
    cr.rectangle(bx, by, tw as f64 + 12.0, th as f64 + 8.0);
    cr.set_source_rgba(0.08, 0.08, 0.08, 0.9);
    cr.fill().ok();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(bx + 6.0, by + 4.0);
    pangocairo::functions::show_layout(cr, &layout);
}

// ----- content detection -----

/// Bounding box of pixels that differ from the dominant border color.
pub fn content_bounds(img: &image::RgbaImage) -> Option<RectF> {
    let (w, h) = (img.width(), img.height());
    if w < 4 || h < 4 {
        return None;
    }
    let bg = *img.get_pixel(0, 0);
    let differs = |p: &image::Rgba<u8>| {
        (p[0] as i32 - bg[0] as i32).abs() + (p[1] as i32 - bg[1] as i32).abs() + (p[2] as i32 - bg[2] as i32).abs() > 30
            || (p[3] as i32 - bg[3] as i32).abs() > 30
    };
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if differs(img.get_pixel(x, y)) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 || y1 < y0 || (x0 == 0 && y0 == 0 && x1 == w - 1 && y1 == h - 1) {
        return None;
    }
    Some(RectF::new(x0 as f64, y0 as f64, (x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64))
}

/// Snap crop edges to nearby strong horizontal/vertical color changes.
fn snap_to_content(img: &image::RgbaImage, r: RectF, zoom: f64) -> RectF {
    let reach = (10.0 / zoom).clamp(3.0, 24.0) as i32;
    let (w, h) = (img.width() as i32, img.height() as i32);
    let edge_strength_col = |x: i32, y0: i32, y1: i32| -> i64 {
        if x <= 0 || x >= w {
            return 0;
        }
        let mut s = 0i64;
        let mut y = y0.max(0);
        while y < y1.min(h) {
            let a = img.get_pixel((x - 1) as u32, y as u32);
            let b = img.get_pixel(x as u32, y as u32);
            s += (a[0] as i64 - b[0] as i64).abs() + (a[1] as i64 - b[1] as i64).abs() + (a[2] as i64 - b[2] as i64).abs();
            y += 2;
        }
        s
    };
    let edge_strength_row = |y: i32, x0: i32, x1: i32| -> i64 {
        if y <= 0 || y >= h {
            return 0;
        }
        let mut s = 0i64;
        let mut x = x0.max(0);
        while x < x1.min(w) {
            let a = img.get_pixel(x as u32, (y - 1) as u32);
            let b = img.get_pixel(x as u32, y as u32);
            s += (a[0] as i64 - b[0] as i64).abs() + (a[1] as i64 - b[1] as i64).abs() + (a[2] as i64 - b[2] as i64).abs();
            x += 2;
        }
        s
    };
    let best = |center: i32, f: &dyn Fn(i32) -> i64, span: i64| -> i32 {
        let mut best = (center, 0i64);
        for d in -reach..=reach {
            let v = f(center + d);
            if v > best.1 && v > span * 20 {
                best = (center + d, v);
            }
        }
        best.0
    };
    let (x0, y0, x1, y1) = (r.x.round() as i32, r.y.round() as i32, r.right().round() as i32, r.bottom().round() as i32);
    let span_v = ((y1 - y0) / 2).max(1) as i64;
    let span_h = ((x1 - x0) / 2).max(1) as i64;
    let nx0 = best(x0, &|x| edge_strength_col(x, y0, y1), span_v);
    let nx1 = best(x1, &|x| edge_strength_col(x, y0, y1), span_v);
    let ny0 = best(y0, &|y| edge_strength_row(y, x0, x1), span_h);
    let ny1 = best(y1, &|y| edge_strength_row(y, x0, x1), span_h);
    RectF::new(nx0 as f64, ny0 as f64, (nx1 - nx0).max(1) as f64, (ny1 - ny0).max(1) as f64)
}

// ----- highlighter text snapping -----

pub fn group_lines(words: &[crate::ocr::Word]) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = Vec::new();
    for w in words {
        let cy = w.y as f64 + w.h as f64 / 2.0;
        if let Some(line) = lines.iter_mut().find(|l| (l.y - cy).abs() < l.height * 0.6) {
            line.words.push((w.x as f64, (w.x + w.w) as f64));
            line.height = line.height.max(w.h as f64);
        } else {
            lines.push(TextLine { y: cy, height: w.h as f64, words: vec![(w.x as f64, (w.x + w.w) as f64)] });
        }
    }
    for l in &mut lines {
        l.words.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    }
    lines
}

/// Bars (center y, height, x0, x1) for a highlighter sweep across text lines.
fn snap_highlight(lines: &[TextLine], pts: &[Pt]) -> Vec<(f64, f64, f64, f64)> {
    let b = points_bounds(pts);
    if b.w < 8.0 || b.h > b.w * 1.5 && b.h > 40.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for l in lines {
        let band = l.height * 0.9;
        if b.y - band > l.y || b.bottom() + band < l.y {
            continue;
        }
        let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
        for (wx0, wx1) in &l.words {
            if *wx1 >= b.x && *wx0 <= b.right() {
                x0 = x0.min(*wx0);
                x1 = x1.max(*wx1);
            }
        }
        if x0.is_finite() {
            out.push((l.y, l.height, x0 - 2.0, x1 + 2.0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame() -> Frame {
        let mut img = image::RgbaImage::from_pixel(400, 300, image::Rgba([240, 240, 240, 255]));
        // A dark block so blur/crop detection has something to find.
        for y in 100..200 {
            for x in 100..300 {
                img.put_pixel(x, y, image::Rgba([20, 30, 40, 255]));
            }
        }
        Frame { image: img, scale: 1.0 }
    }

    fn canvas() -> Canvas {
        let _ = gtk::init();
        let c = Canvas::new(test_frame(), Style::default(), ToolOptions::default());
        {
            let mut s = c.state.borrow_mut();
            s.viewport = (800.0, 600.0);
            s.zoom = 1.0;
            s.pan = (0.0, 0.0);
            s.fitted = true;
        }
        c
    }

    fn drag(c: &Canvas, from: (f64, f64), to: (f64, f64), mods: gdk::ModifierType) {
        c.begin_drag(from.0, from.1, mods);
        c.update_drag((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0, mods);
        c.update_drag(to.0, to.1, mods);
        c.end_drag(to.0, to.1);
    }

    fn items(c: &Canvas) -> Vec<Item> {
        c.state.borrow().doc.sheet.items.clone()
    }

    #[test]
    fn editor_gestures() {
        let c = canvas();
        let none = gdk::ModifierType::empty();
        let shift = gdk::ModifierType::SHIFT_MASK;

        // Rectangle creation, undo, redo.
        c.set_tool(Tool::Rect);
        drag(&c, (10.0, 10.0), (100.0, 80.0), none);
        let it = items(&c);
        assert_eq!(it.len(), 1);
        assert!(matches!(it[0].kind, Kind::Rect { rect, filled: false } if rect == RectF::new(10.0, 10.0, 90.0, 70.0)));
        assert_eq!(c.state.borrow().selection, vec![it[0].id]);
        c.undo();
        assert!(items(&c).is_empty());
        c.redo();
        assert_eq!(items(&c).len(), 1);

        // A click without dragging creates nothing and leaves no undo entry.
        let undo_before = c.state.borrow().doc.can_undo();
        c.begin_drag(200.0, 200.0, none);
        c.end_drag(200.5, 200.0);
        assert_eq!(items(&c).len(), 1);
        assert_eq!(c.state.borrow().doc.can_undo(), undo_before);

        // Shift constrains to a square.
        drag(&c, (150.0, 150.0), (250.0, 190.0), shift);
        let sq = items(&c).last().cloned().unwrap();
        assert!(matches!(sq.kind, Kind::Rect { rect, .. } if (rect.w - rect.h).abs() < 0.01 && rect.w == 100.0));
        c.undo();

        // Moving with the selection tool (grab the left edge of the rectangle).
        c.set_tool(Tool::Select);
        drag(&c, (10.0, 45.0), (30.0, 55.0), none);
        let moved = items(&c)[0].clone();
        assert!(matches!(moved.kind, Kind::Rect { rect, .. } if rect.x == 30.0 && rect.y == 20.0), "{:?}", moved.kind);

        // Resize from the bottom-right handle (item is selected after the move).
        drag(&c, (120.0, 90.0), (160.0, 130.0), none);
        let resized = items(&c)[0].clone();
        assert!(matches!(resized.kind, Kind::Rect { rect, .. } if rect.w == 130.0 && rect.h == 110.0), "{:?}", resized.kind);

        // Arrow with 45-degree snapping, then endpoint drag.
        c.set_tool(Tool::Arrow);
        drag(&c, (200.0, 200.0), (300.0, 220.0), shift);
        let arrow = items(&c).last().cloned().unwrap();
        match arrow.kind {
            Kind::Arrow { a, b, .. } => {
                assert_eq!(a, Pt::new(200.0, 200.0));
                assert!((b.y - 200.0).abs() < 0.01, "snapped to horizontal: {b:?}");
            }
            k => panic!("expected arrow, got {k:?}"),
        }
        c.set_tool(Tool::Select);
        c.state.borrow_mut().selection = vec![arrow.id];
        let bx = match arrow.kind {
            Kind::Arrow { b, .. } => b,
            _ => unreachable!(),
        };
        drag(&c, (bx.x, bx.y), (bx.x, bx.y + 50.0), none);
        let arrow2 = c.state.borrow().doc.item(arrow.id).cloned().unwrap();
        assert!(matches!(arrow2.kind, Kind::Arrow { b, .. } if (b.y - (bx.y + 50.0)).abs() < 0.01));

        // Text placement with live editing and commit.
        c.set_tool(Tool::Text);
        c.begin_drag(50.0, 250.0, none);
        assert!(c.is_text_editing());
        let view = c.state.borrow().text_edit.as_ref().unwrap().view.clone();
        view.buffer().set_text("hello");
        c.commit_text_edit();
        let txt = items(&c).last().cloned().unwrap();
        assert!(matches!(&txt.kind, Kind::Text { text, .. } if text == "hello"));
        assert_eq!(c.state.borrow().tool, Tool::Select);
        // Empty text is discarded.
        c.set_tool(Tool::Text);
        let before = items(&c).len();
        c.begin_drag(60.0, 260.0, none);
        c.commit_text_edit();
        assert_eq!(items(&c).len(), before);

        // Counter auto-increments.
        c.set_tool(Tool::Counter);
        c.begin_drag(300.0, 50.0, none);
        c.end_drag(300.0, 50.0);
        c.begin_drag(330.0, 50.0, none);
        c.end_drag(330.0, 50.0);
        let nums: Vec<u32> = items(&c)
            .iter()
            .filter_map(|i| match i.kind {
                Kind::Counter { number, .. } => Some(number),
                _ => None,
            })
            .collect();
        assert_eq!(nums, vec![1, 2]);

        // Highlighter snaps to detected text lines.
        c.state.borrow_mut().text_lines = Some(vec![TextLine { y: 120.0, height: 16.0, words: vec![(100.0, 140.0), (150.0, 200.0)] }]);
        c.set_tool(Tool::Highlight);
        let before = items(&c).len();
        drag(&c, (105.0, 118.0), (190.0, 124.0), none);
        let hl = items(&c).last().cloned().unwrap();
        assert_eq!(items(&c).len(), before + 1);
        assert!(
            matches!(&hl.kind, Kind::Highlight { points } if points.len() == 2 && points[0].x == 98.0 && points[1].x == 202.0 && points[0].y == 120.0),
            "{:?}",
            hl.kind
        );

        // Blur affects the export pixels inside its region only.
        c.set_tool(Tool::Blur);
        drag(&c, (100.0, 100.0), (300.0, 200.0), none);
        let out = c.render_export();
        assert_eq!((out.width(), out.height()), (400, 300));
        assert_eq!(out.get_pixel(5, 5)[0], 240, "outside blur untouched");

        // Marquee select everything, delete, undo restores.
        c.set_tool(Tool::Select);
        drag(&c, (0.0, 0.0), (399.0, 299.0), none);
        let n = items(&c).len();
        assert_eq!(c.state.borrow().selection.len(), n);
        c.delete_selection();
        assert!(items(&c).is_empty());
        c.undo();
        assert_eq!(items(&c).len(), n);

        // Crop: drag a region, commit, export shrinks; cancel restores.
        c.set_tool(Tool::Crop);
        assert!(c.state.borrow().crop.is_some());
        c.state.borrow_mut().options.crop_snap = false;
        drag(&c, (50.0, 50.0), (250.0, 150.0), none);
        c.commit_crop();
        assert_eq!(c.state.borrow().doc.sheet.crop, Some(RectF::new(50.0, 50.0, 200.0, 100.0)));
        let out = c.render_export();
        assert_eq!((out.width(), out.height()), (200, 100));
        c.set_tool(Tool::Crop);
        drag(&c, (0.0, 0.0), (399.0, 299.0), none);
        c.cancel_crop();
        assert_eq!(c.state.borrow().doc.sheet.crop, Some(RectF::new(50.0, 50.0, 200.0, 100.0)));

        // Auto-crop finds the dark block.
        c.state.borrow_mut().doc.sheet.crop = None;
        c.set_tool(Tool::Crop);
        assert!(c.auto_crop());
        assert_eq!(c.state.borrow().crop.as_ref().unwrap().rect, RectF::new(100.0, 100.0, 200.0, 100.0));
        c.cancel_crop();

        // Copy / paste / duplicate.
        c.set_tool(Tool::Select);
        c.select_all();
        let n = items(&c).len();
        assert!(c.copy_items());
        c.paste_items();
        assert_eq!(items(&c).len(), n * 2);
        c.duplicate();
        assert_eq!(items(&c).len(), n * 2 + n);

        // Zooming keeps the anchor point stable.
        {
            let mut s = c.state.borrow_mut();
            let before = s.to_image(100.0, 100.0);
            s.set_zoom(2.0, Some((100.0, 100.0)));
            let after = s.to_image(100.0, 100.0);
            assert!((before.x - after.x).abs() < 0.01 && (before.y - after.y).abs() < 0.01);
        }

        canvas_export_with_background();
    }

    // GTK may only be initialized from one thread, so this runs inside `editor_gestures`.
    fn canvas_export_with_background() {
        let c = canvas();
        c.update_canvas("bg", |cv| {
            cv.background = Background::Solid { color: Color::rgba(0.0, 0.0, 1.0, 1.0) };
            cv.padding = 20.0;
            cv.corner_radius = 8.0;
        });
        let out = c.render_export();
        assert_eq!((out.width(), out.height()), (440, 340));
        assert_eq!(out.get_pixel(2, 2)[2], 255, "padding is blue");
        assert_eq!(out.get_pixel(220, 170)[0], 20, "image content intact");
    }
}
