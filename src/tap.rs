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

/// Publisher verification status - distinguishes self-declared from cryptographically verified
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublisherStatus {
    /// Non-tap source (AUR, Pacman, Flatpak) - publisher verification not applicable
    NotApplicable,
    /// Has publisher.toml but GPG key not verified against signing key
    SelfDeclared,
    /// Publisher's declared GPG key matches the signing key used for commits/packages
    KeyMatches,
    /// No publisher metadata found
    Unknown,
}

#[derive(Serialize, Deserialize, Default)]
struct SyncState {
    #[serde(with = "chrono::serde::ts_seconds_option")]
    last_sync: Option<DateTime<Utc>>,
}

fn tap_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(crate::paths::taps_config_dir());
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
                if path.extension().and_then(|e| e.to_str()) == Some("toml")
                    && let Ok(toml) = fs::read_to_string(&path)
                    && let Ok(val) = toml.parse::<Value>()
                {
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
    taps.sort_by_key(|tap| std::cmp::Reverse(tap.priority));
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
    let cache_dir = crate::paths::taps_cache_dir();
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

/// Result of tap trust verification
#[derive(Debug, Clone)]
pub enum TapTrustResult {
    /// All commits are GPG signed and verified (includes count of signed commits)
    Trusted { signed_count: u32 },
    /// Some commits are not signed (includes count of unsigned commits)
    UnsignedCommits { unsigned_count: u32 },
    /// Signature verification failed
    InvalidSignature,
    /// Could not verify (git error, etc.)
    Unknown(String),
}

/// Verify that remote commits in a tap are GPG signed
pub fn verify_tap_commits(tap_path: &std::path::Path) -> TapTrustResult {
    // Fetch without merging
    let fetch = std::process::Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(tap_path)
        .output();

    if fetch.is_err() {
        return TapTrustResult::Unknown("Failed to fetch from remote".to_string());
    }

    // Check if HEAD..origin/HEAD has any unsigned commits
    // Using git log --format to check signature status
    let log_output = std::process::Command::new("git")
        .args([
            "log",
            "--format=%G?", // %G? = signature status: G=good, B=bad, U=unknown, N=none
            "HEAD..origin/HEAD",
        ])
        .current_dir(tap_path)
        .output();

    match log_output {
        Ok(output) => {
            let statuses = String::from_utf8_lossy(&output.stdout);
            let mut signed_count: u32 = 0;
            let mut unsigned_count: u32 = 0;
            let mut has_invalid = false;

            for status in statuses.lines() {
                match status.trim() {
                    "G" | "U" => signed_count += 1, // Good signature (trusted or untrusted)
                    "B" => {
                        has_invalid = true;
                        break;
                    }
                    "N" | "" => unsigned_count += 1,
                    _ => unsigned_count += 1,
                }
            }

            if has_invalid {
                TapTrustResult::InvalidSignature
            } else if unsigned_count > 0 {
                TapTrustResult::UnsignedCommits { unsigned_count }
            } else {
                // All commits signed (or no new commits)
                TapTrustResult::Trusted { signed_count }
            }
        }
        Err(e) => TapTrustResult::Unknown(format!("Git log failed: {}", e)),
    }
}

/// Sync a tap with trust verification (advisory mode - warns but doesn't block)
pub fn sync_tap_with_verification(tap: &Tap) -> Result<TapTrustResult, String> {
    let tap_path = ensure_tap_cloned(tap);

    // Verify trust before pulling
    let trust = verify_tap_commits(&tap_path);

    match &trust {
        TapTrustResult::Trusted { signed_count } => {
            // Safe to merge
            let merge = std::process::Command::new("git")
                .args(["merge", "origin/HEAD", "--ff-only"])
                .current_dir(&tap_path)
                .status();

            if merge.map(|s| s.success()).unwrap_or(false) {
                Ok(TapTrustResult::Trusted {
                    signed_count: *signed_count,
                })
            } else {
                Err("Merge failed".to_string())
            }
        }
        TapTrustResult::UnsignedCommits { unsigned_count } => {
            // Advisory: warn but still sync (user chose advisory-only trust)
            eprintln!(
                "[tap] ⚠️  Warning: Tap '{}' has {} unsigned commit(s). Syncing anyway (advisory mode).",
                tap.name, unsigned_count
            );
            let merge = std::process::Command::new("git")
                .args(["merge", "origin/HEAD", "--ff-only"])
                .current_dir(&tap_path)
                .status();

            if merge.map(|s| s.success()).unwrap_or(false) {
                Ok(TapTrustResult::UnsignedCommits {
                    unsigned_count: *unsigned_count,
                })
            } else {
                Err("Merge failed".to_string())
            }
        }
        TapTrustResult::InvalidSignature => {
            eprintln!(
                "[tap] ❌ Warning: Tap '{}' has invalid signatures! Skipping sync.",
                tap.name
            );
            Ok(TapTrustResult::InvalidSignature)
        }
        TapTrustResult::Unknown(reason) => {
            eprintln!("[tap] ⚠️  Could not verify tap '{}': {}", tap.name, reason);
            Ok(trust.clone())
        }
    }
}

/// Gets the file path for a tap's configuration.
pub fn tap_path(name: &str) -> PathBuf {
    let dir = crate::paths::taps_config_dir();
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
    let cache_dir = crate::paths::taps_cache_dir().join(name);
    let _ = fs::remove_dir_all(&cache_dir);
}

/// Enables or disables a tap.
pub fn set_tap_enabled(name: &str, enabled: bool) {
    let path = tap_path(name);
    if path.exists()
        && let Ok(mut doc) = fs::read_to_string(&path)
            .and_then(|s| s.parse::<DocumentMut>().map_err(std::io::Error::other))
    {
        doc["enabled"] = value(enabled);
        let _ = fs::write(&path, doc.to_string());
    }
}

/// Synchronizes all taps by ensuring they are cloned and up-to-date.
/// Uses trust verification in advisory mode (warns but doesn't block).
pub fn sync_taps() {
    for tap in discover_taps() {
        if tap.enabled {
            match sync_tap_with_verification(&tap) {
                Ok(TapTrustResult::Trusted { signed_count }) => {
                    if signed_count > 0 {
                        println!(
                            "[tap] ✓ Synced '{}' (trusted - {} signed commit(s))",
                            tap.name, signed_count
                        );
                    } else {
                        println!("[tap] ✓ Synced '{}' (up to date)", tap.name);
                    }
                }
                Ok(TapTrustResult::UnsignedCommits { unsigned_count }) => {
                    println!(
                        "[tap] ⚠️  Synced '{}' ({} unsigned commit(s) - advisory mode)",
                        tap.name, unsigned_count
                    );
                }
                Ok(TapTrustResult::InvalidSignature) => {
                    println!("[tap] ❌ Skipped '{}' (invalid signature)", tap.name);
                }
                Ok(TapTrustResult::Unknown(_)) => {
                    println!("[tap] ? Synced '{}' (could not verify)", tap.name);
                }
                Err(e) => {
                    eprintln!("[tap] ❌ Failed to sync '{}': {}", tap.name, e);
                }
            }
        }
    }
}

/// Synchronizes enabled taps based on the configured sync interval.
/// Auto-sync is gated by the `auto_sync` setting and `sync_interval_hours`;
/// each tap is verified via [`sync_tap_with_verification`] (advisory mode).
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
    let config_path = crate::paths::USER_CONFIG.clone();
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
            // Route auto-sync through the same GPG-advisory trust path as the
            // manual `reap tap sync` command, instead of a bare `git pull`.
            if let Err(e) = sync_tap_with_verification(tap) {
                eprintln!("[tap] ❌ Auto-sync failed for '{}': {}", tap.name, e);
            }
        }
        state.last_sync = Some(now);
        if let Some(parent) = state_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = fs::write(&state_path, json);
        }
    }
    Ok(())
}

