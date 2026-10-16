//! Screen capture through the `grim` CLI (wlr-screencopy).

use super::{Frame, Rect};
use anyhow::{bail, Context, Result};
use std::process::Command;

fn run(args: &[&str]) -> Result<image::RgbaImage> {
    let out =
        Command::new("grim").args(args).args(["-t", "png", "-l", "0", "-"]).output().context("failed to run `grim`; is it installed?")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn grim_uses_uncompressed_png() {
        if std::env::var_os("OMACAPTURE_GRIM_TEST").is_some() {
            let output = capture_output("DP-1", 2.0, true).unwrap();
            let region = capture_region(Rect::new(1, 2, 3, 4), 1.0, false).unwrap();
            for frame in [output, region] {
                assert_eq!(frame.image.dimensions(), (1, 1));
                assert_eq!(frame.image.get_pixel(0, 0).0, [12, 34, 56, 255]);
            }
            return;
        }

        let dir = std::env::temp_dir().join(format!("omacapture-grim-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("frame.png");
        image::RgbaImage::from_pixel(1, 1, image::Rgba([12, 34, 56, 255])).save(&png).unwrap();
        let args = dir.join("args");
        let script = dir.join("grim");
        std::fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$OMACAPTURE_GRIM_ARGS\"\ncat \"$OMACAPTURE_GRIM_PNG\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![dir.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "capture::grim::tests::grim_uses_uncompressed_png", "--nocapture"])
            .env("OMACAPTURE_GRIM_TEST", "1")
            .env("OMACAPTURE_GRIM_ARGS", &args)
            .env("OMACAPTURE_GRIM_PNG", &png)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .output()
            .unwrap();
        let invoked = std::fs::read_to_string(&args).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stdout));
        assert_eq!(invoked, "-o\nDP-1\n-c\n-t\npng\n-l\n0\n-\n-g\n1,2 3x4\n-t\npng\n-l\n0\n-\n");
    }
}
