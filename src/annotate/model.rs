//! Annotation document model. Coordinates are source-image pixels.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn dist(&self, o: Pt) -> f64 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
    pub fn offset(&self, dx: f64, dy: f64) -> Pt {
        Pt::new(self.x + dx, self.y + dy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl RectF {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn from_points(a: Pt, b: Pt) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        Self { x, y, w: (a.x - b.x).abs(), h: (a.y - b.y).abs() }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn center(&self) -> Pt {
        Pt::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.y >= self.y && p.x <= self.right() && p.y <= self.bottom()
    }
    pub fn inflate(&self, d: f64) -> RectF {
        RectF::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }
    pub fn normalized(&self) -> RectF {
        let mut r = *self;
        if r.w < 0.0 {
            r.x += r.w;
            r.w = -r.w;
        }
        if r.h < 0.0 {
            r.y += r.h;
            r.h = -r.h;
        }
        r
    }
    pub fn union(&self, o: &RectF) -> RectF {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        RectF::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn intersects(&self, o: &RectF) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn translate(&self, dx: f64, dy: f64) -> RectF {
        RectF::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
    pub fn parse(hex: &str) -> Option<Color> {
        let h = hex.trim().trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).ok()?;
        match h.len() {
            6 => Some(Color::rgba(((v >> 16) & 255) as f64 / 255.0, ((v >> 8) & 255) as f64 / 255.0, (v & 255) as f64 / 255.0, 1.0)),
            8 => Some(Color::rgba(
                ((v >> 24) & 255) as f64 / 255.0,
                ((v >> 16) & 255) as f64 / 255.0,
                ((v >> 8) & 255) as f64 / 255.0,
                (v & 255) as f64 / 255.0,
            )),
            _ => None,
        }
    }
    pub fn to_hex(&self) -> String {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        if self.a >= 0.999 {
            format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b), c(self.a))
        }
    }
    pub fn with_alpha(&self, a: f64) -> Color {
        Color { a, ..*self }
    }
    pub fn to_gdk(&self) -> gtk::gdk::RGBA {
        gtk::gdk::RGBA::new(self.r as f32, self.g as f32, self.b as f32, self.a as f32)
    }
    pub fn from_gdk(c: &gtk::gdk::RGBA) -> Color {
        Color::rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64)
    }
    /// A readable text color on top of this color.
    pub fn contrast(&self) -> Color {
        let lum = 0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b;
        if lum > 0.55 {
            Color::rgba(0.05, 0.05, 0.05, 1.0)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 1.0)
        }
    }
}

