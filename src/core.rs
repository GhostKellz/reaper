use crate::aur;
use crate::backend::{AurBackend, Backend};
use crate::config::Config;
use crate::flatpak;
use crate::hooks::{HookContext, post_install, pre_install};
use crate::pacman;
use crate::tap::{Tap, discover_taps, find_tap_for_pkg};
use crate::tui::LogPane;
use crate::utils;
use anyhow::{Context, Result, bail};
use chrono::Local;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::Semaphore;

/// Custom error type for Reap
#[derive(Debug, Error)]
pub enum ReapError {
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Backup package state before install (files and pacman db)
pub fn backup_package_state(pkg: &str) -> Result<PathBuf> {
    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup_dir = crate::paths::backup_dir().join(pkg).join(&timestamp);
    fs::create_dir_all(&backup_dir)
        .with_context(|| format!("Failed to create backup dir: {}", backup_dir.display()))?;

    // Get list of files owned by package
    let file_list = std::process::Command::new("pacman")
        .args(["-Ql", pkg])
        .output();

    if let Ok(output) = file_list
        && output.status.success()
    {
        // Save file list to backup
        let list_path = backup_dir.join("files.txt");
        let _ = fs::write(&list_path, &output.stdout);
    }

    // Backup pacman db entry
    let db_glob = format!("/var/lib/pacman/local/{}-*", pkg);
    if let Ok(entries) = glob::glob(&db_glob) {
        for entry in entries.flatten() {
            let dest = backup_dir.join(entry.file_name().unwrap_or_default());
            let _ = std::process::Command::new("cp")
                .arg("-r")
                .arg(&entry)
                .arg(&dest)
                .status();
        }
    }

    // Backup /usr/bin/<pkg> if exists
    let bin_path = PathBuf::from(format!("/usr/bin/{}", pkg));
    if bin_path.exists() {
        let _ = std::process::Command::new("cp")
            .arg(&bin_path)
            .arg(&backup_dir)
            .status();
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
    pub source: Source,
}

impl InstallTask {
    pub fn new(pkg: String, source: Source) -> Self {
        Self { pkg, source }
    }
}

/// Options controlling package installation behavior.
///
/// # Secure Defaults
///
/// All boolean options default to `false` (most secure):
/// - `insecure: false` - require GPG verification for tap packages
/// - `strict_signatures: false` - allow untrusted but valid signatures
///
/// Use `InstallOptions::default()` for secure defaults.
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Skip GPG signature verification. **Use with caution.**
    /// When true, allows installation of packages without valid signatures.
    /// Default: false (verification required for taps)
    pub insecure: bool,
    /// Skip some optional safety checks for faster installation (not recommended)
    pub fast_mode: bool,
    /// Require signatures to be from fully trusted keys (not just valid)
    pub strict_signatures: bool,
    /// Maximum parallel operations
    #[allow(dead_code)]
    pub max_parallel: usize,
    /// Install as dependency (--asdeps flag for makepkg)
    pub asdeps: bool,
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
    config: &Config,
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
        if let Ok(out) = output
            && out.status.success()
            && !String::from_utf8_lossy(&out.stdout).trim().is_empty()
        {
            return Some((Source::Flatpak, None, 1, None));
        }
    }
    None
}