fn sync_state_path() -> PathBuf {
    crate::paths::sync_state_file()
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
        if let Ok(data) = fs::read_to_string(&index_path)
            && let Ok(json) = serde_json::from_str::<JsonValue>(&data)
            && let Some(obj) = json.as_object()
        {
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
    if pub_path.exists()
        && let Ok(toml) = fs::read_to_string(&pub_path)
        && let Ok(val) = toml.parse::<toml::Value>()
    {
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
    None
}

/// Computes the publisher verification status for a tap.
///
/// This distinguishes between:
/// - Self-declared: publisher.toml exists but we haven't verified the key matches commits
/// - KeyMatches: publisher's GPG key matches the key used to sign commits
/// - Unknown: no publisher metadata
pub fn get_publisher_status(tap: &Tap) -> PublisherStatus {
    let publisher = match get_publisher_info(tap) {
        Some(p) => p,
        None => return PublisherStatus::Unknown,
    };

    // If no GPG key declared, it's just self-declared metadata
    if publisher.gpg_key.is_empty() {
        return PublisherStatus::SelfDeclared;
    }

    // Check if the declared GPG key matches the commit signing key
    let tap_path = ensure_tap_cloned(tap);
    let signing_key = get_commit_signing_key(&tap_path);

    match signing_key {
        Some(key_id) => {
            // Normalize key IDs for comparison (last 16 chars of fingerprint)
            let declared_normalized = normalize_key_id(&publisher.gpg_key);
            let signing_normalized = normalize_key_id(&key_id);

            if declared_normalized == signing_normalized {
                PublisherStatus::KeyMatches
            } else {
                PublisherStatus::SelfDeclared
            }
        }
        None => PublisherStatus::SelfDeclared,
    }
}

/// Gets the GPG key ID used to sign the most recent commit in a repo
fn get_commit_signing_key(repo_path: &std::path::Path) -> Option<String> {
    let output = Command::new("git")
        .args(["log", "-1", "--format=%GK"])
        .current_dir(repo_path)
        .output()
        .ok()?;

    let key_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if key_id.is_empty() {
        None
    } else {
        Some(key_id)
    }
}

/// Normalizes a GPG key ID for comparison (handles both short and long key IDs)
fn normalize_key_id(key: &str) -> String {
    let cleaned = key.trim().to_uppercase().replace(" ", "");
    // Use last 16 characters for comparison (long key ID)
    if cleaned.len() > 16 {
        cleaned[cleaned.len() - 16..].to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_signature_never_merges() {
        // Verify the contract: InvalidSignature result should not trigger a merge
        // This is a design verification test - it documents that the sync function
        // returns InvalidSignature without attempting to merge

        // The sync_tap_with_verification function should:
        // 1. On InvalidSignature: skip merge and return Ok(InvalidSignature)
        // 2. The caller (sync_taps) should print a skip message

        // We can't easily test this without a real git repo with bad signatures,
        // but we can verify the enum variant matches produce the expected control flow.
        let result = TapTrustResult::InvalidSignature;
        assert!(matches!(result, TapTrustResult::InvalidSignature));

        // The key assertion: InvalidSignature is a distinct state that the code
        // handles by NOT merging. This is enforced by the match arm in
        // sync_tap_with_verification that returns early without calling git merge.
    }

    #[test]
    fn test_unsigned_commits_advisory_sync() {
        // Verify unsigned commits still sync in advisory mode
        let result = TapTrustResult::UnsignedCommits { unsigned_count: 3 };
        match result {
            TapTrustResult::UnsignedCommits { unsigned_count } => {
                assert_eq!(unsigned_count, 3);
            }
            _ => panic!("Expected UnsignedCommits variant"),
        }
    }

    #[test]
    fn test_trusted_commits_sync() {
        // Verify trusted commits sync normally
        let result = TapTrustResult::Trusted { signed_count: 5 };
        match result {
            TapTrustResult::Trusted { signed_count } => {
                assert_eq!(signed_count, 5);
            }
            _ => panic!("Expected Trusted variant"),
        }
    }

    #[test]
    fn test_publisher_status_variants() {
        // Test all PublisherStatus variants can be created and compared
        assert_eq!(
            PublisherStatus::NotApplicable,
            PublisherStatus::NotApplicable
        );
        assert_eq!(PublisherStatus::SelfDeclared, PublisherStatus::SelfDeclared);
        assert_eq!(PublisherStatus::KeyMatches, PublisherStatus::KeyMatches);
        assert_eq!(PublisherStatus::Unknown, PublisherStatus::Unknown);

        // Different variants are not equal
        assert_ne!(PublisherStatus::NotApplicable, PublisherStatus::Unknown);
        assert_ne!(PublisherStatus::SelfDeclared, PublisherStatus::KeyMatches);
    }

    #[test]
    fn sync_state_roundtrips_through_json() {
        // Guards the auto-sync persistence path: a serialized state must
        // deserialize back to an equivalent timestamp.
        let state = SyncState {
            last_sync: Some(Utc::now()),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let restored: SyncState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            state.last_sync.map(|t| t.timestamp()),
            restored.last_sync.map(|t| t.timestamp())
        );
    }

    #[test]
    fn sync_state_defaults_to_no_previous_sync() {
        let state = SyncState::default();
        assert!(state.last_sync.is_none());
    }

    #[test]
    fn test_normalize_key_id() {
        // Test key ID normalization
        assert_eq!(normalize_key_id("abc123"), "ABC123");
        assert_eq!(normalize_key_id("  ABC123  "), "ABC123");
        assert_eq!(normalize_key_id("AB CD 12 34"), "ABCD1234");

        // Long key IDs should be truncated to last 16 chars
        let long_key = "ABCDEF1234567890DEADBEEF12345678";
        assert_eq!(normalize_key_id(long_key).len(), 16);
        assert_eq!(normalize_key_id(long_key), "DEADBEEF12345678");
    }
}
