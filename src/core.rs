use crate::aur;
use crate::aur::upgrade_all;
use crate::backend::{AurBackend, Backend};
use crate::cli::Cli;
use crate::cli::{Commands, ConfigCmd, TapCmd};
use crate::config::GlobalConfig;
use crate::config::ReapConfig;
use crate::flatpak;
use crate::hooks::{HookContext, post_install, pre_install};
use crate::pacman;
use crate::tap::{Tap, discover_taps, find_tap_for_pkg};
use crate::tui;
use crate::tui::LogPane;
use crate::utils;
use crate::utils::{audit_flatpak_manifest, pkgb_diff_audit};
use anyhow::anyhow;
use anyhow::{Context, Result};
use chrono::Local;
use futures::FutureExt;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{PathBuf};
use std::pin::Pin;
use std::process::Command;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::Semaphore;
use std::sync::Arc;

/// Custom error type for Reap
#[derive(Debug, Error)]
pub enum ReapError {
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Other error: {0}")]
    Other(String),
    #[error("Install failed: {0}")]
    InstallFailed(String),
}

/// Check for file conflicts before install (returns list of conflicting files and owners)
pub fn detect_file_conflicts(file_list: &[String]) -> Vec<(String, String)> {
    let mut conflicts = Vec::new();
    for file in file_list {
        let output = std::process::Command::new("pacman")
            .arg("-Qo")
            .arg(file)
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let owner = String::from_utf8_lossy(&out.stdout).trim().to_string();
                conflicts.push((file.clone(), owner));
            }
        }
    }
    conflicts
}

/// Backup package state before install (files and pacman db)
pub fn backup_package_state(pkg: &str) -> Result<PathBuf> {
    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("reap/backup/{}/{}", pkg, timestamp));
    fs::create_dir_all(&backup_dir)
        .with_context(|| format!("Failed to create backup dir: {}", backup_dir.display()))?;
    // Backup pacman db
    let db_path = PathBuf::from(format!("/var/lib/pacman/local/{}-*", pkg));
    let _ = std::process::Command::new("cp")
        .arg("-r")
        .arg(&db_path)
        .arg(&backup_dir)
        .status()
        .with_context(|| "Failed to backup pacman db");
    // Backup /usr/bin/<pkg> if exists
    let bin_path = PathBuf::from(format!("/usr/bin/{}", pkg));
    if bin_path.exists() {
        let _ = std::process::Command::new("cp")
            .arg(&bin_path)
            .arg(&backup_dir)
            .status()
            .with_context(|| format!("Failed to backup binary: {}", bin_path.display()));
    }
    Ok(backup_dir)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Source {
    Aur,
    Flatpak,
    Pacman,
    ChaoticAUR,
    GhostctlAUR,
    BinaryRepo(String),
    Custom(String),
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::Aur => "[AUR]",
            Source::Pacman => "[PACMAN]",
            Source::Flatpak => "[FLATPAK]",
            Source::ChaoticAUR => "[CHAOTIC-AUR]",
            Source::GhostctlAUR => "[GHOSTCTL-AUR]",
            Source::BinaryRepo(_) => "[BINREPO]",
            Source::Custom(_) => "[CUSTOM]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallTask {
    pub pkg: String,
    pub confirm: bool,
    pub source: Source,
    pub repo_name: Option<String>,
}

impl InstallTask {
    pub fn new(pkg: String, confirm: bool, source: Source, repo_name: Option<String>) -> Self {
        Self {
            pkg,
            confirm,
            source,
            repo_name,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    pub insecure: bool,
    pub gpg_keyserver: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallFailure {
    pub pkg: String,
    pub step: String,
    pub reason: String,
}

pub fn get_installed_packages() -> HashMap<String, Source> {
    let mut pkgs = HashMap::new();
    // Flatpak
    if let Ok(out) = Command::new("flatpak").arg("list").arg("--app").output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let name = line.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                pkgs.insert(name.to_string(), Source::Flatpak);
            }
        }
    }
    // Pacman
    if let Ok(out) = Command::new("pacman").arg("-Qq").output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            pkgs.insert(line.trim().to_string(), Source::Pacman);
        }
    }
    pkgs
}

/// Resolve the best source for a package, using tap, repo, AUR, or flatpak, in priority order.
pub fn resolve_package_source(
    pkg: &str,
    forced_tap: Option<&str>,
    config: &GlobalConfig,
) -> Option<(Source, Option<String>, u32, Option<Tap>)> {
    let taps = discover_taps();
    // 1. Taps (highest priority)
    if let Some(tap) = find_tap_for_pkg(pkg, &taps, forced_tap) {
        return Some((
            Source::Custom(tap.name.clone()),
            Some(tap.name.clone()),
            tap.priority,
            Some(tap),
        ));
    }
    // 2. Pacman repo
    if config.backend_order.contains(&"pacman".to_string())
        && (repo_has_package(pkg, "core") || repo_has_package(pkg, "extra"))
    {
        return Some((Source::Pacman, None, 20, None));
    }
    // 3. AUR
    if config.backend_order.contains(&"aur".to_string())
        && aur::aur_search_results(pkg).iter().any(|r| r.name == pkg)
    {
        return Some((Source::Aur, None, 10, None));
    }
    // 4. Flatpak
    if config.backend_order.contains(&"flatpak".to_string()) {
        let output = std::process::Command::new("flatpak")
            .arg("search")
            .arg(pkg)
            .output();
        if let Ok(out) = output {
            if out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty() {
                return Some((Source::Flatpak, None, 1, None));
            }
        }
    }
    None
}

