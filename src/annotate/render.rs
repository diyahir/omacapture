//! Cairo rendering of a document; shared by the on-screen canvas and export.

use super::effects;
use super::model::*;
use crate::capture::{rgba_to_surface, surface_to_rgba, Frame};
use std::collections::HashMap;
use std::f64::consts::PI;

pub struct Renderer {
    pub base: cairo::ImageSurface,
    blur_cache: HashMap<(u64, i64, i64, i64, i64, BlurEffect, i64), cairo::ImageSurface>,
    blurred_bg_cache: Option<(i64, cairo::ImageSurface)>,
    wallpaper_cache: Option<(i64, cairo::ImageSurface)>,
}

#[derive(Default)]
pub struct DrawOptions<'a> {
    /// Items to skip (e.g. the text item currently being edited).
    pub hidden: &'a [u64],
    /// Extra dimming outside the crop when the crop tool is active.
    pub show_full_image: bool,
}

impl Renderer {
    pub fn new(source: &Frame) -> Self {
        Self { base: source.to_cairo_surface(), blur_cache: HashMap::new(), blurred_bg_cache: None, wallpaper_cache: None }
    }

    pub fn invalidate(&mut self) {
        self.blur_cache.clear();
        self.blurred_bg_cache = None;
        self.wallpaper_cache = None;
    }

    /// Draw the image plus every annotation in image coordinates.
    pub fn draw_scene(&mut self, cr: &cairo::Context, doc: &Document, opts: &DrawOptions) {
        let visible = if opts.show_full_image { doc.image_rect() } else { doc.crop_rect() };
        cr.save().ok();
        cr.rectangle(visible.x, visible.y, visible.w, visible.h);
        cr.clip();
        cr.set_source_surface(&self.base, 0.0, 0.0).ok();
        cr.paint().ok();

        // Blur regions sit under all markup.
        for it in doc.sheet.items.iter().filter(|i| i.is_blur() && !opts.hidden.contains(&i.id)) {
            if let Kind::Blur { rect, effect, strength } = &it.kind {
                self.draw_blur(cr, &doc.source.image, it.id, rect.normalized(), *effect, *strength, it.style.corner_radius);
            }
        }

        // Spotlight: dim everything except the union of spotlight rects.
        let spots: Vec<&Item> = doc.sheet.items.iter().filter(|i| i.is_spotlight() && !opts.hidden.contains(&i.id)).collect();
        if !spots.is_empty() {
            cr.save().ok();
            cr.set_fill_rule(cairo::FillRule::EvenOdd);
            cr.rectangle(visible.x, visible.y, visible.w, visible.h);
            for s in &spots {
                if let Kind::Spotlight { rect } = &s.kind {
                    let r = rect.normalized();
                    rounded_rect(cr, r.x, r.y, r.w, r.h, s.style.corner_radius);
                }
            }
            cr.set_source_rgba(0.0, 0.0, 0.0, doc.sheet.spotlight_dim.clamp(0.0, 1.0));
            cr.fill().ok();
            cr.restore().ok();
        }

        for it in doc.sheet.items.iter().filter(|i| !i.is_blur() && !i.is_spotlight() && !opts.hidden.contains(&i.id)) {
            draw_item(cr, it);
        }
        cr.restore().ok();
    }

