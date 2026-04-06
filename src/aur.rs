use crate::utils;
use anyhow::Result;
use futures::future::join_all;
use owo_colors::OwoColorize;
use reqwest::Client; // Use async client for parallel downloads
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::time::timeout;

// Keep blocking client for synchronous functions
use reqwest::blocking::Client as BlockingClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: crate::core::Source,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AurResult {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Description")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AurResponse {
    pub results: Vec<AurResult>,
}

pub struct AurInfo {
    pub version: String,
}

/// Fetch package info from AUR
///
/// # Errors
///
/// Returns an error if the request to the AUR fails or if the package is not found.
pub fn fetch_package_info(pkg: &str) -> Result<AurInfo, Box<dyn Error + Send + Sync>> {
    // Check in-memory cache first
    if let Some(cached) = crate::aur_cache::get_info(pkg) {
        return Ok(AurInfo {
            version: cached.version,
        });
    }

    let url = format!("https://aur.archlinux.org/rpc/?v=5&type=info&arg[]={}", pkg);
    let client = BlockingClient::new();
    let resp = client.get(&url).send()?;
    let aur_resp: AurResponse = resp.json()?;
    if let Some(r) = aur_resp.results.into_iter().next() {
        // Cache the result
        crate::aur_cache::put_info(
            pkg,
            AurResult {
                name: pkg.to_string(),
                version: r.version.clone(),
                description: r.description,
            },
        );
        Ok(AurInfo { version: r.version })
    } else {
        Err("Package not found".into())
    }
}

/// Search for a package in AUR
///
/// # Errors
///
/// Returns an error if the request to the AUR fails.
pub async fn search(query: &str) -> Result<Vec<SearchResult>, Box<dyn Error + Send + Sync>> {
    // Check in-memory cache first (fastest)
    if let Some(cached) = crate::aur_cache::get_search(query) {
        return Ok(cached);
    }

    // Check file cache (still faster than network)
    #[cfg(feature = "cache")]
    if let Some(cached) = crate::utils::get_cached_search(query) {
        // Promote to in-memory cache
        crate::aur_cache::put_search(query, cached.clone());
        return Ok(cached);
    }

    let url = format!(
        "https://aur.archlinux.org/rpc/?v=5&type=search&arg={}",
        query
    );
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;
    let aur_resp: AurResponse = resp.json().await?;
    let results: Vec<SearchResult> = aur_resp
        .results
        .into_iter()
        .map(|r| SearchResult {
            name: r.name,
            version: r.version,
            description: r.description.unwrap_or_default(),
            source: crate::core::Source::Aur,
        })
        .collect();

    // Cache in both layers
    crate::aur_cache::put_search(query, results.clone());
    #[cfg(feature = "cache")]
    crate::utils::cache_search_result(query, &results);

    Ok(results)
}

/// Get AUR search results (blocking)
pub fn aur_search_results(query: &str) -> Vec<AurResult> {
    // Check in-memory cache first (convert SearchResult back to AurResult)
    if let Some(cached) = crate::aur_cache::get_search(query) {
        return cached
            .into_iter()
            .map(|r| AurResult {
                name: r.name,
                version: r.version,
                description: Some(r.description),
            })
            .collect();
    }

    let url = format!(
        "https://aur.archlinux.org/rpc/?v=5&type=search&arg={}",
        query
    );
    if let Ok(resp) = reqwest::blocking::get(&url)
        && let Ok(json) = resp.json::<AurResponse>()
    {
        // Cache the results
        let search_results: Vec<SearchResult> = json
            .results
            .iter()
            .map(|r| SearchResult {
                name: r.name.clone(),
                version: r.version.clone(),
                description: r.description.clone().unwrap_or_default(),
                source: crate::core::Source::Aur,
            })
            .collect();
        crate::aur_cache::put_search(query, search_results);
        return json.results;
    }
    vec![]
}

#[cfg(feature = "cache")]
#[allow(dead_code)]
pub async fn get_pkgbuild_cached(pkg: &str) -> String {
    crate::utils::async_get_pkgbuild_cached(pkg).await
}

