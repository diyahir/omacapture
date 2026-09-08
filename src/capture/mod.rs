pub mod grim;
pub mod hypr;
pub mod overlay;

use serde::{Deserialize, Serialize};

/// Axis-aligned integer rectangle in logical (compositor) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn from_points(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        let x = x0.min(x1).round() as i32;
        let y = y0.min(y1).round() as i32;
        let w = (x0.max(x1).round() as i32 - x).max(1);
        let h = (y0.max(y1).round() as i32 - y).max(1);
        Self { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x as f64 && py >= self.y as f64 && px < self.right() as f64 && py < self.bottom() as f64
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r > x && b > y {
            Some(Rect::new(x, y, r - x, b - y))
        } else {
            None
        }
    }
    pub fn grim_geometry(&self) -> String {
        format!("{},{} {}x{}", self.x, self.y, self.w, self.h)
    }
}

/// Which way a capture was requested; drives the post-capture action matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Fullscreen,
    Area,
    Window,
    AnnotateExport,
}

/// A captured (or edited) raster image in memory: RGBA8, premultiplied = false.
#[derive(Clone)]
pub struct Frame {
    pub image: image::RgbaImage,
    /// Physical pixels per logical pixel for the source monitor.
    pub scale: f64,
}

impl Frame {
    pub fn width(&self) -> u32 {
        self.image.width()
    }
    pub fn height(&self) -> u32 {
        self.image.height()
    }

    /// Crop in physical pixel coordinates.
    pub fn crop_px(&self, x: u32, y: u32, w: u32, h: u32) -> Frame {
        let x = x.min(self.width().saturating_sub(1));
        let y = y.min(self.height().saturating_sub(1));
        let w = w.min(self.width() - x).max(1);
        let h = h.min(self.height() - y).max(1);
        Frame { image: image::imageops::crop_imm(&self.image, x, y, w, h).to_image(), scale: self.scale }
    }

    pub fn to_cairo_surface(&self) -> cairo::ImageSurface {
        rgba_to_surface(&self.image)
    }

    pub fn to_texture(&self) -> gtk::gdk::Texture {
        let bytes = glib::Bytes::from(self.image.as_raw());
        gtk::gdk::MemoryTexture::new(
            self.width() as i32,
            self.height() as i32,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            (self.width() * 4) as usize,
        )
        .into()
    }
}

/// Convert straight-alpha RGBA to a premultiplied BGRA cairo surface.
pub fn rgba_to_surface(img: &image::RgbaImage) -> cairo::ImageSurface {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut surf = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).expect("surface");
    let stride = surf.stride() as usize;
    {
        let mut data = surf.data().expect("surface data");
        for (y, row) in img.rows().enumerate() {
            let out = &mut data[y * stride..y * stride + (w as usize) * 4];
            for (x, p) in row.enumerate() {
                let a = p[3] as u32;
                let pm = |c: u8| ((c as u32 * a + 127) / 255) as u8;
                out[x * 4] = pm(p[2]);
                out[x * 4 + 1] = pm(p[1]);
                out[x * 4 + 2] = pm(p[0]);
                out[x * 4 + 3] = p[3];
            }
        }
    }
    surf
}

/// Convert a premultiplied BGRA cairo surface back into straight RGBA.
pub fn surface_to_rgba(surf: &mut cairo::ImageSurface) -> image::RgbaImage {
    surf.flush();
    let (w, h) = (surf.width() as u32, surf.height() as u32);
    let stride = surf.stride() as usize;
    let data = surf.data().expect("surface data");
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * stride + x * 4;
            let a = data[i + 3] as u32;
            let un = |c: u8| (c as u32 * 255 + a / 2).checked_div(a).map(|v| v.min(255) as u8).unwrap_or(0);
            img.put_pixel(x as u32, y as u32, image::Rgba([un(data[i + 2]), un(data[i + 1]), un(data[i]), a as u8]));
        }
    }
    img
}