/// Install a package using prioritized source resolution and log the decision.
pub async fn install_with_priority(
    pkg: &str,
    config: Arc<Config>,
    _confirm: bool,
    log: Arc<LogPane>,
    opts: &InstallOptions,
) -> Result<()> {
    use owo_colors::OwoColorize;
    let start = Instant::now();

    // Print colorized header
    println!(
        "\n{} Installing package: {}",
        "📦".bright_blue(),
        pkg.bright_white().bold()
    );

    // Step 1: Resolve source FIRST (before hooks, so context is complete)
    if let Some((source, tap_name, prio, tap_obj)) = resolve_package_source(pkg, None, &config) {
        // Transaction recording: capture pre-install state
        let journal = crate::transaction::TransactionJournal::new();
        let mut tx_builder = journal.begin_transaction(
            crate::transaction::TransactionOperation::Install,
            vec![pkg.to_string()],
        );
        let mut pkg_change = crate::transaction::capture_pre_state(pkg, &source);
        // Print source information with colors
        match &source {
            Source::Aur => println!(
                "{} Source: {} (Priority: {})",
                "📍".bright_yellow(),
                "AUR".bright_magenta(),
                prio.to_string().bright_green()
            ),
            Source::Flatpak => println!(
                "{} Source: {} (Priority: {})",
                "📍".bright_yellow(),
                "Flatpak".bright_blue(),
                prio.to_string().bright_green()
            ),
            Source::Pacman => println!(
                "{} Source: {} (Priority: {})",
                "📍".bright_yellow(),
                "Pacman".bright_cyan(),
                prio.to_string().bright_green()
            ),
            Source::Custom(name) => println!(
                "{} Source: {} {} (Priority: {})",
                "📍".bright_yellow(),
                "Tap".bright_purple(),
                name.bright_white(),
                prio.to_string().bright_green()
            ),
            _ => println!(
                "{} Source: {} (Priority: {})",
                "📍".bright_yellow(),
                format!("{:?}", source).bright_white(),
                prio.to_string().bright_green()
            ),
        }

        // Step 2: Create hook context with resolved source info
        let ctx = HookContext {
            pkg: pkg.to_string(),
            version: None,
            source: Some(format!("{:?}", source)),
            install_path: None,
            tap: tap_name.clone(),
        };

        // Step 3: Run pre-install hooks (after source resolution)
        println!("{} Running pre-install hooks...", "🔧".bright_cyan());
        log.push(&format!("{} pre_install executing for {}", "🔧", pkg));
        pre_install(&ctx);

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
                        // Check actual GPG signature status - don't trust self-declared 'verified' field
                        let sig_result = if sig_path.exists() && pkgb_path.exists() {
                            Some(crate::gpg::verify_signature(&sig_path, &pkgb_path))
                        } else {
                            None
                        };
                        let sig_verified = sig_result
                            .as_ref()
                            .map(|result| result.is_valid())
                            .unwrap_or(false);
                        let verified_str = if sig_verified {
                            "[✓ GPG Verified]".green().to_string()
                        } else if pubinfo.verified {
                            // Self-declared but not cryptographically verified - warn user
                            "[⚠ Self-Declared]".yellow().to_string()
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
                            let keyserver = "hkps://keys.openpgp.org";
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
                            let verify = crate::gpg::verify_signature(&sig_path, &pkgb_path);
                            if verify.is_valid() {
                                if opts.strict_signatures && !verify.is_trusted() {
                                    log.push(&format!(
                                        "{} PKGBUILD signature is valid but not fully trusted (key: {})",
                                        "❌".red(),
                                        keyid
                                    ));
                                    if !opts.insecure {
                                        log.push(&format!(
                                            "{} Aborting install. Use --insecure to override.",
                                            "✋".red()
                                        ));
                                        bail!(
                                            "PKGBUILD signature for {} is valid but not fully trusted",
                                            pkg
                                        );
                                    }
                                } else {
                                    log.push(&format!(
                                        "{} PKGBUILD signature verified",
                                        "✓".green()
                                    ));
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
                                    bail!("PKGBUILD signature verification failed for {}", pkg);
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
                                bail!("PKGBUILD signature is missing for {}", pkg);
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
                            bail!("tap publisher is not verified for {}", pkg);
                        }
                    }
                }
                // ...proceed with install if verified or --insecure...
            }
            Source::Pacman => {
                log.push(&format!("[reap][pacman] Installing {} from repo", pkg));
                pacman::install_result(pkg, true)?;
                log.push(&format!("[✓] Installed {} from Pacman", pkg));
            }
            Source::Aur => {
                println!(
                    "{} Building {} from AUR source...",
                    "🔨".bright_yellow(),
                    pkg.bright_white()
                );
                log.push(&format!("[reap][aur] Installing {} from AUR", pkg));

                // Capture installed packages before build to detect split packages
                let installed_before = crate::pacman::list_installed_names();

                let aur_opts = InstallOptions {
                    insecure: opts.insecure,
                    fast_mode: opts.fast_mode,
                    strict_signatures: opts.strict_signatures,
                    max_parallel: 4,
                    asdeps: opts.asdeps,
                };
                match install_aur_native(pkg, &log, &aur_opts).await {
                    Ok(()) => {
                        println!(
                            "{} Successfully installed {} from AUR!",
                            "✅".bright_green(),
                            pkg.bright_white().bold()
                        );
                        log.push(&format!("[✓] Installed {} from AUR", pkg));

                        // Detect split packages installed during build
                        let installed_after = crate::pacman::list_installed_names();
                        for new_pkg in installed_after.difference(&installed_before) {
                            if new_pkg != pkg {
                                // Add split package to transaction (it's a fresh install)
                                let split_change = crate::transaction::PackageChange {
                                    name: new_pkg.clone(),
                                    source: Source::Aur,
                                    previous_version: None, // Fresh install
                                    new_version: crate::pacman::get_version(new_pkg),
                                    previous_artifact: None,
                                    new_artifact: None,
                                    change_type:
                                        crate::transaction::PackageChangeType::DependencyInstall,
                                };
                                tx_builder.add_package_change(split_change);
                                log.push(&format!(
                                    "[transaction] Added split package: {}",
                                    new_pkg
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed to install {} from AUR: {}",
                            "❌".bright_red(),
                            pkg.bright_white(),
                            e
                        );
                        log.push(&format!("[✗] Failed to install {} from AUR: {}", pkg, e));
                        bail!("failed to install {} from AUR: {}", pkg, e);
                    }
                }
            }
            Source::Flatpak => {
                log.push(&format!("[reap][flatpak] Installing {} from Flatpak", pkg));
                match flatpak::install(pkg) {
                    Ok(()) => {
                        log.push(&format!("[✓] Installed {} from Flatpak", pkg));
                    }
                    Err(e) => {
                        eprintln!("[flatpak] Failed to install {}: {}", pkg, e);
                        log.push(&format!(
                            "[✗] Failed to install {} from Flatpak: {}",
                            pkg, e
                        ));
                        bail!("failed to install {} from Flatpak: {}", pkg, e);
                    }
                }
            }
            _ => {
                log.push(&format!("[!] Unknown source for {}", pkg));
                bail!("unknown source for {}", pkg);
            }
        }

        // Transaction recording: capture post-install state and save
        crate::transaction::capture_post_state(&mut pkg_change);
        tx_builder.add_package_change(pkg_change);
        let record = tx_builder.complete();
        if let Err(e) = journal.save_transaction(&record) {
            log.push(&format!("[transaction] Warning: Failed to save: {}", e));
        } else {
            log.push(&format!("[transaction] Recorded: {}", record.id));
        }

        println!("{} Running post-install hooks...", "🔧".bright_cyan());
        log.push(&format!("[reap][hook] post_install executing for {}", pkg));
        post_install(&ctx);

        let elapsed = start.elapsed();
        println!(
            "\n{} Installation completed in {:.2}s",
            "⏱️".bright_blue(),
            elapsed.as_secs_f64().to_string().bright_green()
        );
        log.push(&format!(
            "[reap][timing] install_with_priority for {} took: {:?}",
            pkg, elapsed
        ));
        Ok(())
    } else {
        eprintln!(
            "{} Could not resolve source for '{}'",
            "❌".bright_red(),
            pkg.bright_white()
        );
        eprintln!(
            "{} Package not found in AUR, Pacman repos, or configured taps",
            "ℹ️".bright_blue()
        );
        log.push(&format!(
            "[reap][error] Could not resolve source for '{}'",
            pkg
        ));
        // Note: No rollback needed here - nothing was installed
        bail!(
            "package '{}' not found in AUR, Pacman repos, or configured taps",
            pkg
        );
    }
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
    let flatpak_fut = async { flatpak::search(query).unwrap_or_else(|_| vec![]) };
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

// === Bulk Install Logic ===
pub async fn parallel_install(
    pkgs: &[String],
    config: Arc<Config>,
    log: Arc<LogPane>,
) -> Vec<String> {
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
            let _permit = match permit_fut.await {
                Ok(permit) => permit,
                Err(_) => {
                    eprintln!("[batch] Could not acquire install slot for {}", pkg);
                    return Some(pkg);
                }
            };
            match install_with_priority(&pkg, config, true, log, &InstallOptions::default()).await {
                Ok(()) => None,
                Err(e) => {
                    eprintln!("[batch] Failed to install {}: {}", pkg, e);
                    Some(pkg)
                }
            }
        }));
    }
    join_all(tasks)
        .await
        .into_iter()
        .filter_map(|result| match result {
            Ok(Some(pkg)) => Some(pkg),
            Ok(None) => None,
            Err(e) => Some(format!("<task failed: {}>", e)),
        })
        .collect()
}

