//! Text recognition via the `tesseract` CLI.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn recognize(png: &[u8], languages: &str) -> Result<String> {
    let mut child = Command::new("tesseract")
        .args(["stdin", "stdout", "-l", languages, "--psm", "6"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `tesseract`; install tesseract and a language pack")?;
    child.stdin.take().unwrap().write_all(png)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("tesseract failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A recognized word with its bounding box in image pixels.
#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Word-level boxes, used for highlighter text snapping and redaction.
pub fn words(png: &[u8], languages: &str) -> Result<Vec<Word>> {
    let mut child = Command::new("tesseract")
        .args(["stdin", "stdout", "-l", languages, "--psm", "6", "tsv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to run `tesseract`")?;
    child.stdin.take().unwrap().write_all(png)?;
    let out = child.wait_with_output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut words = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let t = cols[11].trim();
        if t.is_empty() {
            continue;
        }
        let n = |i: usize| cols[i].parse::<i32>().unwrap_or(0);
        words.push(Word { text: t.to_string(), x: n(6), y: n(7), w: n(8), h: n(9) });
    }
    Ok(words)
}
