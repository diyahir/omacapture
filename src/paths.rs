use std::path::PathBuf;

pub const APP_ID: &str = "dev.grabbit.Grabbit";

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config")).join("grabbit")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share")).join("grabbit")
}

pub fn history_db() -> PathBuf {
    data_dir().join("history.sqlite")
}

/// Directory holding editable annotation sessions (`*.grabbit.json`).
pub fn sessions_dir() -> PathBuf {
    data_dir().join("sessions")
}

pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("grabbit")
}

pub fn ensure_dirs() {
    for d in [config_dir(), data_dir(), sessions_dir(), temp_dir()] {
        let _ = std::fs::create_dir_all(d);
    }
}