pub async fn parallel_upgrade(
    pkgs: &[String],
    config: Arc<Config>,
    log: Arc<LogPane>,
) -> Vec<String> {
    let mut tasks = Vec::new();
    for pkg in pkgs {
        let config = Arc::clone(&config);
        let log = Arc::clone(&log);
        let pkg = pkg.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) =
                install_with_priority(&pkg, config, true, log, &InstallOptions::default()).await
            {
                eprintln!("[upgrade] Failed to upgrade {}: {}", pkg, e);
                Some(pkg)
            } else {
                None
            }
        }));
    }
    let failures: Vec<String> = join_all(tasks)
        .await
        .into_iter()
        .filter_map(|result| match result {
            Ok(Some(pkg)) => Some(pkg),
            Ok(None) => None,
            Err(e) => Some(format!("<task failed: {}>", e)),
        })
        .collect();
    log.push("[reap] All upgrades complete.");
    failures
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
        if let Some(repo) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']'))
            && (repo.ends_with("-aur") || repo == "chaotic-aur" || repo == "ghostctl-aur")
        {
            repos.push(repo.to_string());
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
        if let Ok(out) = output
            && out.status.success()
            && !String::from_utf8_lossy(&out.stdout).trim().is_empty()
        {
            return Some(Source::Flatpak);
        }
    }
    None
}

