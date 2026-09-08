//! Screen capture through the `grim` CLI (wlr-screencopy).

use super::{Frame, Rect};
use anyhow::{bail, Context, Result};
use std::process::Command;

fn run(args: &[&str]) -> Result<image::RgbaImage> {
    let out = Command::new("grim").args(args).arg("-t").arg("png").arg("-").output().context("failed to run `grim`; is it installed?")?;
    if !out.status.success() {
        bail!("grim failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let img = image::load_from_memory(&out.stdout).context("decoding grim output")?;
    Ok(img.to_rgba8())
}

/// Capture one named output at its native physical resolution.
pub fn capture_output(name: &str, scale: f64, cursor: bool) -> Result<Frame> {
    let mut args = vec!["-o", name];
    if cursor {
        args.push("-c");
    }
    Ok(Frame { image: run(&args)?, scale })
}

/// Capture a logical-coordinate region across outputs.
pub fn capture_region(rect: Rect, scale: f64, cursor: bool) -> Result<Frame> {
    let geom = rect.grim_geometry();
    let mut args = vec!["-g", geom.as_str()];
    if cursor {
        args.push("-c");
    }
    Ok(Frame { image: run(&args)?, scale })
}