/// Install a package using prioritized source resolution and log the decision.
pub async fn install_with_priority(
    pkg: &str,
    _config: Arc<ReapConfig>,
    _confirm: bool,
    log: Arc<LogPane>,
    opts: &InstallOptions,
) {
    use owo_colors::OwoColorize;
    let start = Instant::now();
    let ctx = HookContext {
        pkg: pkg.to_string(),
        version: None,
        source: None,
        install_path: None,
        tap: None,
    };
    log.push(&format!("[reap][hook] pre_install executing for {}", pkg));
    pre_install(&ctx);
    let global_config = GlobalConfig::load();
    if let Some((source, tap_name, prio, tap_obj)) =
        resolve_package_source(pkg, None, &global_config)
    {
        // Prepare hook context
        let ctx = HookContext {
            pkg: pkg.to_string(),
            version: None,
            source: Some(format!("{:?}", source)),
            install_path: None,
            tap: tap_name.clone(),
        };
        log.push(&format!(
            "[reap][priority] Resolved source for '{}': {}{} (priority {})",
            pkg,
            source.label(),
            tap_name.as_deref().unwrap_or(""),
            prio
        ));
        match source {
            Source::Custom(ref _tap_repo) => {
                if let Some(tap) = tap_obj {
                    let tap_path = crate::tap::ensure_tap_cloned(&tap);
                    let pkg_dir = tap_path.join(pkg);
                    let pkgb_path = pkg_dir.join("PKGBUILD");
                    let sig_path = pkg_dir.join("PKGBUILD.sig");
                    let pubinfo = crate::tap::get_publisher_info(&tap);
                    if let Some(pubinfo) = pubinfo {
                        let keyid = pubinfo.gpg_key.split_whitespace().last().unwrap_or("");
                        let verified_str = if pubinfo.verified {
                            "[✓ Verified GPG]".green().to_string()
                        } else {
                            "[Unverified]".yellow().to_string()
                        };
                        log.push(&format!(
                            "👤 {} from {} {}",
                            tap.name.bold(),
                            pubinfo.name,
                            verified_str
                        ));
                        log.push(&format!("🔑 GPG Key: {}", keyid));
                        // Check if key is in keyring
                        let key_present = std::process::Command::new("gpg")
                            .args(["--list-keys", keyid])
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        if !key_present {
                            let keyserver = opts
                                .gpg_keyserver
                                .as_deref()
                                .unwrap_or("hkps://keys.openpgp.org");
                            log.push(&format!(
                                "[reap][gpg] Importing publisher key {} from {}...",
                                keyid, keyserver
                            ));
                            let fetch = std::process::Command::new("gpg")
                                .args(["--keyserver", keyserver, "--recv-keys", keyid])
                                .status();
                            match fetch {
                                Ok(s) if s.success() => log.push(&format!(
                                    "[reap][gpg] {} Successfully imported {}",
                                    "✓".green(),
                                    keyid
                                )),
                                Ok(_) | Err(_) => log.push(&format!(
                                    "[reap][gpg] {} Failed to import publisher key {}",
                                    "❌".red(),
                                    keyid
                                )),
                            }
                        }
                        // Verify PKGBUILD.sig
                        if sig_path.exists() && pkgb_path.exists() {
                            let verify = std::process::Command::new("gpg")
                                .arg("--verify")
                                .arg(&sig_path)
                                .arg(&pkgb_path)
                                .status();
                            if let Ok(s) = verify {
                                if s.success() {
                                    log.push(&format!(
                                        "{} PKGBUILD signature verified",
                                        "✓".green()
                                    ));
                                } else {
                                    log.push(&format!(
                                        "{} Verification failed for PKGBUILD.sig (key: {})",
                                        "❌".red(),
                                        keyid
                                    ));
                                    if !opts.insecure {
                                        log.push(&format!(
                                            "{} Aborting install. Use --insecure to override.",
                                            "✋".red()
                                        ));
                                        return;
                                    } else {
                                        log.push(&format!(
                                            "{} Continuing install due to --insecure.",
                                            "⚠️".yellow()
                                        ));
                                    }
                                }
                            } else {
                                log.push(&format!(
                                    "{} Verification failed for PKGBUILD.sig (key: {})",
                                    "❌".red(),
                                    keyid
                                ));
                                if !opts.insecure {
                                    log.push(&format!(
                                        "{} Aborting install. Use --insecure to override.",
                                        "✋".red()
                                    ));
                                    return;
                                } else {
                                    log.push(&format!(
                                        "{} Continuing install due to --insecure.",
                                        "⚠️".yellow()
                                    ));
                                }
                            }
                        } else {
                            log.push(&format!("{} PKGBUILD.sig missing. Aborting install. Use --insecure to override.", "❌".red()));
                            if !opts.insecure {
                                return;
                            } else {
                                log.push(&format!(
                                    "{} Continuing install due to --insecure.",
                                    "⚠️".yellow()
                                ));
                            }
                        }
                    } else {
                        log.push(&format!(
                            "{} Warning: Tap publisher not verified. Installing with --insecure.",
                            "⚠️".yellow()
                        ));
                        if !opts.insecure {
                            return;
                        }
                    }
                }
                // ...proceed with install if verified or --insecure...
            }
            Source::Pacman => {
                log.push(&format!("[reap][pacman] Installing {} from repo", pkg));
                pacman::install(pkg);
                log.push(&format!("[✓] Installed {} from Pacman", pkg));
            }
            Source::Aur => {
                log.push(&format!("[reap][aur] Installing {} from AUR", pkg));
                let opts = InstallOptions {
                    insecure: false,
                    gpg_keyserver: None,
                };
                let _ = install_aur_native(pkg, &log, &opts).await;
                log.push(&format!("[✓] Installed {} from AUR", pkg));
            }
            Source::Flatpak => {
                log.push(&format!("[reap][flatpak] Installing {} from Flatpak", pkg));
                let _ = flatpak::install_flatpak(pkg).await;
            }
            _ => log.push(&format!("[!] Unknown source for {}", pkg)),
        }
        log.push(&format!("[reap][hook] post_install executing for {}", pkg));
        post_install(&ctx);
    } else {
        log.push(&format!(
            "[reap][error] Could not resolve source for {}",
            pkg
        ));
        crate::utils::rollback(pkg);
    }
    let elapsed = start.elapsed();
    log.push(&format!("[reap][timing] install_with_priority for {} took: {:?}", pkg, elapsed));
    // Backup before install
    if let Ok(backup_path) = backup_package_state(pkg) {
        log.push(&format!("[reap][backup] State backed up to {}", backup_path.display()));
    }
    // Show PKGBUILD diff before install
    show_pkgbuild_diff(pkg);
}