#[allow(dead_code)]
/// Build a Tokio runtime, reporting failure instead of panicking.
///
/// Returns `None` if the runtime cannot be created (e.g. resource
/// exhaustion), letting callers bail out cleanly rather than crash.
fn new_runtime() -> Option<tokio::runtime::Runtime> {
    match tokio::runtime::Runtime::new() {
        Ok(rt) => Some(rt),
        Err(e) => {
            eprintln!("[reap] Failed to start async runtime: {e}");
            None
        }
    }
}

pub fn handle_install(pkgs: Vec<String>) {
    let backend: Box<dyn Backend> = Box::new(AurBackend::new());
    for pkg in pkgs {
        println!("[reap] Installing {}...", pkg);
        let Some(rt) = new_runtime() else { return };
        rt.block_on(backend.install(&pkg));
    }
}

pub fn handle_removal(pkgs: &[String]) -> bool {
    // Transaction recording
    let journal = crate::transaction::TransactionJournal::new();
    let mut tx_builder = journal.begin_transaction(
        crate::transaction::TransactionOperation::Remove,
        pkgs.to_vec(),
    );

    let mut all_ok = true;
    let mut failed: Vec<String> = Vec::new();
    for pkg in pkgs {
        // Capture pre-removal state (version before removal)
        let mut pkg_change = crate::transaction::capture_pre_state(pkg, &Source::Pacman);
        pkg_change.change_type = crate::transaction::PackageChangeType::Remove;

        println!("[reap] Removing {}...", pkg);
        if !aur::uninstall(pkg) {
            all_ok = false;
            failed.push(pkg.clone());
        }

        // After removal, new_version should be None
        pkg_change.new_version = None;
        pkg_change.new_artifact = None;
        tx_builder.add_package_change(pkg_change);
    }

    // Classify the transaction based on the actual removal outcome.
    let record = if all_ok {
        tx_builder.complete()
    } else {
        tx_builder.fail(format!("failed to remove: {}", failed.join(", ")))
    };
    if let Err(e) = journal.save_transaction(&record) {
        eprintln!("[transaction] Warning: Failed to save: {}", e);
    } else {
        println!("[transaction] Recorded removal: {}", record.id);
    }

    all_ok
}

