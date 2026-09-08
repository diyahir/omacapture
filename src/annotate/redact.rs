//! Automatic detection of sensitive text for one-click redaction.

use super::model::{BlurEffect, Kind, RectF, Style};
use crate::ocr::Word;
use regex::Regex;

fn luhn(digits: &str) -> bool {
    let d: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() < 13 || d.len() > 19 {
        return false;
    }
    let mut sum = 0;
    for (i, v) in d.iter().rev().enumerate() {
        let mut x = *v;
        if i % 2 == 1 {
            x *= 2;
            if x > 9 {
                x -= 9;
            }
        }
        sum += x;
    }
    sum % 10 == 0
}

pub fn is_sensitive(token: &str) -> bool {
    thread_local! {
        static PATTERNS: Vec<Regex> = vec![
            Regex::new(r"^[\w.+-]+@[\w-]+\.[\w.-]+$").unwrap(),                       // email
            Regex::new(r"^\+?\d[\d\s().-]{8,}\d$").unwrap(),                            // phone
            Regex::new(r"^(https?://|www\.)\S+$").unwrap(),                             // url
            Regex::new(r"^(AKIA|ASIA)[A-Z0-9]{16}$").unwrap(),                          // aws key id
            Regex::new(r"^(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}$").unwrap(),           // github token
            Regex::new(r"^github_pat_[A-Za-z0-9_]{20,}$").unwrap(),
            Regex::new(r"^xox[baprs]-[A-Za-z0-9-]{10,}$").unwrap(),                     // slack
            Regex::new(r"^(sk|pk|rk)_(live|test)_[A-Za-z0-9]{10,}$").unwrap(),          // stripe
            Regex::new(r"^sk-[A-Za-z0-9_-]{20,}$").unwrap(),                            // openai
            Regex::new(r"^eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}$").unwrap(), // jwt
            Regex::new(r"^(?i)(password|passwd|pwd|secret|token|api[_-]?key|apikey|auth)\s*[=:]\s*\S+$").unwrap(),
            Regex::new(r"^[A-Fa-f0-9]{32,}$").unwrap(),                                 // long hex secrets
        ];
    }
    let t = token.trim_matches(|c: char| c == ',' || c == ';' || c == '"' || c == '\'' || c == '(' || c == ')');
    if t.is_empty() {
        return false;
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 13 && t.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '-') && luhn(&digits) {
        return true;
    }
    PATTERNS.with(|ps| ps.iter().any(|p| p.is_match(t)))
}

/// Build blur items covering every sensitive-looking word (and credit card number groups).
pub fn redaction_items(words: &[Word], style: &Style, strength: f64) -> Vec<(Kind, Style)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        // Card numbers are often split into 4 groups on one line.
        if w.text.chars().all(|c| c.is_ascii_digit()) && w.text.len() == 4 && i + 3 < words.len() {
            let group: Vec<&Word> = words[i..i + 4].iter().collect();
            let joined: String = group.iter().map(|g| g.text.as_str()).collect();
            let same_line = group.iter().all(|g| (g.y - w.y).abs() < w.h.max(1));
            if same_line && joined.len() == 16 && luhn(&joined) {
                let x0 = group.iter().map(|g| g.x).min().unwrap();
                let x1 = group.iter().map(|g| g.x + g.w).max().unwrap();
                let y0 = group.iter().map(|g| g.y).min().unwrap();
                let y1 = group.iter().map(|g| g.y + g.h).max().unwrap();
                out.push(blur_item(RectF::new(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64), style, strength));
                i += 4;
                continue;
            }
        }
        if is_sensitive(&w.text) {
            out.push(blur_item(RectF::new(w.x as f64, w.y as f64, w.w as f64, w.h as f64), style, strength));
        }
        i += 1;
    }
    out
}

fn blur_item(r: RectF, style: &Style, strength: f64) -> (Kind, Style) {
    (Kind::Blur { rect: r.inflate(3.0), effect: BlurEffect::Pixelate, strength }, style.clone())
}