    fn draw_blur(
        &mut self,
        cr: &cairo::Context,
        src: &image::RgbaImage,
        id: u64,
        r: RectF,
        effect: BlurEffect,
        strength: f64,
        radius: f64,
    ) {
        let x0 = r.x.floor().max(0.0) as i64;
        let y0 = r.y.floor().max(0.0) as i64;
        let x1 = (r.right().ceil() as i64).min(src.width() as i64);
        let y1 = (r.bottom().ceil() as i64).min(src.height() as i64);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let key = (id, x0, y0, x1, y1, effect, (strength * 10.0) as i64);
        if let std::collections::hash_map::Entry::Vacant(e) = self.blur_cache.entry(key) {
            let region = image::imageops::crop_imm(src, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32).to_image();
            let processed = effects::apply(&region, effect, strength, id as u32);
            e.insert(rgba_to_surface(&processed));
            if self.blur_cache.len() > 64 {
                let stale: Vec<_> = self.blur_cache.keys().filter(|k| k.0 != id).take(16).cloned().collect();
                for k in stale {
                    self.blur_cache.remove(&k);
                }
            }
        }
        let surf = &self.blur_cache[&key];
        cr.save().ok();
        rounded_rect(cr, x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64, radius);
        cr.clip();
        cr.set_source_surface(surf, x0 as f64, y0 as f64).ok();
        cr.paint().ok();
        cr.restore().ok();
    }

    /// Full export render including the canvas background, padding, and crop.
    pub fn render_export(&mut self, doc: &Document) -> image::RgbaImage {
        let crop = doc.crop_rect();
        let canvas = &doc.sheet.canvas;
        let pad = canvas.padding.max(0.0);
        let mut out_w = crop.w + pad * 2.0;
        let mut out_h = crop.h + pad * 2.0;
        if let Some((aw, ah)) = canvas.aspect {
            let target = aw / ah;
            if out_w / out_h < target {
                out_w = out_h * target;
            } else {
                out_h = out_w / target;
            }
        }
        let (ow, oh) = (out_w.round().max(1.0) as i32, out_h.round().max(1.0) as i32);
        let mut surf = cairo::ImageSurface::create(cairo::Format::ARgb32, ow, oh).expect("export surface");
        {
            let cr = cairo::Context::new(&surf).expect("cr");
            self.draw_background(&cr, doc, ow as f64, oh as f64);
            let ox = ((ow as f64 - crop.w) / 2.0).round();
            let oy = ((oh as f64 - crop.h) / 2.0).round();
            if !canvas.is_plain() && canvas.shadow > 0.0 {
                draw_shadow(&cr, ox, oy, crop.w, crop.h, canvas.corner_radius, canvas.shadow);
            }
            cr.save().ok();
            cr.translate(ox, oy);
            if canvas.corner_radius > 0.0 {
                rounded_rect(&cr, 0.0, 0.0, crop.w, crop.h, canvas.corner_radius);
                cr.clip();
            }
            cr.translate(-crop.x, -crop.y);
            self.draw_scene(&cr, doc, &DrawOptions::default());
            cr.restore().ok();
        }
        surface_to_rgba(&mut surf)
    }