pub fn handle_local_install(pkgs: &[String]) -> bool {
    let mut all_ok = true;
    for pkg in pkgs {
        println!("[reap] Installing local package {}...", pkg);
        if !aur::install_local(pkg) {
            all_ok = false;
        }
    }
    all_ok
}

pub fn handle_search(terms: &[String]) {
    for term in terms {
        println!("[reap] Searching for {}...", term);
        let Some(rt) = new_runtime() else { return };
        // Use unified_search to search across all backends (taps, AUR, flatpak)
        let results = rt.block_on(unified_search(term));
        print_search_results(&results);
    }
}

pub fn handle_update() {
    use owo_colors::OwoColorize;
    println!("{} Checking for package updates...", "🔍".bright_blue());

    let config = crate::config::Config::load();
    let installed = crate::pacman::list_installed_aur();
    let mut updates_available: Vec<(String, String, String)> = Vec::new();

    println!(
        "{} Scanning {} AUR packages...",
        "📦".bright_cyan(),
        installed.len()
    );

    for pkg in installed {
        if config.is_ignored(&pkg) {
            println!(
                "{} Skipping ignored package: {}",
                "⏭️".yellow(),
                pkg.dimmed()
            );
            continue;
        }

        if let Ok(remote) = crate::aur::fetch_package_info(&pkg) {
            let local_ver = crate::pacman::get_version(&pkg);
            if let Some(local) = local_ver
                && local != remote.version
            {
                updates_available.push((pkg.clone(), local, remote.version));
            }
        }
    }

    if updates_available.is_empty() {
        println!("{} All AUR packages are up to date!", "✅".bright_green());
    } else {
        println!(
            "\n{} {} package(s) can be updated:",
            "📋".bright_yellow(),
            updates_available.len().to_string().bright_white()
        );
        for (pkg, local_ver, remote_ver) in &updates_available {
            println!(
                "  {} {} → {}",
                pkg.bright_white(),
                local_ver.red(),
                remote_ver.bright_green()
            );
        }
        println!(
            "\n{} Run {} to upgrade all packages",
            "💡".bright_blue(),
            "reap -Syu".bright_cyan()
        );
    }
}

pub fn handle_sync_db() -> bool {
    use owo_colors::OwoColorize;
    println!("{} Synchronizing package databases...", "🔄".bright_blue());

    let status = std::process::Command::new("sudo")
        .arg("pacman")
        .arg("-Sy")
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("{} Database sync completed", "✅".bright_green());
            true
        }
        Ok(_) => {
            eprintln!("{} Failed to sync database", "❌".bright_red());
            false
        }
        Err(e) => {
            eprintln!("{} Error syncing database: {}", "❌".bright_red(), e);
            false
        }
    }
}

pub fn handle_upgrade_all() -> bool {
    use owo_colors::OwoColorize;
    println!("{} Upgrading all packages...", "🚀".bright_blue());
    let Some(rt) = new_runtime() else {
        return false;
    };
    if let Err(e) = rt.block_on(aur::upgrade_all()) {
        eprintln!("{} Upgrade all failed: {}", "❌".bright_red(), e);
        return false;
    }
    true
}

