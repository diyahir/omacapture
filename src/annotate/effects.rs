//! Pixel effects for blur / redaction regions.

use super::model::BlurEffect;
use image::{Rgba, RgbaImage};

fn avg(img: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> Rgba<u8> {
    let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for y in y0..y1.min(img.height()) {
        for x in x0..x1.min(img.width()) {
            let p = img.get_pixel(x, y);
            r += p[0] as u64;
            g += p[1] as u64;
            b += p[2] as u64;
            a += p[3] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    Rgba([(r / n) as u8, (g / n) as u8, (b / n) as u8, (a / n) as u8])
}

pub fn pixelate(img: &RgbaImage, block: u32) -> RgbaImage {
    let block = block.max(2);
    let mut out = img.clone();
    let (w, h) = (img.width(), img.height());
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let c = avg(img, x, y, x + block, y + block);
            for yy in y..(y + block).min(h) {
                for xx in x..(x + block).min(w) {
                    out.put_pixel(xx, yy, c);
                }
            }
            x += block;
        }
        y += block;
    }
    out
}

/// Three-pass box blur approximating a gaussian of the given radius.
pub fn gaussian(img: &RgbaImage, radius: u32) -> RgbaImage {
    let r = radius.max(1) as i64;
    let mut cur = img.clone();
    for _ in 0..3 {
        cur = box_blur_h(&cur, r);
        cur = box_blur_v(&cur, r);
    }
    cur
}

fn box_blur_h(img: &RgbaImage, r: i64) -> RgbaImage {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let mut out = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        let mut acc = [0i64; 4];
        for x in -r..=r {
            let p = img.get_pixel(x.clamp(0, w - 1) as u32, y as u32);
            for c in 0..4 {
                acc[c] += p[c] as i64;
            }
        }
        let n = 2 * r + 1;
        for x in 0..w {
            out.put_pixel(x as u32, y as u32, Rgba([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, (acc[3] / n) as u8]));
            let out_p = img.get_pixel((x - r).clamp(0, w - 1) as u32, y as u32);
            let in_p = img.get_pixel((x + r + 1).clamp(0, w - 1) as u32, y as u32);
            for c in 0..4 {
                acc[c] += in_p[c] as i64 - out_p[c] as i64;
            }
        }
    }
    out
}

fn box_blur_v(img: &RgbaImage, r: i64) -> RgbaImage {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let mut out = RgbaImage::new(w as u32, h as u32);
    for x in 0..w {
        let mut acc = [0i64; 4];
        for y in -r..=r {
            let p = img.get_pixel(x as u32, y.clamp(0, h - 1) as u32);
            for c in 0..4 {
                acc[c] += p[c] as i64;
            }
        }
        let n = 2 * r + 1;
        for y in 0..h {
            out.put_pixel(x as u32, y as u32, Rgba([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, (acc[3] / n) as u8]));
            let out_p = img.get_pixel(x as u32, (y - r).clamp(0, h - 1) as u32);
            let in_p = img.get_pixel(x as u32, (y + r + 1).clamp(0, h - 1) as u32);
            for c in 0..4 {
                acc[c] += in_p[c] as i64 - out_p[c] as i64;
            }
        }
    }
    out
}

/// Voronoi-style cells around jittered grid seeds.
pub fn crystallize(img: &RgbaImage, cell: u32) -> RgbaImage {
    let cell = cell.max(4) as i64;
    let (w, h) = (img.width() as i64, img.height() as i64);
    let cols = (w + cell - 1) / cell + 2;
    let rows = (h + cell - 1) / cell + 2;
    let mut seeds = Vec::with_capacity((cols * rows) as usize);
    let mut rng = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % 1000) as f64 / 1000.0
    };
    for gy in -1..rows - 1 {
        for gx in -1..cols - 1 {
            seeds.push(((gx as f64 + next()) * cell as f64, (gy as f64 + next()) * cell as f64));
        }
    }
    let seed_color: Vec<Rgba<u8>> = seeds
        .iter()
        .map(|(sx, sy)| {
            let x = (*sx as i64).clamp(0, w - 1);
            let y = (*sy as i64).clamp(0, h - 1);
            avg(img, (x - cell / 2).max(0) as u32, (y - cell / 2).max(0) as u32, (x + cell / 2) as u32, (y + cell / 2) as u32)
        })
        .collect();
    let mut out = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let gx = x / cell + 1;
            let gy = y / cell + 1;
            let mut best = (f64::INFINITY, 0usize);
            for oy in -1..=1 {
                for ox in -1..=1 {
                    let idx = ((gy + oy) * cols + (gx + ox)) as usize;
                    if idx < seeds.len() {
                        let (sx, sy) = seeds[idx];
                        let d = (sx - x as f64).powi(2) + (sy - y as f64).powi(2);
                        if d < best.0 {
                            best = (d, idx);
                        }
                    }
                }
            }
            out.put_pixel(x as u32, y as u32, seed_color[best.1]);
        }
    }
    out
}

