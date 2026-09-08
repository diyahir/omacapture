//! Clipboard access through `wl-copy`, which persists after we exit.

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

fn wl_copy(mime: &str, bytes: &[u8]) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .arg("--type")
        .arg(mime)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to run `wl-copy`; install wl-clipboard")?;
    child.stdin.take().unwrap().write_all(bytes)?;
    // wl-copy forks and serves the selection on its own; don't wait for it.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn copy_png(bytes: &[u8]) -> Result<()> {
    wl_copy("image/png", bytes)
}

pub fn copy_text(text: &str) -> Result<()> {
    wl_copy("text/plain;charset=utf-8", text.as_bytes())
}