    pub fn draw_background(&mut self, cr: &cairo::Context, doc: &Document, w: f64, h: f64) {
        match &doc.sheet.canvas.background {
            Background::None => {}
            Background::Solid { color } => {
                set_color(cr, color);
                cr.rectangle(0.0, 0.0, w, h);
                cr.fill().ok();
            }
            Background::Gradient { from, to, angle } => {
                let a = angle.to_radians();
                let (dx, dy) = (a.cos(), a.sin());
                let len = (w * dx.abs() + h * dy.abs()) / 2.0;
                let (cx, cy) = (w / 2.0, h / 2.0);
                let grad = cairo::LinearGradient::new(cx - dx * len, cy - dy * len, cx + dx * len, cy + dy * len);
                grad.add_color_stop_rgba(0.0, from.r, from.g, from.b, from.a);
                grad.add_color_stop_rgba(1.0, to.r, to.g, to.b, to.a);
                cr.set_source(&grad).ok();
                cr.rectangle(0.0, 0.0, w, h);
                cr.fill().ok();
            }
            Background::Blurred { strength, dim } => {
                let key = (*strength * 10.0) as i64;
                if self.blurred_bg_cache.as_ref().map(|c| c.0) != Some(key) {
                    let small = image::imageops::thumbnail(&doc.source.image, 160, 90);
                    let blurred = effects::gaussian(&small, (2.0 + strength * 1.5) as u32);
                    self.blurred_bg_cache = Some((key, rgba_to_surface(&blurred)));
                }
                let surf = &self.blurred_bg_cache.as_ref().unwrap().1;
                cr.save().ok();
                cr.scale(w / surf.width() as f64, h / surf.height() as f64);
                cr.set_source_surface(surf, 0.0, 0.0).ok();
                cr.source().set_filter(cairo::Filter::Bilinear);
                cr.paint().ok();
                cr.restore().ok();
                cr.set_source_rgba(0.0, 0.0, 0.0, dim.clamp(0.0, 1.0));
                cr.rectangle(0.0, 0.0, w, h);
                cr.fill().ok();
            }
            Background::Wallpaper { strength, dim } => {
                let key = (*strength * 10.0) as i64;
                if self.wallpaper_cache.as_ref().map(|c| c.0) != Some(key) {
                    let surf = crate::theme::wallpaper_path().and_then(|p| image::open(p).ok()).map(|img| {
                        // Downscale first: the blur radius then acts on a small image, which is
                        // both fast and gives the soft, defocused look.
                        let small = image::imageops::thumbnail(&img.to_rgba8(), 320, 180);
                        let blurred = effects::gaussian(&small, (2.0 + strength * 1.5) as u32);
                        rgba_to_surface(&blurred)
                    });
                    match surf {
                        Some(surf) => self.wallpaper_cache = Some((key, surf)),
                        None => {
                            // No wallpaper available: fall back to a neutral dark field.
                            cr.set_source_rgb(0.12, 0.12, 0.13);
                            cr.rectangle(0.0, 0.0, w, h);
                            cr.fill().ok();
                            return;
                        }
                    }
                }
                let surf = &self.wallpaper_cache.as_ref().unwrap().1;
                let (sw, sh) = (surf.width() as f64, surf.height() as f64);
                let scale = (w / sw).max(h / sh);
                cr.save().ok();
                cr.translate((w - sw * scale) / 2.0, (h - sh * scale) / 2.0);
                cr.scale(scale, scale);
                cr.set_source_surface(surf, 0.0, 0.0).ok();
                cr.source().set_filter(cairo::Filter::Bilinear);
                cr.paint().ok();
                cr.restore().ok();
                cr.set_source_rgba(0.0, 0.0, 0.0, dim.clamp(0.0, 1.0));
                cr.rectangle(0.0, 0.0, w, h);
                cr.fill().ok();
            }
            Background::Image { path } => {
                if let Ok(img) = image::open(path) {
                    let img = img.to_rgba8();
                    let surf = rgba_to_surface(&img);
                    let s = (w / img.width() as f64).max(h / img.height() as f64);
                    cr.save().ok();
                    cr.translate((w - img.width() as f64 * s) / 2.0, (h - img.height() as f64 * s) / 2.0);
                    cr.scale(s, s);
                    cr.set_source_surface(&surf, 0.0, 0.0).ok();
                    cr.paint().ok();
                    cr.restore().ok();
                } else {
                    cr.set_source_rgb(0.2, 0.2, 0.2);
                    cr.rectangle(0.0, 0.0, w, h);
                    cr.fill().ok();
                }
            }
        }
    }
}

fn draw_shadow(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64, intensity: f64) {
    let steps = 12;
    for i in 0..steps {
        let t = i as f64 / steps as f64;
        let spread = 18.0 * (1.0 - t);
        cr.set_source_rgba(0.0, 0.0, 0.0, intensity * 0.08);
        rounded_rect(cr, x - spread, y - spread + 8.0, w + spread * 2.0, h + spread * 2.0, radius + spread);
        cr.fill().ok();
    }
}

pub fn set_color(cr: &cairo::Context, c: &Color) {
    cr.set_source_rgba(c.r, c.g, c.b, c.a);
}

pub fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w.abs() / 2.0).min(h.abs() / 2.0).max(0.0);
    if r <= 0.0 {
        cr.rectangle(x, y, w, h);
        return;
    }
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