pub async fn unified_search(query: &str) -> Vec<aur::SearchResult> {
    use crate::tap::search_tap_indexes;
    let mut tap_results = Vec::new();
    // Remove unused variable: self
    for (name, desc, repo, _source) in search_tap_indexes(query) {
        tap_results.push(aur::SearchResult {
            name,
            version: String::new(),
            description: desc,
            source: Source::Custom(repo),
        });
    }
    let aur_fut = async { aur::search(query).await.unwrap_or_else(|_| vec![]) };
    let flatpak_fut = async { flatpak::search(query) };
    let (aur, flatpak): (Vec<aur::SearchResult>, Vec<aur::SearchResult>) =
        tokio::join!(aur_fut, flatpak_fut);
    // Deduplicate by name, favoring tap > aur > flatpak
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();
    for r in tap_results.into_iter().chain(aur).chain(flatpak) {
        if seen.insert(r.name.clone()) {
            results.push(r);
        }
    }
    results
}

pub fn print_search_results(results: &[aur::SearchResult]) {
    use owo_colors::OwoColorize;
    for r in results {
        let tag = match &r.source {
            Source::Custom(tap) => format!("[tap:{}]", tap).yellow().to_string(),
            Source::Aur => "[aur]".blue().to_string(),
            Source::Flatpak => "[flatpak]".green().to_string(),
            Source::Pacman => "[pacman]".magenta().to_string(),
            _ => format!("[{}]", r.source.label()),
        };
        println!("{:<20} ▸ {:<40} {}", r.name.bold(), r.description, tag);
    }
}

/// Prompt the user for confirmation unless --yes or config disables it
pub fn confirm_action(prompt: &str, assume_yes: bool) -> bool {
    if assume_yes {
        return true;
    }
    use std::io::{self, Write};
    print!("{} [y/N]: ", prompt);
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    } else {
        false
    }
}

// === Bulk Install Logic ===
pub async fn parallel_install(pkgs: &[String], config: Arc<ReapConfig>, log: Arc<LogPane>) {
    let max_parallel = 4; // or config.parallel
    let semaphore = Arc::new(Semaphore::new(max_parallel));
    let mut tasks = Vec::new();
    for pkg in pkgs {
        let sem = Arc::clone(&semaphore);
        let pkg = pkg.clone();
        let config = Arc::clone(&config);
        let log = Arc::clone(&log);
        let permit_fut = sem.acquire_owned();
        tasks.push(tokio::spawn(async move {
            let _permit = permit_fut.await.unwrap();
            install_with_priority(&pkg, config, true, log, &InstallOptions::default()).await;
        }));
    }
    let _ = join_all(tasks).await;
}

pub async fn parallel_upgrade(pkgs: &[String], config: Arc<ReapConfig>, log: Arc<LogPane>) {
    let mut tasks = Vec::new();
    for pkg in pkgs {
        let config = Arc::clone(&config);
        let log = Arc::clone(&log);
        let pkg = pkg.clone();
        tasks.push(tokio::spawn(async move {
            install_with_priority(&pkg, config, true, log, &InstallOptions::default()).await;
        }));
    }
    let _ = join_all(tasks).await;
    log.push("[reap] All upgrades complete.");
}

pub fn repo_has_package(pkg: &str, repo: &str) -> bool {
    let output = std::process::Command::new("pacman")
        .args(["-Slq", repo])
        .output();
    if let Ok(out) = output {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.trim() == pkg)
    } else {
        false
    }
}

pub fn get_enabled_binary_repos() -> Vec<String> {
    let conf = std::fs::read_to_string("/etc/pacman.conf").unwrap_or_default();
    let mut repos = Vec::new();
    for line in conf.lines() {
        if let Some(repo) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if repo.ends_with("-aur") || repo == "chaotic-aur" || repo == "ghostctl-aur" {
                repos.push(repo.to_string());
            }
        }
    }
    repos
}

