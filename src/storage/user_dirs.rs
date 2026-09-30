//! Per-user application data location shared by the desktop app and the MCP
//! server: recent projects, recovery snapshots and user catalog packs.
use std::path::PathBuf;

/// The platform application-data directory, or `None` when the environment
/// does not name one.
pub fn user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/Plan My Cabinet"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .filter(|s| !s.is_empty())
            .map(|base| PathBuf::from(base).join("PlanMyCabinet"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .map(|base| base.join("plan-my-cabinet"))
    }
}
