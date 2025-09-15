use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use toml::Value;
use toml_edit::{DocumentMut, value};

/// Represents a tap source.
#[derive(Debug, Clone)]
pub struct Tap {
    pub name: String,
    pub url: String,
    pub priority: u32,
    pub enabled: bool,
}

/// Represents a publisher of packages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Publisher {
    pub name: String,
    pub gpg_key: String,
    pub email: String,
    pub url: String,
    pub verified: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct SyncState {
    #[serde(with = "chrono::serde::ts_seconds_option")]
    last_sync: Option<DateTime<Utc>>,
}

fn tap_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(cfg) = dirs::config_dir() {
        dirs.push(cfg.join("reap/taps"));
    }
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(xdg_data).join("reap/taps"));
    }
    dirs
}

/// Discovers available taps by scanning configured directories.
pub fn discover_taps() -> Vec<Tap> {
    let mut taps = Vec::new();
    for dir in tap_dirs() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    if let Ok(toml) = fs::read_to_string(&path) {
                        if let Ok(val) = toml.parse::<Value>() {
                            let name = val
                                .as_table()
                                .and_then(|t| t.get("name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let url = val
                                .as_table()
                                .and_then(|t| t.get("url"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let priority = val
                                .as_table()
                                .and_then(|t| t.get("priority"))
                                .and_then(|v| v.as_integer())
                                .unwrap_or(50) as u32;
                            let enabled = val
                                .as_table()
                                .and_then(|t| t.get("enabled"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            if !name.is_empty() && !url.is_empty() && enabled {
                                taps.push(Tap {
                                    name,
                                    url,
                                    priority,
                                    enabled,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    taps.sort_by(|a, b| b.priority.cmp(&a.priority));
    taps
}

/// Finds a tap for a given package, optionally forcing a specific tap.
pub fn find_tap_for_pkg(pkg: &str, taps: &[Tap], forced: Option<&str>) -> Option<Tap> {
    if let Some(force) = forced {
        taps.iter().find(|t| t.name == force).cloned()
    } else {
        taps.iter().find(|t| tap_has_package(t, pkg)).cloned()
    }
}

/// Ensures that a tap is cloned to the local machine, pulling updates if it already exists.
pub fn ensure_tap_cloned(tap: &Tap) -> PathBuf {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reap/taps");
    let tap_path = cache_dir.join(&tap.name);
    if !tap_path.exists() {
        let _ = std::process::Command::new("git")
            .arg("clone")
            .arg(&tap.url)
            .arg(&tap_path)
            .status();
    }
    tap_path
}

/// Gets the file path for a tap's configuration.
pub fn tap_path(name: &str) -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reap/taps");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{}.toml", name))
}

/// Adds or updates a tap's configuration.
pub fn add_or_update_tap(name: &str, url: &str, priority: Option<u8>, enabled: bool) {
    let path = tap_path(name);
    let mut doc = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.parse::<DocumentMut>().ok())
            .unwrap_or_default()
    } else {
        DocumentMut::new()
    };
    doc["name"] = value(name);
    doc["url"] = value(url);
    doc["priority"] = value(priority.unwrap_or(50) as i64);
    doc["enabled"] = value(enabled);
    let _ = fs::write(&path, doc.to_string());
}

/// Removes a tap's configuration and deletes the local copy.
pub fn remove_tap(name: &str) {
    let path = tap_path(name);
    let _ = fs::remove_file(&path);
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reap/taps")
        .join(name);
    let _ = fs::remove_dir_all(&cache_dir);
}

/// Enables or disables a tap.
pub fn set_tap_enabled(name: &str, enabled: bool) {
    let path = tap_path(name);
    if path.exists() {
        if let Ok(mut doc) = fs::read_to_string(&path)
            .and_then(|s| s.parse::<DocumentMut>().map_err(std::io::Error::other))
        {
            doc["enabled"] = value(enabled);
            let _ = fs::write(&path, doc.to_string());
        }
    }
}

/// Synchronizes all taps by ensuring they are cloned and up-to-date.
pub fn sync_taps() {
    for tap in discover_taps() {
        let _ = ensure_tap_cloned(&tap);
    }
}

/// Synchronizes enabled taps based on the configured sync interval.
pub fn sync_enabled_taps() -> Result<(), String> {
    let taps = discover_taps();
    let state_path = sync_state_path();
    let mut state: SyncState = if state_path.exists() {
        fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        SyncState::default()
    };
    let config_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reap/reap.toml");
    let (auto_sync, sync_interval_hours) = if let Ok(toml) = fs::read_to_string(&config_path) {
        if let Ok(val) = toml.parse::<toml::Value>() {
            let auto_sync = val
                .as_table()
                .and_then(|t| t.get("settings"))
                .and_then(|s| s.as_table())
                .and_then(|s| s.get("auto_sync"))
                .and_then(|b| b.as_bool())
                .unwrap_or(true);
            let interval = val
                .as_table()
                .and_then(|t| t.get("settings"))
                .and_then(|s| s.as_table())
                .and_then(|s| s.get("sync_interval_hours"))
                .and_then(|i| i.as_integer())
                .unwrap_or(12);
            (auto_sync, interval)
        } else {
            (true, 12)
        }
    } else {
        (true, 12)
    };
    let now = Utc::now();
    let should_sync = if auto_sync {
        match state.last_sync {
            Some(last) => now - last > Duration::hours(sync_interval_hours),
            None => true,
        }
    } else {
        false
    };
    if should_sync {
        for tap in taps.iter().filter(|t| t.enabled) {
            let tap_path = ensure_tap_cloned(tap);
            if tap_path.exists() {
                // git pull
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&tap_path)
                    .arg("pull")
                    .status();
            } else {
                // already cloned by ensure_tap_cloned
            }
        }
        state.last_sync = Some(now);
        let _ = fs::create_dir_all(state_path.parent().unwrap());
        let _ = fs::write(&state_path, serde_json::to_string(&state).unwrap());
    }
    Ok(())
}

fn sync_state_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reap/.sync-state.json")
}

/// Lists all discovered taps with their details.
pub fn list_taps() {
    for tap in discover_taps() {
        println!(
            "{} | {} | enabled={} | priority={}",
            tap.name, tap.url, tap.enabled, tap.priority
        );
    }
}

/// Checks if a tap has a specific package.
pub fn tap_has_package(tap: &Tap, pkg: &str) -> bool {
    let tap_path = ensure_tap_cloned(tap);
    let pkgb = tap_path.join(pkg).join("PKGBUILD");
    pkgb.exists()
}

/// Loads and merges all tap index.json files, sorted by priority DESC, name ASC.
pub fn search_tap_indexes(query: &str) -> Vec<(String, String, String, String)> {
    let mut results = Vec::new();
    let taps = discover_taps();
    let mut taps_sorted = taps.clone();
    taps_sorted.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
    let mut seen = std::collections::HashSet::new();
    for tap in taps_sorted.iter().filter(|t| t.enabled) {
        let tap_path = ensure_tap_cloned(tap);
        let index_path = tap_path.join("index.json");
        if let Ok(data) = fs::read_to_string(&index_path) {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
                if let Some(obj) = json.as_object() {
                    for (pkg, meta) in obj {
                        if seen.contains(pkg) {
                            continue;
                        }
                        let desc = meta.get("desc").and_then(|v| v.as_str()).unwrap_or("");
                        let repo = meta
                            .get("repo")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&tap.name);
                        results.push((
                            pkg.clone(),
                            desc.to_string(),
                            repo.to_string(),
                            format!("tap:{}", tap.name),
                        ));
                        seen.insert(pkg.clone());
                    }
                }
            }
        }
    }
    // Filter by query
    results
        .into_iter()
        .filter(|(pkg, desc, _, _)| pkg.contains(query) || desc.contains(query))
        .collect()
}

/// Gets publisher information from a tap's publisher.toml file.
pub fn get_publisher_info(tap: &Tap) -> Option<Publisher> {
    let tap_path = ensure_tap_cloned(tap);
    let pub_path = tap_path.join("publisher.toml");
    if pub_path.exists() {
        if let Ok(toml) = fs::read_to_string(&pub_path) {
            if let Ok(val) = toml.parse::<toml::Value>() {
                let name = val
                    .as_table()
                    .and_then(|t| t.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let gpg_key = val
                    .as_table()
                    .and_then(|t| t.get("gpg_key"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let email = val
                    .as_table()
                    .and_then(|t| t.get("email"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let url = val
                    .as_table()
                    .and_then(|t| t.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let verified = val
                    .as_table()
                    .and_then(|t| t.get("verified"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                return Some(Publisher {
                    name,
                    gpg_key,
                    email,
                    url,
                    verified,
                });
            }
        }
    }
    None
}

// No async/parallel flows in tap.rs; nothing to change for prompt 2