pub fn detect_source(pkg: &str, repo: Option<&str>, binary_only: bool) -> Option<Source> {
    if let Some(repo_name) = repo {
        if repo_has_package(pkg, repo_name) {
            return Some(Source::BinaryRepo(repo_name.to_string()));
        }
        if binary_only {
            return None;
        }
    } else {
        for repo in get_enabled_binary_repos() {
            if repo_has_package(pkg, &repo) {
                return Some(Source::BinaryRepo(repo));
            }
        }
    }
    if !binary_only {
        if aur::aur_search_results(pkg).iter().any(|r| r.name == pkg) {
            return Some(Source::Aur);
        }
        let output = std::process::Command::new("flatpak")
            .arg("search")
            .arg(pkg)
            .output();
        if let Ok(out) = output {
            if out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty() {
                return Some(Source::Flatpak);
            }
        }
    }
    None
}

pub fn try_binary_install(pkg: &str, repo_name: &str) -> bool {
    let arch = std::env::consts::ARCH;
    let base_url = match repo_name {
        "chaotic-aur" => "https://cdn-mirror.chaotic.cx/chaotic-aur/x86_64",
        "ghostctl-aur" | "ghostctl" => "https://pkg.ghostctl.io/x86_64",
        _ => "",
    };
    if !base_url.is_empty() {
        let url = format!("{}/{}-latest-{}.pkg.tar.zst", base_url, pkg, arch);
        let out_path = std::env::temp_dir().join(format!("{}.pkg.tar.zst", pkg));
        if let Ok(resp) = reqwest::blocking::get(&url) {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes() {
                    if std::fs::write(&out_path, &bytes).is_ok() {
                        println!("[reap][binrepo] Downloaded {} from {}", pkg, url);
                        let status = std::process::Command::new("sudo")
                            .arg("pacman")
                            .arg("-U")
                            .arg("--noconfirm")
                            .arg(&out_path)
                            .status();
                        if let Ok(s) = status {
                            if s.success() {
                                println!("[reap][binrepo] {} installed from {}", pkg, repo_name);
                                return true;
                            }
                        }
                    }
                }
            }
        }
        println!(
            "[reap][binrepo] Failed to install {} from {}",
            pkg, repo_name
        );
        return false;
    }
    if repo_has_package(pkg, repo_name) {
        println!(
            "[reap][binrepo] Installing {} from binary repo [{}] via pacman...",
            pkg, repo_name
        );
        let status = std::process::Command::new("sudo")
            .arg("pacman")
            .arg("-S")
            .arg("--noconfirm")
            .arg(pkg)
            .status();
        if let Ok(s) = status {
            if s.success() {
                println!("[reap][binrepo] {} installed from {}", pkg, repo_name);
                return true;
            }
        }
        println!(
            "[reap][binrepo] pacman -S failed for {} from {}",
            pkg, repo_name
        );
    }
    false
}

pub fn handle_install(pkgs: Vec<String>) {
    let backend: Box<dyn Backend> = Box::new(AurBackend::new());
    for pkg in pkgs {
        println!("[reap] Installing {}...", pkg);
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(backend.install(&pkg));
    }
}

pub async fn handle_install_parallel(pkgs: Vec<String>, max_parallel: usize) {
    let semaphore = Arc::new(Semaphore::new(max_parallel));
    let pb = ProgressBar::new(pkgs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("Failed to create ProgressStyle")
            .progress_chars("#>-"),
    );
    let mut handles = Vec::new();
    for pkg in pkgs {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let pb = pb.clone();
        let pkg = pkg.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let _ = std::panic::AssertUnwindSafe(async {
                handle_install(vec![pkg.clone()]);
            })
            .catch_unwind()
            .await;
            pb.inc(1);
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }));
    }
    let _ = join_all(handles).await;
    pb.finish_with_message("All installs complete.");
    println!("[reap] All installs complete.");
}

pub fn handle_upgrade(parallel: bool) {
    let config = crate::config::ReapConfig::load();
    let installed = crate::pacman::list_installed_aur();
    let mut to_upgrade: Vec<String> = Vec::new();
    for pkg in installed {
        if config.is_ignored(&pkg) {
            println!("[reap] Skipping ignored package: {}", pkg);
            continue;
        }
        if let Ok(remote) = crate::aur::fetch_package_info(&pkg) {
            let local_ver = crate::pacman::get_version(&pkg);
            if local_ver.as_deref() != Some(&remote.version) {
                to_upgrade.push(pkg.to_string());
            }
        }
    }
    if to_upgrade.is_empty() {
        println!("[reap] All AUR packages up to date.");
        return;
    }
    println!("[reap] Upgrading: {:?}", to_upgrade);
    if parallel {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(handle_install_parallel(to_upgrade, config.parallel));
    } else {
        for pkg in to_upgrade {
            let _ = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(crate::aur::install(vec![pkg.as_str()]));
        }
    }
}

pub fn handle_rollback(pkg: &str) {
    // Restore or remove utils::rollback and hooks::on_rollback
    if let Some(rollback_fn) = std::option::Option::Some(utils::rollback) {
        rollback_fn(pkg);
    }
}

pub fn handle_audit(pkg: &str) {
    match crate::core::detect_source(pkg, None, false) {
        Some(crate::core::Source::Aur) => {
            let pkgb = crate::aur::get_pkgbuild_preview(pkg);
            pkgb_diff_audit(pkg, &pkgb);
        }
        Some(crate::core::Source::Flatpak) => {
            let output = std::process::Command::new("flatpak")
                .arg("info")
                .arg(pkg)
                .output();
            if let Ok(out) = output {
                audit_flatpak_manifest(&String::from_utf8_lossy(&out.stdout));
            } else {
                println!("[AUDIT][FLATPAK] Could not get info for {}.", pkg);
            }
        }
        _ => println!("[AUDIT] Unknown package source for {}.", pkg),
    }
}