#[cfg(not(feature = "cache"))]
#[allow(dead_code)]
pub async fn get_pkgbuild_cached(pkg: &str) -> String {
    let url = format!(
        "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}",
        pkg
    );
    match reqwest::get(&url).await {
        Ok(resp) => resp.text().await.unwrap_or_default(),
        Err(_) => String::from("[reap] PKGBUILD not found."),
    }
}

/// Install packages using native AUR logic (no yay/paru fallback)
///
/// # Errors
///
/// Returns an error if the installation fails.
pub async fn install(pkgs: Vec<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("[reap] Installing packages: {:?} (native AUR)...", pkgs);
    let log = crate::tui::LogPane::default();
    let opts = crate::core::InstallOptions::default();

    for pkg in pkgs {
        println!("[reap] Installing {} via native AUR...", pkg.yellow());
        if let Err(e) = crate::core::install_aur_native(pkg, &log, &opts).await {
            eprintln!("[reap] Install failed for {}: {}", pkg.red(), e);
        } else {
            println!("[reap] Installed {}.", pkg.green());
        }
    }
    Ok(())
}

/// Uninstall a package using pacman directly (no yay/paru fallback)
pub fn uninstall(package: &str) {
    println!(
        "[reap] Uninstalling {} (sudo pacman -R)...",
        package.yellow()
    );
    let status = Command::new("sudo")
        .args(["pacman", "-R", package])
        .status();
    match status {
        Ok(s) if s.success() => println!("[reap] Uninstalled {}.", package.green()),
        Ok(_) => eprintln!("[reap] Uninstall failed for {}.", package.red()),
        Err(e) => eprintln!("[reap] Failed to run pacman -R: {}", e),
    }
}

/// Get PKGBUILD preview
pub fn get_pkgbuild_preview(pkg: &str) -> String {
    // Check in-memory cache first
    if let Some(cached) = crate::aur_cache::get_pkgbuild(pkg) {
        return cached;
    }

    // Check file cache
    #[cfg(feature = "cache")]
    if let Some(cached) = crate::utils::cache::load_pkgbuild(pkg) {
        crate::aur_cache::put_pkgbuild(pkg, cached.clone());
        return cached;
    }

    let url = format!(
        "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}",
        pkg
    );
    if let Ok(resp) = reqwest::blocking::get(&url)
        && let Ok(text) = resp.text()
    {
        // Cache in both layers
        crate::aur_cache::put_pkgbuild(pkg, text.clone());
        #[cfg(feature = "cache")]
        crate::utils::cache::save_pkgbuild(pkg, &text);
        return text;
    }
    String::from("[reap] PKGBUILD not found.")
}

/// Extract dependencies from PKGBUILD
pub fn get_deps(pkgb: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_dep = false;
    let mut dep_buf = String::new();
    for line in pkgb.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("depends=") {
            in_dep = true;
            dep_buf.push_str(trimmed.split_once('=').map(|x| x.1).unwrap_or("").trim());
            if trimmed.ends_with(')') {
                in_dep = false;
            }
        } else if in_dep {
            dep_buf.push_str(trimmed);
            if trimmed.ends_with(')') {
                in_dep = false;
            }
        }
        if !in_dep && !dep_buf.is_empty() {
            let dep_line = dep_buf.trim_matches(&['(', ')', '"', '\'', ' '] as &[_]);
            deps.extend(
                dep_line
                    .split_whitespace()
                    .map(|s| s.trim_matches(&['"', '\'', ' '] as &[_]))
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            );
            dep_buf.clear();
        }
    }
    deps
}