/// Hexagonal cells.
pub fn hexagonal(img: &RgbaImage, size: u32) -> RgbaImage {
    let s = size.max(3) as f64;
    let (w, h) = (img.width(), img.height());
    let hw = 3f64.sqrt() * s;
    let mut out = RgbaImage::new(w, h);
    let mut cache: std::collections::HashMap<(i64, i64), Rgba<u8>> = std::collections::HashMap::new();
    for y in 0..h {
        for x in 0..w {
            // Axial hex coordinates (pointy-top).
            let px = x as f64;
            let py = y as f64;
            let q = (3f64.sqrt() / 3.0 * px - 1.0 / 3.0 * py) / s;
            let r = (2.0 / 3.0 * py) / s;
            let (cq, cr) = hex_round(q, r);
            let color = *cache.entry((cq, cr)).or_insert_with(|| {
                let cx = s * (3f64.sqrt() * cq as f64 + 3f64.sqrt() / 2.0 * cr as f64);
                let cy = s * 1.5 * cr as f64;
                avg(
                    img,
                    (cx - hw / 2.0).max(0.0) as u32,
                    (cy - s / 2.0).max(0.0) as u32,
                    (cx + hw / 2.0).max(1.0) as u32,
                    (cy + s / 2.0).max(1.0) as u32,
                )
            });
            out.put_pixel(x, y, color);
        }
    }
    out
}

fn hex_round(q: f64, r: f64) -> (i64, i64) {
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    (rq as i64, rr as i64)
}

/// Dots of the local average color over a blurred base.
pub fn pointillism(img: &RgbaImage, dot: u32) -> RgbaImage {
    let dot = dot.max(3);
    let mut out = gaussian(img, dot / 2 + 2);
    let (w, h) = (img.width(), img.height());
    let r = dot as f64 / 2.0;
    let mut y = 0u32;
    let mut row = 0;
    while y < h + dot {
        let mut x = if row % 2 == 0 { 0 } else { dot / 2 };
        while x < w + dot {
            let c = avg(img, x.saturating_sub(dot / 2), y.saturating_sub(dot / 2), x + dot / 2, y + dot / 2);
            for yy in y.saturating_sub(dot / 2)..(y + dot / 2).min(h) {
                for xx in x.saturating_sub(dot / 2)..(x + dot / 2).min(w) {
                    if ((xx as f64 - x as f64).powi(2) + (yy as f64 - y as f64).powi(2)).sqrt() <= r * 0.9 {
                        out.put_pixel(xx, yy, c);
                    }
                }
            }
            x += dot;
        }
        y += dot;
        row += 1;
    }
    out
}

/// Monochrome halftone: dot size follows luminance.
pub fn halftone(img: &RgbaImage, cell: u32) -> RgbaImage {
    let cell = cell.max(3);
    let (w, h) = (img.width(), img.height());
    let mut out = RgbaImage::from_pixel(w, h, Rgba([245, 245, 245, 255]));
    let mut y = 0u32;
    while y < h {
        let mut x = 0u32;
        while x < w {
            let c = avg(img, x, y, x + cell, y + cell);
            let lum = (0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64) / 255.0;
            let radius = (1.0 - lum) * cell as f64 / 2.0 * 1.15;
            let (cx, cy) = (x as f64 + cell as f64 / 2.0, y as f64 + cell as f64 / 2.0);
            for yy in y..(y + cell).min(h) {
                for xx in x..(x + cell).min(w) {
                    if ((xx as f64 + 0.5 - cx).powi(2) + (yy as f64 + 0.5 - cy).powi(2)).sqrt() <= radius {
                        out.put_pixel(xx, yy, Rgba([30, 30, 30, 255]));
                    }
                }
            }
            x += cell;
        }
        y += cell;
    }
    out
}

/// Opaque "masking tape" strip with a paper texture.
pub fn tape(img: &RgbaImage, seed: u32) -> RgbaImage {
    let (w, h) = (img.width(), img.height());
    let mut out = RgbaImage::new(w, h);
    let mut rng = 0x2545F4914F6CDD1Du64 ^ seed as u64;
    for y in 0..h {
        for x in 0..w {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let noise = (rng % 23) as i32 - 11;
            let stripe = if (x / 3 + y / 7) % 11 == 0 { -8 } else { 0 };
            let base = [232i32, 214, 168];
            let px = |v: i32| (v + noise + stripe).clamp(0, 255) as u8;
            out.put_pixel(x, y, Rgba([px(base[0]), px(base[1]), px(base[2]), 250]));
        }
    }
    out
}

/// Pastel washi-paper strip with fibrous noise.
pub fn washi(img: &RgbaImage, seed: u32) -> RgbaImage {
    let (w, h) = (img.width(), img.height());
    let mut out = RgbaImage::new(w, h);
    let mut rng = 0x9E3779B97F4A7C15u64 ^ seed as u64;
    let palette = [[246u8, 196, 210], [200, 224, 246], [214, 240, 204], [250, 232, 190]];
    let base = palette[(seed as usize) % palette.len()];
    for y in 0..h {
        for x in 0..w {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let n = (rng % 31) as i32 - 15;
            let fiber = if ((x * 7 + y * 13) % 29) < 2 { -18 } else { 0 };
            let px = |v: u8| (v as i32 + n + fiber).clamp(0, 255) as u8;
            out.put_pixel(x, y, Rgba([px(base[0]), px(base[1]), px(base[2]), 240]));
        }
    }
    out
}

pub fn apply(img: &RgbaImage, effect: BlurEffect, strength: f64, seed: u32) -> RgbaImage {
    let n = strength.clamp(1.0, 20.0) as u32;
    match effect {
        BlurEffect::Pixelate => pixelate(img, 6 + 2 * n),
        BlurEffect::Gaussian => gaussian(img, 8 + 4 * n),
        BlurEffect::Hexagonal => hexagonal(img, 4 + n),
        BlurEffect::Crystallize => crystallize(img, 8 + 2 * n),
        BlurEffect::Pointillism => pointillism(img, 6 + n),
        BlurEffect::Halftone => halftone(img, 4 + n),
        BlurEffect::Tape => tape(img, seed),
        BlurEffect::Washi => washi(img, seed),
    }
}