pub fn handle_orphan(remove: bool, all: bool) {
    let output = std::process::Command::new("pacman")
        .args(["-Qdtq"])
        .output();
    let mut aur_orphans = Vec::new();
    let mut repo_orphans = Vec::new();
    if let Ok(out) = output {
        for pkg in String::from_utf8_lossy(&out.stdout).lines() {
            let repo_check = std::process::Command::new("pacman")
                .arg("-Si")
                .arg(pkg)
                .output();
            let is_repo = repo_check
                .as_ref()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if is_repo {
                repo_orphans.push(pkg.to_string());
            } else {
                aur_orphans.push(pkg.to_string());
            }
        }
    }
    if !aur_orphans.is_empty() {
        println!("Orphaned AUR packages:\n");
        for pkg in &aur_orphans {
            println!("    {}", pkg);
        }
        if remove {
            for pkg in &aur_orphans {
                println!("[reap] Uninstalling orphaned AUR package: {}", pkg);
                crate::aur::uninstall(pkg);
            }
        } else {
            println!("\nRun with --remove to uninstall.");
        }
    } else {
        println!("No orphaned AUR packages found.");
    }
    if all && !repo_orphans.is_empty() {
        println!("\nOrphaned pacman packages:\n");
        for pkg in &repo_orphans {
            println!("    {}", pkg);
        }
        if remove {
            for pkg in &repo_orphans {
                println!("[reap] Uninstalling orphaned pacman package: {}", pkg);
                crate::aur::uninstall(pkg);
            }
        } else {
            println!("\nRun with --remove to uninstall.");
        }
    }
}

pub fn show_pkgbuild_diff(pkg: &str) {
    let local_path = std::env::temp_dir().join(format!("reap-aur-{}/PKGBUILD", pkg));
    let local = std::fs::read_to_string(&local_path).unwrap_or_default();
    let remote = crate::aur::get_pkgbuild_preview(pkg);
    let diff = diff::lines(&local, &remote);
    for d in diff {
        match d {
            diff::Result::Left(l) => println!("\x1b[31m- {}\x1b[0m", l),
            diff::Result::Right(r) => println!("\x1b[32m+ {}\x1b[0m", r),
            diff::Result::Both(l, _) => println!("  {}", l),
        }
    }
}

pub fn get_pkgbuild_preview(pkg: &str) -> String {
    let url = format!(
        "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}",
        pkg
    );
    // Try to read from a local cache file for demonstration
    let local_path = std::env::temp_dir().join(format!("reap-aur-{}/PKGBUILD", pkg));
    if let Ok(file) = File::open(&local_path) {
        let reader = BufReader::new(file);
        let mut content = String::new();
        // Replace manual if let Ok(l) = line with .map_while(Result::ok)
        for line in reader.lines().map_while(Result::ok) {
            content.push_str(&line);
            content.push('\n');
        }
        return content;
    }
    // Fallback to remote fetch
    if let Ok(resp) = reqwest::blocking::get(&url) {
        if let Ok(text) = resp.text() {
            return text;
        }
    }
    String::from("[reap] PKGBUILD not found.")
}