fn apply_line_style(cr: &cairo::Context, style: &Style) {
    cr.set_line_width(style.width.max(0.5));
    match style.line_style {
        LineStyle::Solid => {
            cr.set_dash(&[], 0.0);
            cr.set_line_cap(cairo::LineCap::Round);
        }
        LineStyle::Dashed => {
            cr.set_dash(&[style.width * 3.0, style.width * 2.0], 0.0);
            cr.set_line_cap(cairo::LineCap::Butt);
        }
        LineStyle::Dotted => {
            cr.set_dash(&[0.01, style.width * 2.0], 0.0);
            cr.set_line_cap(cairo::LineCap::Round);
        }
    }
    cr.set_line_join(cairo::LineJoin::Round);
}

pub fn font_desc(style: &Style, bold: bool) -> pango::FontDescription {
    let mut fd = pango::FontDescription::new();
    fd.set_family(&style.font_family);
    fd.set_absolute_size(style.font_size.max(4.0) * pango::SCALE as f64);
    if bold {
        fd.set_weight(pango::Weight::Bold);
    }
    fd
}

thread_local! {
    static SCRATCH: cairo::Context = {
        let s = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4).unwrap();
        cairo::Context::new(&s).unwrap()
    };
}

/// Measured size of a text item's label (text only, without padding).
pub fn measure_text(text: &str, style: &Style) -> (f64, f64) {
    SCRATCH.with(|cr| {
        let layout = pangocairo::functions::create_layout(cr);
        layout.set_font_description(Some(&font_desc(style, false)));
        layout.set_text(if text.is_empty() { " " } else { text });
        let (w, h) = layout.pixel_size();
        (w as f64, h as f64)
    })
}

pub fn text_padding(style: &Style, presentation: TextPresentation) -> f64 {
    match presentation {
        TextPresentation::Plain => 0.0,
        _ => (style.font_size * 0.4).max(4.0),
    }
}

/// Real bounds of an item, using text measurement where needed.
pub fn item_bounds(it: &Item) -> RectF {
    match &it.kind {
        Kind::Text { pos, text, presentation, .. } => {
            let (w, h) = measure_text(text, &it.style);
            let p = text_padding(&it.style, *presentation);
            RectF::new(pos.x, pos.y, w + p * 2.0, h + p * 2.0)
        }
        _ => it.bounds(),
    }
}

