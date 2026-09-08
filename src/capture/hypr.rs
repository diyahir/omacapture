//! Hyprland IPC helpers (`hyprctl -j`).

use super::Rect;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    #[serde(default)]
    pub transform: i32,
    #[serde(default)]
    pub focused: bool,
    #[serde(rename = "activeWorkspace")]
    pub active_workspace: WorkspaceRef,
}

impl Monitor {
    /// Logical rectangle of the monitor in compositor space.
    pub fn logical_rect(&self) -> Rect {
        let (w, h) = if self.transform % 2 == 1 { (self.height, self.width) } else { (self.width, self.height) };
        Rect::new(self.x, self.y, (w as f64 / self.scale).round() as i32, (h as f64 / self.scale).round() as i32)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceRef {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Client {
    pub address: String,
    pub at: [i32; 2],
    pub size: [i32; 2],
    pub workspace: WorkspaceRef,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub mapped: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub floating: bool,
    #[serde(default)]
    pub monitor: i64,
    #[serde(default, rename = "focusHistoryID")]
    pub focus_history_id: i64,
}

impl Client {
    pub fn rect(&self) -> Rect {
        Rect::new(self.at[0], self.at[1], self.size[0].max(1), self.size[1].max(1))
    }
}

fn hyprctl<T: for<'de> Deserialize<'de>>(what: &str) -> Result<T> {
    let out = Command::new("hyprctl").args(["-j", what]).output().context("running hyprctl")?;
    serde_json::from_slice(&out.stdout).with_context(|| format!("parsing `hyprctl -j {what}`"))
}

pub fn monitors() -> Result<Vec<Monitor>> {
    hyprctl("monitors")
}

/// Visible windows on the active workspaces, sorted front-most first.
pub fn visible_windows() -> Result<Vec<Client>> {
    let mons = monitors()?;
    let active: std::collections::HashSet<i64> = mons.iter().map(|m| m.active_workspace.id).collect();
    let mut clients: Vec<Client> = hyprctl::<Vec<Client>>("clients")?
        .into_iter()
        .filter(|c| c.mapped && !c.hidden && active.contains(&c.workspace.id) && c.size[0] > 0 && c.size[1] > 0)
        .collect();
    // focusHistoryID 0 is the most recently focused window; floating windows sit above tiled ones.
    clients.sort_by_key(|c| (!c.floating, c.focus_history_id));
    Ok(clients)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CursorPos {
    pub x: i32,
    pub y: i32,
}

pub fn cursor_pos() -> Result<CursorPos> {
    hyprctl("cursorpos")
}

pub fn is_hyprland() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}