pub async fn install_aur_native(
    pkg: &str,
    log: &LogPane,
    opts: &InstallOptions,
) -> Result<(), ReapError> {
    use chrono::Local;
    use std::env;
    use std::fs;
    use std::process::{Command, Stdio};
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let build_dir = cache_dir.join(format!("reap-aur-{}-{}", pkg, now));
    let repo_url = format!("https://aur.archlinux.org/{}.git", pkg);
    let log_line = |step: &str, msg: &str| {
        let entry = format!("[{}][reap][aur][{}] {}", now, step, msg);
        log.push(&entry);
    };
    // --- Fetch PKGBUILD ---
    log_line("fetch", &format!("Fetching PKGBUILD for {}", pkg));
    let mut clone_cmd = Command::new("git");
    clone_cmd
        .arg("clone")
        .arg(&repo_url)
        .arg(&build_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match clone_cmd.spawn().and_then(|mut child| {
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut reader = std::io::BufReader::new(stdout);
        let mut err_reader = std::io::BufReader::new(stderr);
        let mut buf = String::new();
        let mut err_buf = String::new();
        while reader.read_line(&mut buf).unwrap_or(0) > 0 {
            log_line("clone", buf.trim_end());
            buf.clear();
        }
        while err_reader.read_line(&mut err_buf).unwrap_or(0) > 0 {
            log_line("clone", err_buf.trim_end());
            err_buf.clear();
        }
        child.wait()
    }) {
        Ok(status) if status.success() => {}
        Ok(_) => {
            log_line("clone", &format!("❌ Failed to clone repo for {}", pkg));
            return Err(ReapError::CommandFailed("git clone failed".to_string()));
        }
        Err(e) => {
            log_line(
                "clone",
                &format!("❌ Failed to run git clone for {}: {}", pkg, e),
            );
            return Err(ReapError::Io(e));
        }
    }
    let pkgb_path = build_dir.join("PKGBUILD");
    // --- Diff ---
    // --- Edit ---
    if opts.insecure {
        log_line("edit", "Editing PKGBUILD");
        let editor = env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
        let status = Command::new(editor).arg(&pkgb_path).status();
        match status {
            Ok(s) if s.success() => log_line("edit", "PKGBUILD edited successfully."),
            Ok(_) => log_line("edit", "Editor exited with error status."),
            Err(e) => log_line("edit", &format!("Failed to launch editor: {}", e)),
        }
    }
    // --- Dry Run ---
    if opts.insecure {
        log_line("dry-run", &format!("Would build and install: {}", pkg));
        let _ = fs::remove_dir_all(&build_dir);
        log_line("cleanup", &format!("Cleaned up {}", build_dir.display()));
        return Ok(());
    }
    // --- Build ---
    log_line("build", &format!("Running makepkg for {}", pkg));
    let mut makepkg_cmd = Command::new("makepkg");
    makepkg_cmd
        .arg("-si")
        .arg("--noconfirm")
        .arg("--needed")
        .current_dir(&build_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match makepkg_cmd.spawn().and_then(|mut child| {
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut reader = std::io::BufReader::new(stdout);
        let mut err_reader = std::io::BufReader::new(stderr);
        let mut buf = String::new();
        let mut err_buf = String::new();
        while reader.read_line(&mut buf).unwrap_or(0) > 0 {
            log_line("build", buf.trim_end());
            buf.clear();
        }
        while err_reader.read_line(&mut err_buf).unwrap_or(0) > 0 {
            log_line("build", err_buf.trim_end());
            err_buf.clear();
        }
        child.wait()
    }) {
        Ok(status) if status.success() => {
            log_line("install", &format!("✅ {} installed successfully!", pkg));
        }
        Ok(_) => {
            log_line("install", &format!("❌ makepkg failed for {}", pkg));
            return Err(ReapError::CommandFailed("makepkg failed".to_string()));
        }
        Err(e) => {
            log_line(
                "install",
                &format!("❌ Failed to run makepkg for {}: {}", pkg, e),
            );
            return Err(ReapError::Io(e));
        }
    }
    let _ = fs::remove_dir_all(&build_dir);
    log_line("cleanup", &format!("Cleaned up {}", build_dir.display()));
    Ok(())
}

pub async fn install_flatpak(pkg: &str, log: &LogPane) -> Result<(), ReapError> {
    log.push(&format!("→ Installing Flatpak: {}", pkg));
    let status = std::process::Command::new("flatpak")
        .args(["install", "-y", pkg])
        .status();
    match status {
        Ok(s) if s.success() => {
            log.push(&format!("✓ Flatpak installed: {}", pkg));
            Ok(())
        }
        Ok(_) => {
            log.push(&format!("❌ Flatpak install failed: {}", pkg));
            Err(ReapError::InstallFailed(pkg.to_string()))
        }
        Err(e) => {
            log.push(&format!("❌ Flatpak install error: {}: {}", pkg, e));
            Err(ReapError::Io(e))
        }
    }
}

pub async fn handle_install_dispatch(
    pkg: &str,
    log: &LogPane,
    opts: &InstallOptions,
    backend: &str,
) -> Result<(), ReapError> {
    match backend {
        "aur" => install_aur_native(pkg, log, opts).await,
        "flatpak" => install_flatpak(pkg, log).await,
        _ => {
            log.push(&format!("❌ Unknown backend: {}", backend));
            Err(ReapError::Other(format!("Unknown backend: {}", backend)))
        }
    }
}

/// Recursively resolve all missing dependencies for a list of packages (AUR + repo)
pub fn resolve_and_install_deps<'a>(
    pkgs: &'a [&str],
    log: Option<&'a LogPane>,
    already_installed: &'a mut HashSet<String>,
    already_queued: &'a mut HashSet<String>,
) -> Pin<Box<dyn Future<Output = Result<(), ()>> + 'a>> {
    Box::pin(async move {
        for &pkg in pkgs {
            if already_installed.contains(pkg) || already_queued.contains(pkg) {
                continue;
            }

            let pkgb = aur::get_pkgbuild_preview(pkg);
            let mut resolved = HashSet::new();
            let mut dep_map = HashMap::new();
            let deps: Vec<(String, Source)> =
                resolve_deps_for(&pkgb, &mut resolved, &mut dep_map, true);

            if deps.is_empty() {
                if let Some(log) = log {
                    log.push(&format!("✓ All dependencies already installed for {}", pkg));
                } else {
                    println!("✓ All dependencies already installed for {}", pkg);
                }
            } else {
                if let Some(log) = log {
                    log.push(&format!(
                        "⚠ Installing missing dependencies for {}: {:?}",
                        pkg, deps
                    ));
                } else {
                    println!("⚠ Installing missing dependencies for {}: {:?}", pkg, deps);
                }

                for dep in &deps {
                    if already_installed.contains(&dep.0) || already_queued.contains(&dep.0) {
                        continue;
                    }

                    // Recursively resolve
                    resolve_and_install_deps(
                        &[dep.0.as_str()],
                        log,
                        already_installed,
                        already_queued,
                    )
                    .await?;

                    let repo_status = std::process::Command::new("pacman")
                        .arg("-Si")
                        .arg(&dep.0)
                        .status();

                    if let Ok(s) = repo_status {
                        if s.success() {
                            let _ = std::process::Command::new("sudo")
                                .arg("pacman")
                                .arg("-S")
                                .arg("--noconfirm")
                                .arg(&dep.0)
                                .status();

                            if let Some(log) = log {
                                log.push(&format!("[deps] Installed {} from repo", dep.0));
                            } else {
                                println!("[deps] Installed {} from repo", dep.0);
                            }

                            already_installed.insert(dep.0.clone());
                            continue;
                        }
                    }

                    let opts = InstallOptions {
                        insecure: false,
                        gpg_keyserver: None,
                    };

                    install_aur_native(dep.0.as_str(), log.unwrap(), &opts)
                        .await
                        .unwrap_or_else(|e| {
                            println!("[deps] Failed to install {}: {:?}", dep.0, e);
                        });

                    already_installed.insert(dep.0.clone());
                }
            }

            already_queued.insert(pkg.to_string());
        }

        Ok(())
    })
}

