use std::path::PathBuf;

pub const APP_ID: &str = "io.github.diyaclanker.omashot";

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config")).join("omashot")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share")).join("omashot")
}

pub fn history_db() -> PathBuf {
    data_dir().join("history.sqlite")
}

/// Directory holding editable annotation sessions (`*.omashot.json`).
pub fn sessions_dir() -> PathBuf {
    data_dir().join("sessions")
}

/// Scratch captures (Quick Access without save, drag-to-app, MCP `save:false`).
/// Lives under the user's data dir, never a shared /tmp, and is created 0700.
pub fn temp_dir() -> PathBuf {
    data_dir().join("captures")
}

pub fn ensure_dirs() {
    use std::os::unix::fs::DirBuilderExt;
    for d in [config_dir(), data_dir(), sessions_dir(), temp_dir()] {
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(d);
    }
}

/// Delete scratch captures older than `max_age_days` (0 keeps everything).
pub fn sweep_temp(max_age_days: u32) {
    if max_age_days == 0 {
        return;
    }
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(max_age_days as u64 * 86_400);
    if let Ok(entries) = std::fs::read_dir(temp_dir()) {
        for e in entries.flatten() {
            if let Ok(meta) = e.metadata() {
                if meta.is_file() && meta.modified().map(|m| m < cutoff).unwrap_or(false) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}
