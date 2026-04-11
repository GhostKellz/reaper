//! Centralized path definitions for Reaper
//!
//! All runtime paths use the "reap" namespace consistently.
//! Follows XDG Base Directory specification with /tmp fallback.

#![allow(dead_code)]

use once_cell::sync::Lazy;
use std::path::PathBuf;

/// The namespace used for all reap directories
pub const NAMESPACE: &str = "reap";

// =============================================================================
// Config paths (~/.config/reap/)
// =============================================================================

/// User config directory: ~/.config/reap/
pub static CONFIG_DIR: Lazy<PathBuf> = Lazy::new(|| {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(NAMESPACE)
});

/// Global config file: /etc/reap/reap.toml
pub static GLOBAL_CONFIG: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from("/etc").join(NAMESPACE).join("reap.toml"));

/// User config file: ~/.config/reap/reap.toml
pub static USER_CONFIG: Lazy<PathBuf> = Lazy::new(|| CONFIG_DIR.join("reap.toml"));

// =============================================================================
// Cache paths (~/.cache/reap/)
// =============================================================================

/// Cache directory: ~/.cache/reap/
pub static CACHE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(NAMESPACE)
});

// =============================================================================
// Data paths (~/.local/share/reap/)
// =============================================================================

/// Data directory: ~/.local/share/reap/
pub static DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(NAMESPACE)
});

// =============================================================================
// Subdirectory functions
// =============================================================================

/// Profiles directory: ~/.config/reap/profiles/
pub fn profiles_dir() -> PathBuf {
    CONFIG_DIR.join("profiles")
}

/// Taps config directory: ~/.config/reap/taps/
pub fn taps_config_dir() -> PathBuf {
    CONFIG_DIR.join("taps")
}

/// Taps cache directory: ~/.cache/reap/taps/
pub fn taps_cache_dir() -> PathBuf {
    CACHE_DIR.join("taps")
}

/// Hooks directory: ~/.config/reap/hooks/
pub fn hooks_dir() -> PathBuf {
    CONFIG_DIR.join("hooks")
}

/// Backup directory: ~/.local/share/reap/backup/
pub fn backup_dir() -> PathBuf {
    DATA_DIR.join("backup")
}

/// History directory: ~/.local/share/reap/history/
pub fn history_dir() -> PathBuf {
    DATA_DIR.join("history")
}

/// Transactions directory: ~/.local/share/reap/history/transactions/
pub fn transactions_dir() -> PathBuf {
    history_dir().join("transactions")
}

/// Trust cache directory: ~/.cache/reap/trust/
pub fn trust_dir() -> PathBuf {
    CACHE_DIR.join("trust")
}

/// Development package cache: ~/.cache/reap/devel/
pub fn devel_dir() -> PathBuf {
    CACHE_DIR.join("devel")
}

/// AUR cache directory: ~/.cache/reap/aur/
pub fn aur_cache_dir() -> PathBuf {
    CACHE_DIR.join("aur")
}

/// PKGBUILD cache directory: ~/.cache/reap/pkgbuilds/
pub fn pkgbuild_cache_dir() -> PathBuf {
    CACHE_DIR.join("pkgbuilds")
}

/// Search cache directory: ~/.cache/reap/search/
pub fn search_cache_dir() -> PathBuf {
    CACHE_DIR.join("search")
}

/// Ratings cache directory: ~/.cache/reap/ratings/
pub fn ratings_dir() -> PathBuf {
    CACHE_DIR.join("ratings")
}

/// Metrics directory: ~/.local/share/reap/metrics/
pub fn metrics_dir() -> PathBuf {
    DATA_DIR.join("metrics")
}

/// Pinned packages file: ~/.config/reap/pinned.toml
pub fn pinned_file() -> PathBuf {
    CONFIG_DIR.join("pinned.toml")
}

/// Reap Lua hooks script: ~/.config/reap/reap.lua
pub fn reap_lua() -> PathBuf {
    CONFIG_DIR.join("reap.lua")
}

/// Chroot directory: ~/.cache/reap/chroot/
pub fn chroot_dir() -> PathBuf {
    CACHE_DIR.join("chroot")
}

/// Keyring directory: ~/.local/share/reap/keyring/
pub fn keyring_dir() -> PathBuf {
    DATA_DIR.join("keyring")
}

/// Trust database: ~/.local/share/reap/trust.db
pub fn trust_db() -> PathBuf {
    DATA_DIR.join("trust.db")
}

/// Sync state file: ~/.local/share/reap/.sync-state.json
pub fn sync_state_file() -> PathBuf {
    DATA_DIR.join(".sync-state.json")
}

// =============================================================================
// Legacy paths (for migration)
// =============================================================================

/// Legacy backup directory: /var/lib/reaper/backups/
pub fn legacy_backup_dir() -> PathBuf {
    PathBuf::from("/var/lib/reaper/backups")
}

/// Legacy config directory: ~/.config/reaper/
pub fn legacy_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/reaper")
}

// =============================================================================
// Migration
// =============================================================================

/// Migrate legacy paths to new locations.
/// Returns a list of migrations performed.
pub fn migrate_legacy_paths() -> Vec<String> {
    let mut migrated = Vec::new();

    // Migrate ~/.config/reaper -> ~/.config/reap
    let legacy_config = legacy_config_dir();
    if legacy_config.exists() && !CONFIG_DIR.exists() {
        // Ensure parent exists
        if let Some(parent) = CONFIG_DIR.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::rename(&legacy_config, CONFIG_DIR.as_path()) {
            eprintln!("[migrate] Failed to migrate config: {}", e);
        } else {
            migrated.push(format!(
                "{} -> {}",
                legacy_config.display(),
                CONFIG_DIR.display()
            ));
        }
    }

    migrated
}