pub const PALETTE: [&str; 10] =
    ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#00c7be", "#007aff", "#af52de", "#ff2d55", "#ffffff", "#000000"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArrowStyle {
    #[default]
    Straight,
    CurvedRight,
    CurvedLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArrowType {
    #[default]
    Classic,
    Tapered,
    Outlined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Head {
    None,
    #[default]
    Arrow,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextPresentation {
    #[default]
    Plain,
    Label,
    Callout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BlurEffect {
    #[default]
    Pixelate,
    Gaussian,
    Hexagonal,
    Crystallize,
    Pointillism,
    Halftone,
    Tape,
    Washi,
}

impl BlurEffect {
    pub const ALL: [BlurEffect; 8] = [
        BlurEffect::Pixelate,
        BlurEffect::Gaussian,
        BlurEffect::Hexagonal,
        BlurEffect::Crystallize,
        BlurEffect::Pointillism,
        BlurEffect::Halftone,
        BlurEffect::Tape,
        BlurEffect::Washi,
    ];
    pub fn label(self) -> &'static str {
        match self {
            BlurEffect::Pixelate => "Pixelate",
            BlurEffect::Gaussian => "Gaussian",
            BlurEffect::Hexagonal => "Hexagonal",
            BlurEffect::Crystallize => "Crystallize",
            BlurEffect::Pointillism => "Pointillism",
            BlurEffect::Halftone => "Halftone",
            BlurEffect::Tape => "Tape",
            BlurEffect::Washi => "Washi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WatermarkStyle {
    #[default]
    Single,
    Diagonal,
    Tiled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub color: Color,
    pub width: f64,
    pub line_style: LineStyle,
    pub corner_radius: f64,
    pub font_size: f64,
    pub font_family: String,
    pub opacity: f64,
    pub rotation: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: Color::parse("#ff3b30").unwrap(),
            width: 3.0,
            line_style: LineStyle::Solid,
            corner_radius: 0.0,
            font_size: 16.0,
            font_family: "Sans".into(),
            opacity: 1.0,
            rotation: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Rect { rect: RectF, filled: bool },
    Oval { rect: RectF },
    Line { a: Pt, b: Pt },
    Arrow { a: Pt, b: Pt, ctrl: Option<Pt>, style: ArrowStyle, kind: ArrowType, head_start: Head, head_end: Head },
    Text { pos: Pt, text: String, presentation: TextPresentation, tail: Option<Pt> },
    Highlight { points: Vec<Pt> },
    Blur { rect: RectF, effect: BlurEffect, strength: f64 },
    Spotlight { rect: RectF },
    Counter { center: Pt, number: u32, size: f64 },
    Watermark { rect: RectF, text: String, style: WatermarkStyle },
    Pencil { points: Vec<Pt> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub kind: Kind,
    pub style: Style,
}

impl Item {
    /// Axis-aligned bounds used for hit testing and selection handles.
    pub fn bounds(&self) -> RectF {
        match &self.kind {
            Kind::Rect { rect, .. }
            | Kind::Oval { rect }
            | Kind::Blur { rect, .. }
            | Kind::Spotlight { rect }
            | Kind::Watermark { rect, .. } => rect.normalized(),
            Kind::Line { a, b } => RectF::from_points(*a, *b),
            Kind::Arrow { a, b, ctrl, .. } => {
                let mut r = RectF::from_points(*a, *b);
                if let Some(c) = ctrl {
                    r = r.union(&RectF::new(c.x, c.y, 0.0, 0.0));
                }
                r
            }
            Kind::Text { pos, .. } => RectF::new(pos.x, pos.y, 10.0, self.style.font_size * 1.4),
            Kind::Highlight { points } | Kind::Pencil { points } => points_bounds(points).inflate(self.style.width),
            Kind::Counter { center, size, .. } => {
                let d = 12.0 + 4.0 * size;
                RectF::new(center.x - d / 2.0, center.y - d / 2.0, d, d)
            }
        }
    }

    pub fn is_resizable(&self) -> bool {
        matches!(self.kind, Kind::Rect { .. } | Kind::Oval { .. } | Kind::Blur { .. } | Kind::Spotlight { .. } | Kind::Watermark { .. })
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        match &mut self.kind {
            Kind::Rect { rect, .. }
            | Kind::Oval { rect }
            | Kind::Blur { rect, .. }
            | Kind::Spotlight { rect }
            | Kind::Watermark { rect, .. } => *rect = rect.translate(dx, dy),
            Kind::Line { a, b } => {
                *a = a.offset(dx, dy);
                *b = b.offset(dx, dy);
            }
            Kind::Arrow { a, b, ctrl, .. } => {
                *a = a.offset(dx, dy);
                *b = b.offset(dx, dy);
                if let Some(c) = ctrl {
                    *c = c.offset(dx, dy);
                }
            }
            Kind::Text { pos, tail, .. } => {
                *pos = pos.offset(dx, dy);
                if let Some(t) = tail {
                    *t = t.offset(dx, dy);
                }
            }
            Kind::Highlight { points } | Kind::Pencil { points } => {
                for p in points {
                    *p = p.offset(dx, dy);
                }
            }
            Kind::Counter { center, .. } => *center = center.offset(dx, dy),
        }
    }

    /// Replace the rectangle of a rect-like item (used by resize handles).
    pub fn set_rect(&mut self, r: RectF) {
        match &mut self.kind {
            Kind::Rect { rect, .. }
            | Kind::Oval { rect }
            | Kind::Blur { rect, .. }
            | Kind::Spotlight { rect }
            | Kind::Watermark { rect, .. } => *rect = r,
            _ => {}
        }
    }

    pub fn is_blur(&self) -> bool {
        matches!(self.kind, Kind::Blur { .. })
    }
    pub fn is_spotlight(&self) -> bool {
        matches!(self.kind, Kind::Spotlight { .. })
    }
}

pub fn points_bounds(points: &[Pt]) -> RectF {
    if points.is_empty() {
        return RectF::default();
    }
    let mut r = RectF::new(points[0].x, points[0].y, 0.0, 0.0);
    for p in points {
        r = r.union(&RectF::new(p.x, p.y, 0.0, 0.0));
    }
    r
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum Background {
    #[default]
    None,
    Solid {
        color: Color,
    },
    Gradient {
        from: Color,
        to: Color,
        angle: f64,
    },
    Blurred {
        strength: f64,
        dim: f64,
    },
    Image {
        path: std::path::PathBuf,
    },
    /// The current Omarchy wallpaper, blurred, framing the screenshot.
    Wallpaper {
        strength: f64,
        dim: f64,
    },
}

pub const GRADIENTS: [(&str, &str, &str); 8] = [
    ("Pink Orange", "#ff6ec4", "#ff9a3c"),
    ("Blue Purple", "#4facfe", "#8e5cf6"),
    ("Green Blue", "#43e97b", "#38b6ff"),
    ("Orange Red", "#ffb347", "#ff3d3d"),
    ("Purple Pink", "#a18cd1", "#fbc2eb"),
    ("Blue Green", "#2b86c5", "#3ad59f"),
    ("Yellow Orange", "#f6d365", "#fda085"),
    ("Cyan Blue", "#22d3ee", "#3b82f6"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Canvas {
    pub background: Background,
    pub padding: f64,
    pub corner_radius: f64,
    pub shadow: f64,
    /// Aspect ratio of the outer canvas; None keeps it tight around the padded image.
    pub aspect: Option<(f64, f64)>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self { background: Background::None, padding: 0.0, corner_radius: 0.0, shadow: 0.3, aspect: None }
    }
}

impl Canvas {
    /// The one-click "Omarchy frame": the whole current wallpaper, lightly
    /// blurred, at its own aspect ratio, with the capture floating on top.
    pub fn omarchy_frame(capture_w: f64, capture_h: f64) -> Canvas {
        let aspect = crate::theme::wallpaper_size().map(|(w, h)| (w as f64, h as f64)).unwrap_or((16.0, 9.0));
        // Padding relative to the capture so small and large shots frame alike.
        let padding = (capture_w.max(capture_h) * 0.05).clamp(16.0, 320.0);
        Canvas {
            background: Background::Wallpaper { strength: 3.0, dim: 0.12 },
            padding,
            corner_radius: 0.0,
            shadow: 0.55,
            aspect: Some(aspect),
        }
    }

    pub fn is_omarchy_frame(&self) -> bool {
        matches!(self.background, Background::Wallpaper { .. })
    }

    pub fn is_plain(&self) -> bool {
        self.background == Background::None && self.padding <= 0.0 && self.corner_radius <= 0.0 && self.aspect.is_none()
    }
}

/// Everything about the drawing except the pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Sheet {
    pub items: Vec<Item>,
    pub crop: Option<RectF>,
    pub canvas: Canvas,
    pub spotlight_dim: f64,
    pub rotation_quarters: i32,
    pub next_id: u64,
    pub next_counter: u32,
}

pub struct Document {
    pub source: crate::capture::Frame,
    pub sheet: Sheet,
    undo: Vec<Sheet>,
    redo: Vec<Sheet>,
    pub dirty: bool,
}

impl Document {
    pub fn new(source: crate::capture::Frame) -> Self {
        Self {
            source,
            sheet: Sheet { spotlight_dim: 0.5, next_id: 1, next_counter: 1, ..Default::default() },
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
        }
    }

    pub fn width(&self) -> f64 {
        self.source.width() as f64
    }
    pub fn height(&self) -> f64 {
        self.source.height() as f64
    }
    pub fn image_rect(&self) -> RectF {
        RectF::new(0.0, 0.0, self.width(), self.height())
    }
    /// Visible (cropped) rectangle of the source.
    pub fn crop_rect(&self) -> RectF {
        self.sheet.crop.unwrap_or_else(|| self.image_rect())
    }

    /// Record the state before a mutation.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.sheet.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    pub fn undo(&mut self) -> bool {
        if let Some(s) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.sheet, s));
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(s) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.sheet, s));
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn add(&mut self, kind: Kind, style: Style) -> u64 {
        let id = self.sheet.next_id;
        self.sheet.next_id += 1;
        self.sheet.items.push(Item { id, kind, style });
        id
    }

    pub fn item(&self, id: u64) -> Option<&Item> {
        self.sheet.items.iter().find(|i| i.id == id)
    }
    pub fn item_mut(&mut self, id: u64) -> Option<&mut Item> {
        self.sheet.items.iter_mut().find(|i| i.id == id)
    }
    pub fn remove(&mut self, id: u64) {
        self.sheet.items.retain(|i| i.id != id);
    }

    pub fn duplicate(&mut self, ids: &[u64], offset: f64) -> Vec<u64> {
        let mut out = Vec::new();
        let clones: Vec<Item> = self.sheet.items.iter().filter(|i| ids.contains(&i.id)).cloned().collect();
        for mut c in clones {
            c.translate(offset, offset);
            out.push(self.add(c.kind, c.style));
        }
        out
    }

    /// Top-most item under a point, with a hit tolerance in image pixels.
    pub fn hit_test(&self, p: Pt, tol: f64) -> Option<u64> {
        self.sheet.items.iter().rev().find(|it| hit_item(it, p, tol)).map(|it| it.id)
    }
}

fn dist_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-9 {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.dist(Pt::new(a.x + t * dx, a.y + t * dy))
}

pub fn dist_to_polyline(p: Pt, pts: &[Pt]) -> f64 {
    if pts.len() == 1 {
        return p.dist(pts[0]);
    }
    pts.windows(2).map(|w| dist_to_segment(p, w[0], w[1])).fold(f64::INFINITY, f64::min)
}

pub fn hit_item(it: &Item, p: Pt, tol: f64) -> bool {
    let tol = tol.max(it.style.width / 2.0 + 2.0);
    match &it.kind {
        Kind::Rect { rect, filled } => {
            let r = rect.normalized();
            if *filled {
                r.inflate(tol).contains(p)
            } else {
                r.inflate(tol).contains(p) && !r.inflate(-tol).contains(p)
            }
        }
        Kind::Oval { rect } => {
            let r = rect.normalized();
            let c = r.center();
            let (rx, ry) = ((r.w / 2.0).max(1.0), (r.h / 2.0).max(1.0));
            let d = ((p.x - c.x) / rx).powi(2) + ((p.y - c.y) / ry).powi(2);
            let band = tol / rx.min(ry);
            (d.sqrt() - 1.0).abs() <= band.max(0.08)
        }
        Kind::Line { a, b } => dist_to_segment(p, *a, *b) <= tol,
        Kind::Arrow { a, b, ctrl, .. } => match ctrl {
            Some(c) => dist_to_polyline(p, &bezier_points(*a, *c, *b, 24)) <= tol,
            None => dist_to_segment(p, *a, *b) <= tol,
        },
        Kind::Text { .. } => it.bounds().inflate(tol).contains(p),
        Kind::Highlight { points } => dist_to_polyline(p, points) <= tol.max(it.style.width * 1.5),
        Kind::Pencil { points } => dist_to_polyline(p, points) <= tol,
        Kind::Blur { rect, .. } | Kind::Spotlight { rect } | Kind::Watermark { rect, .. } => rect.normalized().inflate(tol).contains(p),
        Kind::Counter { center, size, .. } => p.dist(*center) <= (12.0 + 4.0 * size) / 2.0 + tol,
    }
}

pub fn bezier_points(a: Pt, c: Pt, b: Pt, n: usize) -> Vec<Pt> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            Pt::new(u * u * a.x + 2.0 * u * t * c.x + t * t * b.x, u * u * a.y + 2.0 * u * t * c.y + t * t * b.y)
        })
        .collect()
}