pub fn draw_item(cr: &cairo::Context, it: &Item) {
    let s = &it.style;
    cr.save().ok();
    match &it.kind {
        Kind::Rect { rect, filled } => {
            let r = rect.normalized();
            apply_line_style(cr, s);
            rounded_rect(cr, r.x, r.y, r.w, r.h, s.corner_radius);
            if *filled {
                set_color(cr, &s.color.with_alpha(s.color.a * 0.35));
                cr.fill_preserve().ok();
            }
            set_color(cr, &s.color);
            cr.stroke().ok();
        }
        Kind::Oval { rect } => {
            let r = rect.normalized();
            apply_line_style(cr, s);
            cr.save().ok();
            cr.translate(r.x + r.w / 2.0, r.y + r.h / 2.0);
            cr.scale((r.w / 2.0).max(0.5), (r.h / 2.0).max(0.5));
            cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
            cr.restore().ok();
            set_color(cr, &s.color);
            cr.stroke().ok();
        }
        Kind::Line { a, b } => {
            apply_line_style(cr, s);
            set_color(cr, &s.color);
            cr.move_to(a.x, a.y);
            cr.line_to(b.x, b.y);
            cr.stroke().ok();
        }
        Kind::Arrow { a, b, ctrl, style: _, kind, head_start, head_end } => {
            draw_arrow(cr, s, *a, *b, *ctrl, *kind, *head_start, *head_end);
        }
        Kind::Text { pos, text, presentation, tail } => {
            draw_text(cr, s, *pos, text, *presentation, *tail);
        }
        Kind::Highlight { points } => {
            if points.len() >= 2 {
                // Plain alpha reads well on both light and dark captures.
                set_color(cr, &s.color.with_alpha(0.5));
                cr.set_line_width(s.width * 3.0);
                cr.set_line_cap(cairo::LineCap::Butt);
                cr.set_line_join(cairo::LineJoin::Round);
                cr.move_to(points[0].x, points[0].y);
                for p in &points[1..] {
                    cr.line_to(p.x, p.y);
                }
                cr.stroke().ok();
            }
        }
        Kind::Pencil { points } => {
            if !points.is_empty() {
                set_color(cr, &s.color);
                cr.set_line_width(s.width);
                cr.set_line_cap(cairo::LineCap::Round);
                cr.set_line_join(cairo::LineJoin::Round);
                cr.move_to(points[0].x, points[0].y);
                if points.len() == 1 {
                    cr.line_to(points[0].x + 0.1, points[0].y);
                }
                for w in points.windows(2) {
                    let mid = Pt::new((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
                    cr.curve_to(w[0].x, w[0].y, w[0].x, w[0].y, mid.x, mid.y);
                }
                if let Some(last) = points.last() {
                    cr.line_to(last.x, last.y);
                }
                cr.stroke().ok();
            }
        }
        Kind::Counter { center, number, size } => {
            let d = 12.0 + 4.0 * size;
            set_color(cr, &s.color);
            cr.arc(center.x, center.y, d / 2.0, 0.0, 2.0 * PI);
            cr.fill().ok();
            let layout = pangocairo::functions::create_layout(cr);
            let mut fd = font_desc(s, true);
            fd.set_absolute_size(d * 0.55 * pango::SCALE as f64);
            layout.set_font_description(Some(&fd));
            layout.set_text(&number.to_string());
            let (tw, th) = layout.pixel_size();
            set_color(cr, &s.color.contrast());
            cr.move_to(center.x - tw as f64 / 2.0, center.y - th as f64 / 2.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
        Kind::Watermark { rect, text, style } => {
            draw_watermark(cr, s, rect.normalized(), text, *style);
        }
        Kind::Blur { .. } | Kind::Spotlight { .. } => {}
    }
    cr.restore().ok();
}

fn draw_arrow(cr: &cairo::Context, s: &Style, a: Pt, b: Pt, ctrl: Option<Pt>, kind: ArrowType, head_start: Head, head_end: Head) {
    let width = s.width.max(1.0);
    let head_len = (width * 4.0).max(10.0);
    set_color(cr, &s.color);
    // Direction at each end (tangent of the curve if present).
    let (dir_end, dir_start) = match ctrl {
        Some(c) => (angle(c, b), angle(c, a)),
        None => (angle(a, b), angle(b, a)),
    };
    match kind {
        ArrowType::Classic => {
            // Shorten the shaft so the line does not poke through the arrow tip.
            let mut sa = a;
            let mut sb = b;
            if head_end == Head::Arrow {
                sb = Pt::new(b.x - dir_end.cos() * head_len * 0.6, b.y - dir_end.sin() * head_len * 0.6);
            }
            if head_start == Head::Arrow {
                sa = Pt::new(a.x - dir_start.cos() * head_len * 0.6, a.y - dir_start.sin() * head_len * 0.6);
            }
            apply_line_style(cr, s);
            cr.move_to(sa.x, sa.y);
            match ctrl {
                Some(c) => cr.curve_to(
                    sa.x + 2.0 / 3.0 * (c.x - sa.x),
                    sa.y + 2.0 / 3.0 * (c.y - sa.y),
                    sb.x + 2.0 / 3.0 * (c.x - sb.x),
                    sb.y + 2.0 / 3.0 * (c.y - sb.y),
                    sb.x,
                    sb.y,
                ),
                None => cr.line_to(sb.x, sb.y),
            }
            cr.stroke().ok();
            cr.set_dash(&[], 0.0);
            draw_head(cr, b, dir_end, head_len, width, head_end);
            draw_head(cr, a, dir_start, head_len, width, head_start);
        }
        ArrowType::Tapered | ArrowType::Outlined => {
            // Build a filled polygon: wide at the tail, narrowing into the head.
            let pts: Vec<Pt> = match ctrl {
                Some(c) => bezier_points(a, c, b, 32),
                None => vec![a, b],
            };
            let total: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum::<f64>().max(1.0);
            let head_l = (head_len * 1.4).min(total * 0.5);
            let tail_w = width * 2.2;
            let mut left = Vec::new();
            let mut right = Vec::new();
            let mut travelled = 0.0;
            for (i, p) in pts.iter().enumerate() {
                if i > 0 {
                    travelled += pts[i - 1].dist(*p);
                }
                let t = travelled / total;
                let remaining = total - travelled;
                if remaining < head_l {
                    break;
                }
                let dir = if i + 1 < pts.len() { angle(*p, pts[i + 1]) } else { angle(pts[i - 1], *p) };
                let hw = tail_w * (1.0 - t * 0.6) / 2.0;
                let (nx, ny) = (-dir.sin(), dir.cos());
                left.push(Pt::new(p.x + nx * hw, p.y + ny * hw));
                right.push(Pt::new(p.x - nx * hw, p.y - ny * hw));
            }
            let base = Pt::new(b.x - dir_end.cos() * head_l, b.y - dir_end.sin() * head_l);
            let (nx, ny) = (-dir_end.sin(), dir_end.cos());
            let hw = head_l * 0.5;
            let path: Vec<Pt> = left
                .iter()
                .cloned()
                .chain([Pt::new(base.x + nx * hw, base.y + ny * hw), b, Pt::new(base.x - nx * hw, base.y - ny * hw)])
                .chain(right.iter().rev().cloned())
                .collect();
            if path.len() >= 3 {
                cr.move_to(path[0].x, path[0].y);
                for p in &path[1..] {
                    cr.line_to(p.x, p.y);
                }
                cr.close_path();
                if kind == ArrowType::Outlined {
                    set_color(cr, &Color::rgba(1.0, 1.0, 1.0, 0.9));
                    cr.fill_preserve().ok();
                    set_color(cr, &s.color);
                    cr.set_line_width((width * 0.6).max(1.5));
                    cr.set_line_join(cairo::LineJoin::Round);
                    cr.stroke().ok();
                } else {
                    cr.fill().ok();
                }
            }
        }
    }
}

fn angle(from: Pt, to: Pt) -> f64 {
    (to.y - from.y).atan2(to.x - from.x)
}

fn draw_head(cr: &cairo::Context, tip: Pt, dir: f64, len: f64, width: f64, head: Head) {
    match head {
        Head::None => {}
        Head::Arrow => {
            let spread = 0.45;
            let p1 = Pt::new(tip.x - len * (dir - spread).cos(), tip.y - len * (dir - spread).sin());
            let p2 = Pt::new(tip.x - len * (dir + spread).cos(), tip.y - len * (dir + spread).sin());
            cr.move_to(tip.x, tip.y);
            cr.line_to(p1.x, p1.y);
            cr.line_to(p2.x, p2.y);
            cr.close_path();
            cr.fill().ok();
        }
        Head::Circle => {
            cr.arc(tip.x, tip.y, (width * 1.6).max(4.0), 0.0, 2.0 * PI);
            cr.fill().ok();
        }
    }
}

fn draw_text(cr: &cairo::Context, s: &Style, pos: Pt, text: &str, presentation: TextPresentation, tail: Option<Pt>) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&font_desc(s, false)));
    layout.set_text(text);
    let (tw, th) = layout.pixel_size();
    let pad = text_padding(s, presentation);
    let (bw, bh) = (tw as f64 + pad * 2.0, th as f64 + pad * 2.0);
    cr.save().ok();
    if s.rotation.abs() > 0.01 {
        cr.translate(pos.x + bw / 2.0, pos.y + bh / 2.0);
        cr.rotate(s.rotation.to_radians());
        cr.translate(-(pos.x + bw / 2.0), -(pos.y + bh / 2.0));
    }
    match presentation {
        TextPresentation::Plain => {
            // Subtle outline for legibility on busy backgrounds.
            cr.move_to(pos.x, pos.y);
            pangocairo::functions::layout_path(cr, &layout);
            let outline = s.color.contrast().with_alpha(0.55);
            set_color(cr, &outline);
            cr.set_line_width((s.font_size * 0.08).max(1.0));
            cr.set_line_join(cairo::LineJoin::Round);
            cr.stroke().ok();
            set_color(cr, &s.color);
            cr.move_to(pos.x, pos.y);
            pangocairo::functions::show_layout(cr, &layout);
        }
        TextPresentation::Label | TextPresentation::Callout => {
            set_color(cr, &s.color);
            rounded_rect(cr, pos.x, pos.y, bw, bh, s.corner_radius.max(4.0));
            cr.fill().ok();
            if presentation == TextPresentation::Callout {
                let t = tail.unwrap_or(Pt::new(pos.x + bw / 2.0, pos.y + bh + 24.0));
                let c = Pt::new(pos.x + bw / 2.0, pos.y + bh / 2.0);
                let dir = angle(c, t);
                let base_w = (bh * 0.35).max(8.0);
                let (nx, ny) = (-dir.sin(), dir.cos());
                let anchor = Pt::new(c.x + dir.cos() * (bw.min(bh) * 0.25), c.y + dir.sin() * (bw.min(bh) * 0.25));
                cr.move_to(anchor.x + nx * base_w, anchor.y + ny * base_w);
                cr.line_to(t.x, t.y);
                cr.line_to(anchor.x - nx * base_w, anchor.y - ny * base_w);
                cr.close_path();
                cr.fill().ok();
            }
            set_color(cr, &s.color.contrast());
            cr.move_to(pos.x + pad, pos.y + pad);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }
    cr.restore().ok();
}

fn draw_watermark(cr: &cairo::Context, s: &Style, r: RectF, text: &str, style: WatermarkStyle) {
    if text.is_empty() || r.w < 2.0 || r.h < 2.0 {
        return;
    }
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&font_desc(s, true)));
    layout.set_text(text);
    let (tw, th) = layout.pixel_size();
    let (tw, th) = (tw as f64, th as f64);
    set_color(cr, &s.color.with_alpha(s.opacity.clamp(0.05, 1.0)));
    cr.rectangle(r.x, r.y, r.w, r.h);
    cr.clip();
    let rot = match style {
        WatermarkStyle::Single => s.rotation,
        _ => -24.0 + s.rotation,
    }
    .to_radians();
    match style {
        WatermarkStyle::Single | WatermarkStyle::Diagonal => {
            let c = r.center();
            cr.translate(c.x, c.y);
            cr.rotate(rot);
            cr.move_to(-tw / 2.0, -th / 2.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
        WatermarkStyle::Tiled => {
            let c = r.center();
            cr.translate(c.x, c.y);
            cr.rotate(rot);
            let span = (r.w + r.h) * 1.2;
            let step_x = tw + s.font_size * 2.0;
            let step_y = th + s.font_size * 1.5;
            let mut y = -span / 2.0;
            let mut row = 0;
            while y < span / 2.0 {
                let mut x = -span / 2.0 - if row % 2 == 0 { 0.0 } else { step_x / 2.0 };
                while x < span / 2.0 {
                    cr.move_to(x, y);
                    pangocairo::functions::show_layout(cr, &layout);
                    x += step_x;
                }
                y += step_y;
                row += 1;
            }
        }
    }
}
