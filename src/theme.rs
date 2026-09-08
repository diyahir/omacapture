//! Follow the active Omarchy theme: colors from its `colors.toml`, square
//! corners like the shell, and the system monospace font. Falls back to plain
//! libadwaita when no theme is present.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use serde::Deserialize;
use std::cell::RefCell;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ThemeColors {
    pub mode: String,
    pub accent: String,
    pub selection: String,
    pub muted: String,
    pub background: String,
    pub dark_background: String,
    pub darker_background: String,
    pub lighter_background: String,
    pub foreground: String,
    pub dark_foreground: String,
    pub light_foreground: String,
    pub bright_foreground: String,
    pub red: String,
    pub yellow: String,
    pub orange: String,
    pub green: String,
    pub cyan: String,
    pub blue: String,
    pub magenta: String,
    pub brown: String,
}

impl ThemeColors {
    pub fn is_dark(&self) -> bool {
        self.mode != "light"
    }

    /// Editor swatches: the theme's accent colors, then foreground and background.
    pub fn palette(&self) -> Vec<String> {
        [&self.red, &self.orange, &self.yellow, &self.green, &self.cyan, &self.blue, &self.magenta, &self.foreground, &self.background]
            .into_iter()
            .filter(|c| !c.is_empty())
            .cloned()
            .collect()
    }
}

thread_local! {
    static CURRENT: RefCell<Option<ThemeColors>> = const { RefCell::new(None) };
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static MONITOR: RefCell<Option<gio::FileMonitor>> = const { RefCell::new(None) };
}

pub fn colors_path() -> Option<PathBuf> {
    let state = dirs::state_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/state"));
    let candidates = [
        state.join("omarchy/current/theme/colors.toml"),
        dirs::config_dir().unwrap_or_default().join("omarchy/current/theme/colors.toml"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub fn load() -> Option<ThemeColors> {
    let text = std::fs::read_to_string(colors_path()?).ok()?;
    toml::from_str(&text).ok()
}

/// The theme currently applied, if any.
pub fn current() -> Option<ThemeColors> {
    CURRENT.with(|c| c.borrow().clone())
}

pub fn accent_rgb() -> (f64, f64, f64) {
    current()
        .and_then(|t| parse_rgb(&t.accent))
        .unwrap_or((0.35, 0.65, 1.0))
}

pub fn parse_rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() < 6 {
        return None;
    }
    let v = u32::from_str_radix(&h[..6], 16).ok()?;
    Some((((v >> 16) & 255) as f64 / 255.0, ((v >> 8) & 255) as f64 / 255.0, (v & 255) as f64 / 255.0))
}

pub fn font_family() -> String {
    "monospace".into()
}

/// Install the base stylesheet plus theme overrides, and watch the theme for changes.
pub fn install() {
    let Some(display) = gdk::Display::default() else { return };
    let base = gtk::CssProvider::new();
    base.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(&display, &base, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    let provider = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
    PROVIDER.with(|p| *p.borrow_mut() = Some(provider));
    apply();

    if let Some(path) = colors_path() {
        let file = gio::File::for_path(&path);
        if let Ok(monitor) = file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            monitor.connect_changed(|_, _, _, event| {
                if matches!(event, gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::Changed | gio::FileMonitorEvent::Created) {
                    glib::timeout_add_local_once(std::time::Duration::from_millis(200), apply);
                }
            });
            MONITOR.with(|m| *m.borrow_mut() = Some(monitor));
        }
    }
}

fn apply() {
    let theme = load();
    CURRENT.with(|c| *c.borrow_mut() = theme.clone());
    let manager = adw::StyleManager::default();
    let css = match &theme {
        Some(t) => {
            manager.set_color_scheme(if t.is_dark() { adw::ColorScheme::ForceDark } else { adw::ColorScheme::ForceLight });
            theme_css(t)
        }
        None => {
            manager.set_color_scheme(adw::ColorScheme::PreferDark);
            String::new()
        }
    };
    PROVIDER.with(|p| {
        if let Some(p) = p.borrow().as_ref() {
            p.load_from_string(&css);
        }
    });
}

fn theme_css(t: &ThemeColors) -> String {
    let c = |v: &str, fallback: &str| if v.is_empty() { fallback.to_string() } else { v.to_string() };
    let dark = t.is_dark();
    let bg = c(&t.background, if dark { "#1e1e1e" } else { "#f5f5f5" });
    let bg_dark = c(&t.dark_background, &bg);
    let bg_darker = c(&t.darker_background, &bg_dark);
    let bg_light = c(&t.lighter_background, &bg);
    let fg = c(&t.foreground, if dark { "#e0e0e0" } else { "#202020" });
    let fg_dim = c(&t.dark_foreground, &fg);
    let accent = c(&t.accent, "#7daea3");
    let accent_fg = if dark { &bg_darker } else { &bg };
    let muted = c(&t.muted, &bg_light);
    let red = c(&t.red, "#ea6962");
    let green = c(&t.green, "#a9b665");
    let yellow = c(&t.yellow, "#d8a657");
    format!(
        r#"
@define-color window_bg_color {bg};
@define-color window_fg_color {fg};
@define-color view_bg_color {bg_dark};
@define-color view_fg_color {fg};
@define-color headerbar_bg_color {bg};
@define-color headerbar_fg_color {fg};
@define-color headerbar_border_color {muted};
@define-color headerbar_backdrop_color {bg};
@define-color headerbar_shade_color {muted};
@define-color headerbar_darker_shade_color {bg_darker};
@define-color sidebar_bg_color {bg};
@define-color sidebar_fg_color {fg};
@define-color sidebar_backdrop_color {bg};
@define-color sidebar_border_color {muted};
@define-color sidebar_shade_color {bg_dark};
@define-color card_bg_color {bg_light};
@define-color card_fg_color {fg};
@define-color card_shade_color {muted};
@define-color dialog_bg_color {bg};
@define-color dialog_fg_color {fg};
@define-color popover_bg_color {bg};
@define-color popover_fg_color {fg};
@define-color popover_shade_color {muted};
@define-color thumbnail_bg_color {bg_light};
@define-color thumbnail_fg_color {fg};
@define-color shade_color {muted};
@define-color scrollbar_outline_color {muted};
@define-color borders {muted};
@define-color accent_bg_color {accent};
@define-color accent_fg_color {accent_fg};
@define-color accent_color {accent};
@define-color destructive_bg_color {red};
@define-color destructive_fg_color {accent_fg};
@define-color destructive_color {red};
@define-color success_bg_color {green};
@define-color success_fg_color {accent_fg};
@define-color success_color {green};
@define-color warning_bg_color {yellow};
@define-color warning_fg_color {accent_fg};
@define-color warning_color {yellow};
@define-color error_bg_color {red};
@define-color error_fg_color {accent_fg};
@define-color error_color {red};
.dim-label {{ color: {fg_dim}; }}
.annotate-canvas {{ background: {bg_darker}; }}
"#
    )
}
