//! User configuration, persisted at `~/.config/omashot/config.toml`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpg,
    Webp,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpg => "jpg",
            ImageFormat::Webp => "webp",
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
        }
    }
}

/// What happens after a capture finishes, per capture mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct AfterCapture {
    pub save: bool,
    pub copy: bool,
    pub quick_access: bool,
    pub annotate: bool,
}

impl Default for AfterCapture {
    fn default() -> Self {
        Self { save: true, copy: true, quick_access: true, annotate: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// Folder screenshots are written to.
    pub save_folder: PathBuf,
    /// strftime-style pattern used for filenames (without extension).
    pub filename_pattern: String,
    pub format: ImageFormat,
    /// JPG / WebP quality, 1-100.
    pub quality: u8,
    pub include_cursor: bool,
    /// Play the shutter sound.
    pub sound: bool,
    pub notifications: bool,
    /// Remember the last selected region and offer it on the next area capture.
    pub remember_last_area: bool,
    /// Milliseconds to wait between the hotkey and the capture (0 = none).
    pub delay_ms: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            // Follow Omarchy's own screenshot tooling when it is configured.
            save_folder: std::env::var_os("OMARCHY_SCREENSHOT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::picture_dir()
                        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"))
                        .join("Screenshots")
                }),
            filename_pattern: "Screenshot %Y-%m-%d at %H.%M.%S".into(),
            format: ImageFormat::Png,
            quality: 90,
            include_cursor: false,
            sound: true,
            notifications: true,
            remember_last_area: true,
            delay_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PostCapture {
    pub fullscreen: AfterCapture,
    pub area: AfterCapture,
    pub window: AfterCapture,
    pub annotate_export: AfterCapture,
}

impl Default for PostCapture {
    fn default() -> Self {
        Self {
            fullscreen: AfterCapture::default(),
            area: AfterCapture::default(),
            window: AfterCapture::default(),
            annotate_export: AfterCapture { save: true, copy: true, quick_access: true, annotate: false },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickAccess {
    pub enabled: bool,
    pub corner: Corner,
    /// Seconds before a card dismisses itself; 0 keeps it until dismissed.
    pub auto_dismiss_secs: u32,
    pub max_cards: usize,
    pub thumbnail_width: i32,
    /// Keep the editor open after dragging out of Quick Access.
    pub keep_editing_after_drag: bool,
}

impl Default for QuickAccess {
    fn default() -> Self {
        Self {
            enabled: true,
            corner: Corner::BottomRight,
            auto_dismiss_secs: 8,
            max_cards: 4,
            thumbnail_width: 240,
            keep_editing_after_drag: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotate {
    pub stroke_color: String,
    pub fill_color: String,
    pub text_color: String,
    pub stroke_width: f64,
    pub font_family: String,
    pub font_size: f64,
    pub blur_style: String,
    pub blur_strength: f64,
    pub corner_radius: f64,
    /// Auto-crop transparent edges when exporting a remove-background result.
    pub auto_crop: bool,
    /// Automatically redact things that look like secrets when opening the editor.
    pub auto_redact: bool,
    pub watermark_text: String,
    pub watermark_image: Option<PathBuf>,
}

impl Default for Annotate {
    fn default() -> Self {
        Self {
            stroke_color: "#ff3b30".into(),
            fill_color: "#ff3b3080".into(),
            text_color: "#ff3b30".into(),
            stroke_width: 4.0,
            font_family: "Sans".into(),
            font_size: 28.0,
            blur_style: "pixelate".into(),
            blur_strength: 12.0,
            corner_radius: 8.0,
            auto_crop: true,
            auto_redact: false,
            watermark_text: String::new(),
            watermark_image: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub enabled: bool,
    /// Days to keep entries; 0 keeps forever.
    pub retention_days: u32,
    pub max_entries: u32,
}

impl Default for History {
    fn default() -> Self {
        Self { enabled: true, retention_days: 30, max_entries: 500 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Ocr {
    /// Tesseract language codes, e.g. "eng" or "eng+deu".
    pub languages: String,
    pub copy_to_clipboard: bool,
}

impl Default for Ocr {
    fn default() -> Self {
        Self { languages: std::env::var("OMARCHY_OCR_LANGS").unwrap_or_else(|_| "eng".into()), copy_to_clipboard: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub post_capture: PostCapture,
    pub quick_access: QuickAccess,
    pub annotate: Annotate,
    pub history: History,
    pub ocr: Ocr,
}

impl Config {
    pub fn load() -> Self {
        let path = crate::paths::config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!("config parse error in {}: {e}; using defaults", path.display());
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                if let Err(e) = cfg.save() {
                    tracing::warn!("could not write default config: {e}");
                }
                cfg
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        crate::paths::ensure_dirs();
        let text = toml::to_string_pretty(self)?;
        std::fs::write(crate::paths::config_file(), text)?;
        Ok(())
    }
}

/// Shared handle to the live configuration.
#[derive(Clone)]
pub struct ConfigHandle(Arc<RwLock<Config>>);

impl ConfigHandle {
    pub fn new(cfg: Config) -> Self {
        Self(Arc::new(RwLock::new(cfg)))
    }
    pub fn get(&self) -> Config {
        self.0.read().unwrap().clone()
    }
    pub fn update(&self, f: impl FnOnce(&mut Config)) {
        let mut guard = self.0.write().unwrap();
        f(&mut guard);
        if let Err(e) = guard.save() {
            tracing::warn!("saving config failed: {e}");
        }
    }
}
