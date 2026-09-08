//! Opt-in Hyprland keybinding installer. Writes a clearly marked block into
//! `~/.config/hypr/bindings.lua` only when the user asks (CLI or Preferences),
//! never on install, and removes exactly that block again.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

const BEGIN: &str = "-- omashot:begin (managed by `omashot keybinds`; edit or remove freely)";
const END: &str = "-- omashot:end";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Preset {
    /// Super+I captures an area, Super+Shift+I captures and annotates.
    SuperI,
    /// Take over Print (unbinds Omarchy's screenshot), Shift+Print for window, Ctrl+Print to annotate.
    Print,
}

pub fn bindings_file() -> PathBuf {
    dirs::config_dir().unwrap_or_default().join("hypr/bindings.lua")
}

fn shell_cmd(verb: &str) -> String {
    format!("omarchy-shell omashot {verb}")
}

pub fn block(preset: Preset) -> String {
    let mut lines = vec![BEGIN.to_string()];
    match preset {
        Preset::SuperI => {
            lines.push(format!("o.bind(\"SUPER + I\", \"Screenshot (Omashot)\", \"{}\")", shell_cmd("area")));
            lines.push(format!("o.bind(\"SUPER + SHIFT + I\", \"Screenshot and annotate (Omashot)\", \"{}\")", shell_cmd("annotate")));
        }
        Preset::Print => {
            lines.push("hl.unbind(\"PRINT\")".into());
            lines.push(format!("o.bind(\"PRINT\", \"Screenshot (Omashot)\", \"{}\")", shell_cmd("area")));
            lines.push(format!("o.bind(\"SHIFT + PRINT\", \"Screenshot window (Omashot)\", \"{}\")", shell_cmd("window")));
            lines.push(format!("o.bind(\"CTRL + PRINT\", \"Screenshot and annotate (Omashot)\", \"{}\")", shell_cmd("annotate")));
            lines.push("hl.unbind(\"SUPER + CTRL + PRINT\")".into());
            lines.push(format!("o.bind(\"SUPER + CTRL + PRINT\", \"Extract text (Omashot)\", \"{}\")", shell_cmd("ocr")));
        }
    }
    lines.push(END.to_string());
    lines.join("\n")
}

/// Keys the preset would claim that are already bound to something else.
pub fn conflicts(preset: Preset) -> Vec<String> {
    let keys: &[(&str, u32)] = match preset {
        // modmask: SUPER=64, SHIFT=1, CTRL=4
        // Super+Ctrl+I is Omarchy's idle-lock toggle, so the preset stays off it;
        // window capture is `A` inside the overlay.
        Preset::SuperI => &[("I", 64), ("I", 65)],
        Preset::Print => &[("PRINT", 0), ("PRINT", 1), ("PRINT", 4), ("PRINT", 68)],
    };
    let Ok(out) = std::process::Command::new("hyprctl").args(["binds", "-j"]).output() else {
        return Vec::new();
    };
    let Ok(binds) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for b in binds {
        let key = b.get("key").and_then(|k| k.as_str()).unwrap_or("").to_ascii_uppercase();
        let mask = b.get("modmask").and_then(|m| m.as_u64()).unwrap_or(0) as u32;
        let desc = b.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let arg = b.get("arg").and_then(|d| d.as_str()).unwrap_or("");
        if keys.iter().any(|(k, m)| *k == key && *m == mask) && !desc.contains("Omashot") && !arg.contains("omashot") {
            let mods = [(64, "SUPER"), (4, "CTRL"), (1, "SHIFT"), (8, "ALT")]
                .iter()
                .filter(|(bit, _)| mask & bit != 0)
                .map(|(_, n)| *n)
                .collect::<Vec<_>>()
                .join(" + ");
            let combo = if mods.is_empty() { key.clone() } else { format!("{mods} + {key}") };
            found.push(format!("{combo} ({})", if desc.is_empty() { arg } else { desc }));
        }
    }
    found
}

