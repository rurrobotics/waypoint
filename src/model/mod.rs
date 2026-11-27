use std::path::PathBuf;

pub mod history;
pub mod map;
pub mod project;
pub mod template;

pub use history::History;
pub use map::{Obstacle, Origin, Region, RegionColor};
pub use project::{Doc, Project};

/// Per-user config directory for Waypoint, from the usual environment variables.
pub fn config_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    Some(base?.join("waypoint"))
}