pub fn handle_clean() -> bool {
    println!("[reap] Cleaning package cache...");
    let status = std::process::Command::new("sudo")
        .arg("pacman")
        .arg("-Sc")
        .arg("--noconfirm")
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("[reap] Cache cleaned successfully");
            true
        }
        Ok(_) => {
            eprintln!("[reap] Failed to clean cache");
            false
        }
        Err(e) => {
            eprintln!("[reap] Error cleaning cache: {}", e);
            false
        }
    }
}

pub fn handle_doctor() {
    println!("[reap] Running system diagnostics...");
    match crate::utils::doctor_report() {
        Ok(report) => println!("[reap] Doctor report:\n{}", report),
        Err(e) => eprintln!("[reap] Doctor error: {}", e),
    }
}

pub fn handle_upgrade(parallel: bool) -> bool {
    let config = crate::config::Config::load();
    let installed = crate::pacman::list_installed_aur();
    let mut to_upgrade: Vec<String> = Vec::new();
    let mut target_versions = std::collections::HashMap::new();
    for pkg in installed {
        if config.is_ignored(&pkg) {
            println!("[reap] Skipping ignored package: {}", pkg);
            continue;
        }
        if let Ok(remote) = crate::aur::fetch_package_info(&pkg) {
            let local_ver = crate::pacman::get_version(&pkg);
            if local_ver.as_deref() != Some(&remote.version) {
                target_versions.insert(pkg.to_string(), remote.version);
                to_upgrade.push(pkg.to_string());
            }
        }
    }
    if to_upgrade.is_empty() {
        println!("[reap] All AUR packages up to date.");
        return true;
    }
    println!("[reap] Upgrading: {:?}", to_upgrade);
    let Some(rt) = new_runtime() else {
        return false;
    };
    let reviews = match rt.block_on(crate::install_plan::create_install_plan(
        &to_upgrade,
        &config,
    )) {
        Ok(plan) => {
            plan.print_summary();
            if plan.is_blocked() {
                eprintln!("[plan] Upgrade plan has unresolved dependencies or conflicts.");
                return false;
            }
            let reviews = crate::pkgbuild_review::review_plan(&plan);
            crate::pkgbuild_review::print_reviews(&reviews);
            // High-confidence infostealer evidence aborts the upgrade. The
            // upgrade path has no --insecure override, so the block is absolute
            // here; users can still inspect with `reap diff <pkg>`.
            if crate::pkgbuild_review::has_infostealer_block(&reviews) {
                let blocked =
                    crate::pkgbuild_review::infostealer_blocked_packages(&reviews).join(", ");
                eprintln!(
                    "[security] ⛔ BLOCKED: high-confidence infostealer behavior in {}; aborting upgrade.",
                    blocked
                );
                eprintln!(
                    "[security] Inspect with `reap diff <pkg>`; install individually with --insecure only if verified safe."
                );
                return false;
            }
            if crate::pkgbuild_review::has_high_risk_review(&reviews)
                && !crate::interactive::InteractiveManager::confirm_action(
                    "High-risk PKGBUILD findings detected. Continue upgrade?",
                    false,
                )
            {
                // User-initiated cancel is not a failure.
                return true;
            }
            let log = crate::tui::LogPane::default();
            let mut dep_opts = InstallOptions {
                asdeps: true,
                ..Default::default()
            };
            for dep in plan.aur_dependency_steps() {
                println!("[plan] Installing AUR dependency {}...", dep.name);
                if let Err(e) = rt.block_on(install_aur_native(&dep.name, &log, &dep_opts)) {
                    eprintln!(
                        "[plan] Failed to install AUR dependency {}: {}",
                        dep.name, e
                    );
                    return false;
                }
                dep_opts.asdeps = true;
            }
            reviews
        }
        Err(e) => {
            eprintln!("[plan] Failed to create upgrade plan: {}", e);
            return false;
        }
    };
    let failures = if parallel {
        let log = Arc::new(crate::tui::LogPane::default());
        rt.block_on(parallel_upgrade(&to_upgrade, Arc::new(config.clone()), log))
    } else {
        // Use native AUR install path instead of legacy aur::install()
        let log = crate::tui::LogPane::default();
        let opts = InstallOptions::default();
        let mut failures = Vec::new();
        for pkg in &to_upgrade {
            println!("[reap] Upgrading {}...", pkg);
            if let Err(e) = rt.block_on(install_aur_native(pkg, &log, &opts)) {
                eprintln!("[reap] Upgrade failed for {}: {}", pkg, e);
                failures.push(pkg.clone());
            }
        }
        failures
    };
    let upgrade_ok = failures.is_empty();
    if !upgrade_ok {
        eprintln!(
            "[reap] {} upgrade(s) failed: {:?}",
            failures.len(),
            failures
        );
    }

    let completed_reviews: Vec<_> = reviews
        .into_iter()
        .filter(|review| {
            target_versions.get(&review.package).is_some_and(|target| {
                crate::pacman::get_version(&review.package).as_ref() == Some(target)
            })
        })
        .collect();
    if let Err(e) = crate::pkgbuild_review::record_review_baselines(&completed_reviews) {
        eprintln!(
            "[review] Warning: failed to record PKGBUILD review state: {}",
            e
        );
    }

    upgrade_ok
}

