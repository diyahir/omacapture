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
    // Phone numbers: 10-15 digits with only phone punctuation, and not a plain date.
    let phone_chars = t.chars().all(|c| c.is_ascii_digit() || " +()-.".contains(c));
    let formatted = t.starts_with('+') || t.contains('(') || t.contains(' ') || t.contains('.');
    let looks_like_date = t.matches('-').count() == 2 && digits.len() == 8;
    if phone_chars && !looks_like_date && ((formatted && (10..=15).contains(&digits.len())) || (t.chars().all(|c| c.is_ascii_digit()) && (10..=11).contains(&digits.len()))) {
        return true;
    }
    if digits.len() >= 13 && t.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '-') && luhn(&digits) {
        return true;
    }
    PATTERNS.with(|ps| ps.iter().any(|p| p.is_match(t)))
}

fn is_numberish(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || "+()-. ".contains(c)) && t.chars().any(|c| c.is_ascii_digit())
}

fn same_line(a: &Word, b: &Word) -> bool {
    (a.y - b.y).abs() < a.h.max(b.h).max(1) && b.x >= a.x
}

fn bbox(group: &[&Word]) -> RectF {
    let x0 = group.iter().map(|g| g.x).min().unwrap();
    let x1 = group.iter().map(|g| g.x + g.w).max().unwrap();
    let y0 = group.iter().map(|g| g.y).min().unwrap();
    let y1 = group.iter().map(|g| g.y + g.h).max().unwrap();
    RectF::new(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64)
}

/// Build blur items covering every sensitive-looking word, plus runs of numeric
/// words on one line that only read as a phone or card number when joined
/// (OCR splits "+1 (555) 123-4567" and "4111 1111 1111 1111" into pieces).
pub fn redaction_items(words: &[Word], style: &Style, strength: f64) -> Vec<(Kind, Style)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if is_numberish(&w.text) {
            // Extend the run as far as it stays numeric and on the same line.
            let mut j = i + 1;
            while j < words.len() && j - i < 6 && is_numberish(&words[j].text) && same_line(&words[j - 1], &words[j]) {
                j += 1;
            }
            // Try the longest run first so "+1 (555) 123-4567" wins over "(555) 123-4567".
            let mut matched = None;
            for end in (i + 2..=j).rev() {
                let group: Vec<&Word> = words[i..end].iter().collect();
                let joined = group.iter().map(|g| g.text.as_str()).collect::<Vec<_>>().join(" ");
                if is_sensitive(&joined) {
                    matched = Some((end, bbox(&group)));
                    break;
                }
            }
            if let Some((end, r)) = matched {
                out.push(blur_item(r, style, strength));
                i = end;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secrets() {
        assert!(is_sensitive("someone@example.com"));
        assert!(is_sensitive("+1 (555) 123-4567"));
        assert!(is_sensitive("AKIAIOSFODNN7EXAMPLE"));
        assert!(is_sensitive("ghp_abcdefghijklmnopqrstuvwxyz0123456789"));
        assert!(is_sensitive("4111111111111111"));
        assert!(is_sensitive("password=hunter2"));
        assert!(!is_sensitive("hello"));
        assert!(!is_sensitive("2026-09-08"));
        assert!(!is_sensitive("1234567890123"));
    }

    #[test]
    fn groups_card_numbers() {
        let w = |t: &str, x: i32| Word { text: t.into(), x, y: 10, w: 40, h: 12 };
        let words = vec![w("Card", 0), w("4111", 50), w("1111", 100), w("1111", 150), w("1111", 200), w("ok", 260)];
        let items = redaction_items(&words, &Style::default(), 6.0);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].0, Kind::Blur { rect, .. } if rect.x == 47.0 && rect.w == 196.0));
    }

    #[test]
    fn groups_split_phone_numbers() {
        let w = |t: &str, x: i32| Word { text: t.into(), x, y: 10, w: 40, h: 12 };
        let words = vec![w("phone", 0), w("+1", 50), w("(555)", 100), w("123-4567", 150), w("today", 260), w("2026", 320)];
        let items = redaction_items(&words, &Style::default(), 6.0);
        assert_eq!(items.len(), 1, "{items:?}");
        assert!(matches!(items[0].0, Kind::Blur { rect, .. } if rect.x == 47.0 && rect.w == 146.0));
    }
}