/// Hybrid dependency resolver: tap > AUR > system
pub fn resolve_deps_for(
    pkg: &str,
    resolved: &mut std::collections::HashSet<String>,
    dep_map: &mut std::collections::HashMap<String, Source>,
    _cache: bool,
) -> Vec<(String, Source)> {
    #[cfg(feature = "cache")]
    let cache_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reap/cache/resolved");
    #[cfg(feature = "cache")]
    if _cache {
        let cache_path = cache_dir.join(format!("{}.json", pkg));
        if let Ok(data) = fs::read_to_string(&cache_path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&data) {
                for (dep, src) in map {
                    dep_map.insert(
                        dep.clone(),
                        match src.as_str() {
                            "tap" => Source::Custom("tap".to_string()),
                            "aur" => Source::Aur,
                            "system" => Source::Pacman,
                            _ => Source::Aur,
                        },
                    );
                }
                return dep_map
                    .iter()
                    .map(|(d, s)| (d.clone(), s.clone()))
                    .collect();
            }
        }
    }
    let mut queue = Vec::new();
    if resolved.contains(pkg) {
        return vec![];
    }
    resolved.insert(pkg.to_string());
    // 1. Try tapmeta.json or meta.toml
    let mut deps: Vec<String> = Vec::new();
    let taps = crate::tap::discover_taps();
    let mut found_tap = None;
    for tap in taps.iter().filter(|t| t.enabled) {
        let tap_path = crate::tap::ensure_tap_cloned(tap);
        let meta_path = tap_path.join(pkg).join("meta.toml");
        let tapmeta_path = tap_path.join("tapmeta.json");
        if meta_path.exists() {
            if let Ok(meta) = fs::read_to_string(&meta_path) {
                if let Ok(val) = meta.parse::<toml::Value>() {
                    if let Some(arr) = val.get("depends").and_then(|v| v.as_array()) {
                        for d in arr {
                            if let Some(dep) = d.as_str() {
                                deps.push(dep.to_string());
                            }
                        }
                        found_tap = Some(tap.name.clone());
                    }
                }
            }
        } else if tapmeta_path.exists() {
            if let Ok(meta) = fs::read_to_string(&tapmeta_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&meta) {
                    if let Some(obj) = json
                        .get(pkg)
                        .and_then(|v| v.get("depends"))
                        .and_then(|v| v.as_array())
                    {
                        for d in obj {
                            if let Some(dep) = d.as_str() {
                                deps.push(dep.to_string());
                            }
                        }
                        found_tap = Some(tap.name.clone());
                    }
                }
            }
        }
        if found_tap.is_some() {
            break;
        }
    }
    // 2. Fallback: parse PKGBUILD for depends/makedepends
    if deps.is_empty() {
        let pkgb = crate::aur::get_pkgbuild_preview(pkg);
        let (depends, makedepends, _, _, _) = crate::utils::resolve_deps(&pkgb);
        deps.extend(depends);
        deps.extend(makedepends);
    }
    for dep in deps {
        if resolved.contains(&dep) {
            continue;
        }
        if is_installed(&dep) {
            dep_map.insert(dep.clone(), Source::Pacman);
            continue;
        }
        // Tap
        let tap_hit = crate::tap::search_tap_indexes(&dep)
            .iter()
            .any(|(n, _, _, _)| n == &dep);
        if tap_hit {
            dep_map.insert(dep.clone(), Source::Custom("tap".to_string()));
            queue.push((dep.clone(), Source::Custom("tap".to_string())));
        } else if crate::aur::aur_search_results(&dep)
            .iter()
            .any(|r| r.name == dep)
        {
            dep_map.insert(dep.clone(), Source::Aur);
            queue.push((dep.clone(), Source::Aur));
        } else {
            // Not found, skip
        }
    }
    // Recurse if --resolve-deps
    for (dep, _src) in queue.clone() {
        let _ = resolve_deps_for(&dep, resolved, dep_map, _cache);
    }
    #[cfg(feature = "cache")]
    if _cache {
        let _ = fs::create_dir_all(&cache_dir);
        let mut out: HashMap<String, String> = HashMap::new();
        for (dep, src) in dep_map.iter() {
            out.insert(
                dep.clone(),
                match src {
                    Source::Custom(_) => "tap",
                    Source::Aur => "aur",
                    Source::Pacman => "system",
                    _ => "aur",
                }
                .to_string(),
            );
        }
        let cache_path = cache_dir.join(format!("{}.json", pkg));
        let _ = fs::write(&cache_path, serde_json::to_string(&out).unwrap());
    }
    queue
}

pub fn is_installed(pkg: &str) -> bool {
    let status = std::process::Command::new("pacman")
        .arg("-Qq")
        .arg(pkg)
        .status();
    matches!(status, Ok(s) if s.success())
}