/// Upgrade all packages
//
// # Errors
//
// Returns an error if the upgrade fails.
pub async fn upgrade_all() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let outdated = get_outdated();
    if outdated.is_empty() {
        println!("[reap] All packages are up to date.");
        return Ok(());
    }
    println!("[reap] Outdated packages:");
    for pkg in &outdated {
        println!("  - {}", pkg);
    }
    let mut to_upgrade = Vec::new();
    for pkg in &outdated {
        if utils::is_pinned(pkg) {
            println!("[reap] Skipping pinned package: {}", pkg);
            continue;
        }
        to_upgrade.push(pkg.as_str());
    }
    if to_upgrade.is_empty() {
        println!("[reap] No packages to upgrade (all pinned).");
        return Ok(());
    }

    // Transaction recording: capture pre-upgrade state
    let journal = crate::transaction::TransactionJournal::new();
    let mut tx_builder = journal.begin_transaction(
        crate::transaction::TransactionOperation::UpgradeAll,
        to_upgrade.iter().map(|s| s.to_string()).collect(),
    );

    // Capture pre-state for all packages
    let mut pkg_changes: Vec<crate::transaction::PackageChange> = to_upgrade
        .iter()
        .map(|pkg| crate::transaction::capture_pre_state(pkg, &crate::core::Source::Aur))
        .collect();

    println!("[reap] Upgrading {} packages...", to_upgrade.len());
    let res = install(to_upgrade).await;

    // Capture post-state for all packages
    for change in &mut pkg_changes {
        crate::transaction::capture_post_state(change);
        tx_builder.add_package_change(change.clone());
    }

    // Save the transaction
    let record = tx_builder.complete();
    if let Err(e) = journal.save_transaction(&record) {
        eprintln!("[transaction] Warning: Failed to save: {}", e);
    } else {
        println!("[transaction] Recorded upgrade: {}", record.id);
    }

    match res {
        Ok(_) => println!("[reap] Upgrade complete."),
        Err(e) => eprintln!("[reap] Upgrade failed: {}", e),
    }
    Ok(())
}

/// Install a local package
pub fn install_local(path: &str) {
    use std::path::Path;
    let file = Path::new(path);
    if !file.exists() {
        eprintln!("[reap] Local package file does not exist: {}", path.red());
        return;
    }
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !(ext == "zst" || path.ends_with(".pkg.tar.zst")) {
        println!(
            "[reap] Local package file must be a .zst or .pkg.tar.zst: {}",
            path.yellow()
        );
        return;
    }
    println!(
        "[reap] Installing local package from {} (sudo pacman -U)...",
        path.yellow()
    );
    let status = Command::new("sudo")
        .arg("pacman")
        .arg("-U")
        .arg(path)
        .status()
        .expect("Failed to run sudo pacman -U <file>");
    if status.success() {
        println!("[reap] Local install complete: {}.", path.green());
    } else {
        eprintln!("[reap] Local install failed: {}.", path.red());
    }
}

// Get a list of outdated packages using proper Arch version comparison
pub fn get_outdated() -> Vec<String> {
    use crate::aur::fetch_package_info;
    use crate::pacman;
    use crate::version::is_older;

    let installed = pacman::list_installed_aur();
    let mut outdated = Vec::new();
    for pkg in installed {
        let local_ver = pacman::get_version(&pkg);
        if let Ok(remote) = fetch_package_info(&pkg)
            && let Some(local_ver) = local_ver
            && is_older(&local_ver, &remote.version)
        {
            outdated.push(pkg);
        }
    }
    outdated
}