#[allow(dead_code)] // Retained for future use - CLI uses new transaction-based rollback
pub fn handle_rollback(pkg: &str) {
    // Restore or remove utils::rollback and hooks::on_rollback
    if let Some(rollback_fn) = std::option::Option::Some(utils::rollback) {
        rollback_fn(pkg);
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
    // Diff against the last accepted review baseline (not a transient build
    // dir), and include the `.install` hook plus an audit summary.
    crate::pkgbuild_review::render_diff(pkg);
}

/// Maps a raw makepkg output line to a concise build-phase label.
///
/// makepkg emits top-level progress markers prefixed with `==>`; this surfaces
/// the meaningful ones (sources, deps, build, fakeroot, packaging) so the live
/// install path can report real progress instead of streaming raw lines only.
fn makepkg_phase(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("==>")?.trim().to_ascii_lowercase();
    if rest.starts_with("making package") {
        Some("Starting build")
    } else if rest.starts_with("retrieving sources") {
        Some("Retrieving sources")
    } else if rest.starts_with("extracting sources") {
        Some("Extracting sources")
    } else if rest.starts_with("checking") {
        Some("Checking dependencies")
    } else if rest.starts_with("starting prepare") {
        Some("Preparing")
    } else if rest.starts_with("starting build") {
        Some("Building")
    } else if rest.starts_with("starting check") {
        Some("Running tests")
    } else if rest.starts_with("entering fakeroot") {
        Some("Packaging (fakeroot)")
    } else if rest.starts_with("tidying install") {
        Some("Tidying install")
    } else if rest.starts_with("creating package") {
        Some("Creating package")
    } else if rest.starts_with("compressing package") {
        Some("Compressing package")
    } else {
        None
    }
}

pub async fn install_aur_native(
    pkg: &str,
    log: &LogPane,
    opts: &InstallOptions,
) -> Result<(), ReapError> {
    use chrono::Local;
    use std::fs;
    use std::process::{Command, Stdio};
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let build_dir = cache_dir.join(format!("reap-aur-{}-{}", pkg, now));
    let repo_url = format!("https://aur.archlinux.org/{}.git", pkg);
    let log_line = |step: &str, msg: &str| {
        use owo_colors::OwoColorize;
        let entry = format!("[{}][reap][aur][{}] {}", now, step, msg);
        log.push(&entry);
        // Also print colorized output to console
        match step {
            "fetch" => println!("{} {}", "📥".bright_blue(), msg.bright_white()),
            "phase" => println!("{} {}", "⚙️".bright_magenta(), msg.bright_white().bold()),
            "build" => println!("{} {}", "🔨".bright_yellow(), msg.bright_white()),
            "install" => println!("{} {}", "📦".bright_green(), msg.bright_white()),
            "deps" => println!("{} {}", "🔗".bright_cyan(), msg.bright_white()),
            "error" => println!("{} {}", "❌".bright_red(), msg.bright_red()),
            "success" => println!("{} {}", "✅".bright_green(), msg.bright_green()),
            _ => println!("{} {}", "ℹ️".bright_blue(), msg.bright_white()),
        }
    };
    // --- Fetch PKGBUILD ---
    log_line("fetch", &format!("Fetching PKGBUILD for {}", pkg));
    let mut clone_cmd = Command::new("git");
    clone_cmd
        .arg("clone")
        .arg(&repo_url)
        .arg(&build_dir)
        // Abort if the transfer stalls below ~1 KB/s for 30s so a dead or
        // hung mirror cannot block the clone indefinitely.
        .env("GIT_HTTP_LOW_SPEED_LIMIT", "1000")
        .env("GIT_HTTP_LOW_SPEED_TIME", "30")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match clone_cmd.spawn().and_then(|mut child| {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("git clone: failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| std::io::Error::other("git clone: failed to capture stderr"))?;
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
    // NOTE: --insecure flag is handled in install_with_priority() for tap verification.
    // It does NOT trigger edit or dry-run mode here - it only skips signature checks.
    let _ = pkgb_path; // Silence unused warning until PKGBUILD editing is properly implemented
    // --- Build ---
    log_line("build", &format!("Running makepkg for {}", pkg));
    let config = crate::config::Config::load();
    let mut makepkg_cmd = Command::new("makepkg");

    // Use makepkg_flags from config instead of hardcoded values
    for flag in &config.build.makepkg_flags {
        makepkg_cmd.arg(flag);
    }
    // Always add -i for install and --needed to skip reinstalls
    makepkg_cmd.arg("-i").arg("--needed");

    // Add --asdeps if installing as dependency
    if opts.asdeps {
        makepkg_cmd.arg("--asdeps");
    }

    makepkg_cmd
        .current_dir(&build_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match makepkg_cmd.spawn().and_then(|mut child| {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("makepkg: failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| std::io::Error::other("makepkg: failed to capture stderr"))?;
        let mut reader = std::io::BufReader::new(stdout);
        let mut err_reader = std::io::BufReader::new(stderr);
        let mut buf = String::new();
        let mut err_buf = String::new();
        while reader.read_line(&mut buf).unwrap_or(0) > 0 {
            let line = buf.trim_end();
            if let Some(phase) = makepkg_phase(line) {
                log_line("phase", phase);
            }
            log_line("build", line);
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

            // Retain artifact for rollback support (before cleanup)
            if let Some(version) = crate::pacman::get_version(pkg)
                && let Some(retained_path) =
                    crate::transaction::retain_aur_artifact(pkg, &version, &build_dir)
            {
                log_line(
                    "artifact",
                    &format!("Retained artifact at {}", retained_path.display()),
                );
            }
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
// NOTE: install_with_priority_enhanced removed during v0.8.0 consolidation
// All install operations now use the single install_with_priority function

#[cfg(test)]
mod tests {
    use super::makepkg_phase;

    #[test]
    fn makepkg_phase_maps_known_markers() {
        assert_eq!(
            makepkg_phase("==> Retrieving sources..."),
            Some("Retrieving sources")
        );
        assert_eq!(makepkg_phase("==> Starting build()..."), Some("Building"));
        assert_eq!(
            makepkg_phase("==> Entering fakeroot environment..."),
            Some("Packaging (fakeroot)")
        );
        assert_eq!(
            makepkg_phase("==> Compressing package..."),
            Some("Compressing package")
        );
    }

    #[test]
    fn makepkg_phase_ignores_non_markers_and_indents() {
        assert_eq!(makepkg_phase("  -> Found foo.tar.gz"), None);
        assert_eq!(makepkg_phase("regular build output"), None);
        // Leading whitespace before the marker is tolerated.
        assert_eq!(
            makepkg_phase("   ==> Checking runtime dependencies..."),
            Some("Checking dependencies")
        );
    }
}