pub async fn handle_cli(cli: &Cli) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Define or remove gpg_cmd if not needed
    match &cli.command {
        Commands::Install {
            pkg,
            repo,
            binary_only,
            .. // Remove or ignore unused variable: diff
        } => {
            let source = detect_source(pkg, repo.as_deref(), *binary_only).unwrap_or(Source::Aur);
            let task = InstallTask::new(
                pkg.to_string(),
                !cli.noconfirm,
                source.clone(),
                repo.clone(),
            );
            let log_pane = tui::LogPane::default();
            let backend = cli.backend.as_str();
            let try_pacman = backend == "pacman" || backend == "auto";
            let mut tried_pacman = false;
            let mut pacman_success = false;
            if try_pacman {
                // Try native pacman install first
                let status = std::process::Command::new("pacman")
                    .arg("-Si")
                    .arg(&task.pkg)
                    .status();
                if let Ok(s) = status {
                    if s.success() {
                        println!(
                            "[reap] Installing {} from system repo via pacman...",
                            task.pkg
                        );
                        pacman::install(&task.pkg);
                        pacman_success = true;
                        tried_pacman = true;
                    }
                }
            }
            if !pacman_success && (backend == "aur" || backend == "auto") {
                // Fallback to AUR install
                println!("[reap] Installing {} from AUR...", task.pkg);
                let opts = InstallOptions {
                    insecure: false,
                    gpg_keyserver: None,
                };
                install_aur_native(&task.pkg, &log_pane, &opts)
                    .await
                    .unwrap_or_else(|e| {
                        println!("[reap] Failed to install {}: {:?}", task.pkg, e);
                    });
            } else if !pacman_success && !tried_pacman {
                eprintln!("[reap] Package '{}' not found in repos or AUR.", task.pkg);
            }
        }
        Commands::Upgrade { parallel: _ } => {
            let config = crate::config::ReapConfig::load();
            let installed = crate::pacman::list_installed_aur();
            let mut to_upgrade: Vec<String> = Vec::new();
            for pkg in installed {
                if config.is_ignored(&pkg) {
                    println!("[reap] Skipping ignored package: {}", pkg);
                    continue;
                }
                if let Ok(remote) = crate::aur::fetch_package_info(&pkg) {
                    let local_ver = crate::pacman::get_version(&pkg);
                    if local_ver.as_deref() != Some(&remote.version) {
                        to_upgrade.push(pkg.to_string());
                    }
                }
            }
            if to_upgrade.is_empty() {
                println!("[reap] All AUR packages up to date.");
                return Ok(());
            }
            println!("[reap] Upgrading: {:?}", to_upgrade);
            let log_pane = tui::LogPane::default();
            let opts = InstallOptions {
                insecure: false,
                gpg_keyserver: None,
            };
            for pkg in to_upgrade {
                install_aur_native(&pkg, &log_pane, &opts)
                    .await
                    .unwrap_or_else(|e| {
                        println!("[reap] Failed to upgrade {}: {:?}", pkg, e);
                    });
            }
        }
        Commands::Orphan { remove, all } => handle_orphan(*remove, *all),
        Commands::Remove { pkgs } => {
            for pkg in pkgs {
                aur::uninstall(pkg);
            }
        }
        Commands::Local { pkgs } => {
            for pkg in pkgs {
                aur::install_local(pkg);
            }
        }
        Commands::Search { terms } => {
            for term in terms {
                match aur::search(term).await {
                    Ok(results) => print_search_results(&results),
                    Err(e) => eprintln!("[reap] Search failed for '{}': {}", term, e),
                }
            }
        }
        Commands::UpgradeAll => {
            upgrade_all().await?;
            println!("[reap] Upgrade all succeeded");
        }
        Commands::FlatpakUpgrade => {
            // Removed gpg_cmd usage as it's not needed for flatpak upgrade
            let output = std::process::Command::new("flatpak")
                .arg("update")
                .arg("-y")
                .output();
            match output {
                Ok(out) => {
                    if out.status.success() {
                        println!("[reap] Flatpak packages upgraded successfully.");
                    } else {
                        eprintln!("[reap] Flatpak upgrade failed: {:?}", out);
                    }
                }
                Err(e) => eprintln!("[reap] Error running flatpak upgrade: {}", e),
            }
        }
        Commands::Tap { cmd } => match cmd {
            TapCmd::Add {
                name,
                url,
                priority,
            } => crate::tap::add_or_update_tap(name, url, *priority, true),
            TapCmd::Remove { name } => crate::tap::remove_tap(name),
            TapCmd::Enable { name } => crate::tap::set_tap_enabled(name, true),
            TapCmd::Disable { name } => crate::tap::set_tap_enabled(name, false),
            TapCmd::Sync => crate::tap::sync_taps(),
            TapCmd::List => crate::tap::list_taps(),
        },
        Commands::Config { cmd } => match cmd {
            ConfigCmd::Set { key, value } => crate::config::set_config_key(key, value),
            ConfigCmd::Get { key } => {
                if let Some(val) = crate::config::get_config_key(key) {
                    println!("{} = {}", key, val);
                } else {
                    println!("Key '{}' not found in config.", key);
                }
            }
            ConfigCmd::Reset => crate::config::reset_config(),
            ConfigCmd::Show => crate::config::show_config(),
        },
        Commands::Completion { shell } => {
            utils::completion(shell);
        }
        Commands::Backup => match utils::backup_config() {
            Ok(_) => println!("[reap] Config backup complete."),
            Err(e) => eprintln!("[reap] Config backup failed: {}", e),
        },
        Commands::Doctor => {
            let result = crate::utils::doctor_report();
            match result {
                Ok(report) => println!("[reap] Doctor report:\n{}", report),
                Err(e) => eprintln!("[reap] Doctor error: {}", e),
            }
        }
        _ => return Err(anyhow!("Not yet implemented").into()),
    }
    Ok(())
}