/// Parallel search across multiple queries with smart caching
pub async fn parallel_search(
    queries: &[String],
) -> Result<Vec<SearchResult>, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();
    println!(
        "[aur] Starting parallel search for {} queries",
        queries.len()
    );

    let client = Client::new();
    let tasks: Vec<_> = queries
        .iter()
        .map(|query| {
            let client = client.clone();
            let query = query.clone();

            async move {
                // Check cache first
                #[cfg(feature = "cache")]
                if let Some(cached) = utils::get_cached_search(&query) {
                    println!("[aur] Cache hit for query: {}", query);
                    return Ok(cached);
                }

                // Fetch from AUR with timeout
                let url = format!(
                    "https://aur.archlinux.org/rpc/?v=5&type=search&arg={}",
                    query
                );
                let response = timeout(Duration::from_secs(10), client.get(&url).send()).await??;
                let aur_resp: AurResponse = response.json().await?;

                let results: Vec<SearchResult> = aur_resp
                    .results
                    .into_iter()
                    .map(|r| SearchResult {
                        name: r.name,
                        version: r.version,
                        description: r.description.unwrap_or_default(),
                        source: crate::core::Source::Aur,
                    })
                    .collect();

                // Cache the results
                #[cfg(feature = "cache")]
                utils::cache_search_result(&query, &results);

                println!(
                    "[aur] Fetched {} results for query: {}",
                    results.len(),
                    query
                );
                Ok(results)
            }
        })
        .collect();

    let results: Vec<Result<Vec<SearchResult>, Box<dyn Error + Send + Sync>>> =
        join_all(tasks).await;
    let mut all_results = Vec::new();

    for result in results {
        match result {
            Ok(search_results) => all_results.extend(search_results),
            Err(e) => eprintln!("[aur] Search error: {}", e),
        }
    }

    let elapsed = start.elapsed();
    println!(
        "[aur] Parallel search completed in {:?}, found {} total results",
        elapsed,
        all_results.len()
    );

    Ok(all_results)
}

/// Parallel PKGBUILD downloads for multiple packages
pub async fn parallel_pkgbuild_fetch(
    packages: &[String],
) -> Result<Vec<(String, String)>, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();
    println!(
        "[aur] Starting parallel PKGBUILD fetch for {} packages",
        packages.len()
    );

    let client = Client::new();
    let tasks: Vec<_> = packages
        .iter()
        .map(|pkg| {
            let client = client.clone();
            let pkg = pkg.clone();

            async move {
                // Check cache first
                #[cfg(feature = "cache")]
                if let Some(cached) = utils::cache::load_pkgbuild(&pkg) {
                    println!("[aur] PKGBUILD cache hit for: {}", pkg);
                    return Ok((pkg.clone(), cached));
                }

                // Fetch PKGBUILD from AUR
                let url = format!(
                    "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}",
                    pkg
                );
                let response = timeout(Duration::from_secs(15), client.get(&url).send()).await??;

                if response.status().is_success() {
                    let pkgbuild = response.text().await?;

                    // Cache the PKGBUILD
                    #[cfg(feature = "cache")]
                    utils::cache::save_pkgbuild(&pkg, &pkgbuild);

                    println!("[aur] Downloaded PKGBUILD for: {}", pkg);
                    Ok((pkg, pkgbuild))
                } else {
                    Err(format!(
                        "Failed to fetch PKGBUILD for {}: HTTP {}",
                        pkg,
                        response.status()
                    )
                    .into())
                }
            }
        })
        .collect();

    let results: Vec<Result<(String, String), Box<dyn Error + Send + Sync>>> =
        join_all(tasks).await;
    let mut successful_downloads = Vec::new();

    for result in results {
        match result {
            Ok(download) => successful_downloads.push(download),
            Err(e) => eprintln!("[aur] PKGBUILD fetch error: {}", e),
        }
    }

    let elapsed = start.elapsed();
    println!(
        "[aur] Parallel PKGBUILD fetch completed in {:?}, {} successful downloads",
        elapsed,
        successful_downloads.len()
    );

    Ok(successful_downloads)
}

/// Smart cache warming - preload popular packages
pub async fn warm_cache() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("[aur] Warming cache with popular packages...");

    let popular_packages = vec![
        "yay".to_string(),
        "visual-studio-code-bin".to_string(),
        "google-chrome".to_string(),
        "discord".to_string(),
        "spotify".to_string(),
        "slack-desktop".to_string(),
        "zoom".to_string(),
        "firefox".to_string(),
        "brave-bin".to_string(),
        "telegram-desktop".to_string(),
    ];

    let _ = parallel_search(&popular_packages).await?;
    let _ = parallel_pkgbuild_fetch(&popular_packages).await?;

    println!("[aur] Cache warming completed");
    Ok(())
}
