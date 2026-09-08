mod app;
mod capture;
mod clipboard;
mod config;
mod export;
mod history;
mod notify;
mod ocr;
mod paths;
mod postcapture;
mod quickaccess;
mod annotate;

use clap::{Parser, Subcommand};

/// Screenshot capture and annotation for Wayland / Hyprland.
#[derive(Parser, Debug, Clone)]
#[command(name = "grabbit", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Select a region of the screen and capture it.
    Area {
        /// Open the annotation editor inline before saving.
        #[arg(long)]
        annotate: bool,
    },
    /// Capture every monitor.
    Full,
    /// Pick a window under the cursor and capture it.
    Window,
    /// Capture a region and extract its text with OCR to the clipboard.
    Ocr,
    /// Open an image file in the annotation editor.
    Annotate { file: std::path::PathBuf },
    /// Open the capture history browser.
    History,
    /// Open preferences.
    Settings,
    /// Run the resident daemon so hotkeys respond instantly.
    Daemon,
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("grabbit=info")),
        )
        .with_writer(std::io::stderr)
        .init();
    app::run()
}
