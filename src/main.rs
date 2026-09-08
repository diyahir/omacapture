#![recursion_limit = "512"]
#![allow(clippy::type_complexity, clippy::too_many_arguments, clippy::wrong_self_convention)]
mod annotate;
mod app;
mod capture;
mod clipboard;
mod config;
mod export;
mod history;
mod keybinds;
mod mcp;
mod notify;
mod ocr;
mod paths;
mod postcapture;
mod quickaccess;
mod theme;

use clap::{Parser, Subcommand};

/// Screenshot capture and annotation for Wayland / Hyprland.
#[derive(Parser, Debug, Clone)]
#[command(name = "omashot", version, about)]
pub struct Cli {
    /// Run as an independent instance, block until the capture finishes, and print a JSON result line.
    #[arg(long, global = true)]
    pub wait: bool,
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
    /// Serve the Model Context Protocol over stdio for AI agents.
    Mcp {
        /// Expose only capture, OCR, read, and list tools; never write user-named files or change settings.
        #[arg(long)]
        read_only: bool,
    },
    /// Act on the newest Quick Access card (used by the hover-free shortcuts).
    Qa {
        #[arg(value_enum)]
        action: quickaccess::QaAction,
    },
    /// Install, remove, or inspect the optional Hyprland keybindings.
    Keybinds {
        #[command(subcommand)]
        command: keybinds::KeybindsCommand,
    },
}

/// `omashot FILE` opens the editor, so the binary can serve as
/// `OMARCHY_SCREENSHOT_EDITOR` and as a drag-and-drop target.
pub fn normalize_args(args: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let mut v: Vec<std::ffi::OsString> = args.into_iter().collect();
    // Never reinterpret a valid invocation: `omashot area` stays a capture even
    // if a file named `area` happens to exist in the working directory.
    if Cli::try_parse_from(&v).is_ok() {
        return v;
    }
    let first_non_flag = v.iter().skip(1).position(|a| !a.to_string_lossy().starts_with('-')).map(|i| i + 1);
    if let Some(i) = first_non_flag {
        if std::path::Path::new(&v[i]).is_file() {
            v.insert(i, "annotate".into());
        }
    }
    v
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("omashot=info")),
        )
        .with_writer(std::io::stderr)
        .init();
    // Help and version must work without a display.
    if let Err(e) = Cli::try_parse_from(normalize_args(std::env::args_os())) {
        if matches!(e.kind(), clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion) {
            let _ = e.print();
            return glib::ExitCode::SUCCESS;
        }
    }
    // Headless subcommands never touch GTK.
    if let Ok(cli) = Cli::try_parse_from(normalize_args(std::env::args_os())) {
        if let Some(Command::Mcp { read_only }) = cli.command {
            return match mcp::run(read_only) {
                Ok(()) => glib::ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("mcp: {e}");
                    glib::ExitCode::FAILURE
                }
            };
        }
        if let Some(Command::Keybinds { command }) = cli.command {
            return match keybinds::run(command) {
                Ok(()) => glib::ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("keybinds: {e}");
                    glib::ExitCode::FAILURE
                }
            };
        }
    }
    app::run()
}