fn strip_block(text: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in text.lines() {
        if line.trim() == BEGIN {
            skipping = true;
            continue;
        }
        if skipping {
            if line.trim() == END {
                skipping = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

pub fn is_installed(file: &Path) -> bool {
    std::fs::read_to_string(file).map(|t| t.contains(BEGIN)).unwrap_or(false)
}

/// Write (or replace) the managed block. Returns the backup path.
pub fn install(preset: Preset, file: &Path, reload: bool) -> Result<Option<PathBuf>> {
    let existing = std::fs::read_to_string(file).unwrap_or_default();
    let backup = if file.exists() {
        let b = file.with_extension(format!("lua.bak.{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
        std::fs::copy(file, &b).with_context(|| format!("backing up {}", file.display()))?;
        Some(b)
    } else {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        None
    };
    let base = strip_block(&existing);
    let text = if base.is_empty() { format!("{}\n", block(preset)) } else { format!("{base}\n\n{}\n", block(preset)) };
    std::fs::write(file, text).with_context(|| format!("writing {}", file.display()))?;
    if reload {
        reload_hyprland()?;
    }
    Ok(backup)
}

pub fn remove(file: &Path, reload: bool) -> Result<bool> {
    let existing = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    if !existing.contains(BEGIN) {
        return Ok(false);
    }
    std::fs::write(file, format!("{}\n", strip_block(&existing)))?;
    if reload {
        reload_hyprland()?;
    }
    Ok(true)
}

fn reload_hyprland() -> Result<()> {
    let status = std::process::Command::new("hyprctl")
        .arg("reload")
        .stdout(std::process::Stdio::null())
        .status()
        .context("running hyprctl reload")?;
    if !status.success() {
        bail!("hyprctl reload failed");
    }
    let errors = std::process::Command::new("hyprctl").arg("configerrors").output()?;
    let text = String::from_utf8_lossy(&errors.stdout);
    if !text.trim().is_empty() {
        bail!("Hyprland reported config errors after reload:\n{}", text.trim());
    }
    Ok(())
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum KeybindsCommand {
    /// Print the binding block for a preset without touching any file.
    Show {
        #[arg(value_enum, default_value_t = Preset::SuperI)]
        preset: Preset,
    },
    /// Append (or replace) the managed block in ~/.config/hypr/bindings.lua and reload Hyprland.
    Install {
        #[arg(value_enum, default_value_t = Preset::SuperI)]
        preset: Preset,
        /// Install even if one of the keys is already bound to something else.
        #[arg(long)]
        force: bool,
        /// Target file (defaults to ~/.config/hypr/bindings.lua).
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
        /// Do not run `hyprctl reload` afterwards.
        #[arg(long)]
        no_reload: bool,
    },
    /// Remove the managed block again.
    Remove {
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
        #[arg(long)]
        no_reload: bool,
    },
    /// Report whether the block is present and which preset keys are already taken.
    Status,
}

pub fn run(cmd: KeybindsCommand) -> Result<()> {
    match cmd {
        KeybindsCommand::Show { preset } => {
            println!("{}", block(preset));
        }
        KeybindsCommand::Install { preset, force, file, no_reload } => {
            let file = file.unwrap_or_else(bindings_file);
            let taken = conflicts(preset);
            if !taken.is_empty() && !force {
                bail!("already bound: {}\nRe-run with --force to override, or pick another preset.", taken.join(", "));
            }
            let backup = install(preset, &file, !no_reload)?;
            println!("Installed {:?} bindings into {}", preset, file.display());
            if let Some(b) = backup {
                println!("Backup: {}", b.display());
            }
        }
        KeybindsCommand::Remove { file, no_reload } => {
            let file = file.unwrap_or_else(bindings_file);
            if remove(&file, !no_reload)? {
                println!("Removed Omashot bindings from {}", file.display());
            } else {
                println!("No Omashot bindings found in {}", file.display());
            }
        }
        KeybindsCommand::Status => {
            let file = bindings_file();
            println!("{}: {}", file.display(), if is_installed(&file) { "Omashot block present" } else { "no Omashot block" });
            for preset in [Preset::SuperI, Preset::Print] {
                let taken = conflicts(preset);
                println!(
                    "{preset:?}: {}",
                    if taken.is_empty() { "keys free".to_string() } else { format!("taken by {}", taken.join(", ")) }
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent_and_removable() {
        let dir = std::env::temp_dir().join(format!("omashot-kb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bindings.lua");
        std::fs::write(&file, "-- user stuff\no.bind(\"SUPER + B\", \"Browser\", \"chromium\")\n").unwrap();
        install(Preset::SuperI, &file, false).unwrap();
        install(Preset::Print, &file, false).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.matches(BEGIN).count(), 1, "one managed block");
        assert!(text.contains("hl.unbind(\"PRINT\")"));
        assert!(!text.contains("SUPER + I"), "previous preset replaced");
        assert!(text.starts_with("-- user stuff"));
        assert!(remove(&file, false).unwrap());
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains(BEGIN));
        assert!(text.contains("chromium"));
        assert!(!remove(&file, false).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
