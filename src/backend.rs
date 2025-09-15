use crate::aur::SearchResult;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::FutureExt;
use std::error::Error;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

/// Backend trait for all supported package sources.
///
/// - AurBackend: Handles AUR installs via install_aur_native (no yay/paru fallback).
/// - PacmanBackend: Handles official repo installs via pacman CLI, and upgrades.
/// - FlatpakBackend: Handles Flatpak installs/upgrades via flatpak CLI.
/// - TapBackend: Handles install/upgrade of external repos declared via reap tap add (planned).
///
/// Backend selection and prioritization order:
///   1. Local Taps (highest priority, explicit priority field)
///   2. Official Pacman Repos
///   3. AUR (native logic)
///   4. Flatpak (fallback)
///
/// See doc/ARCHITECTURE.md for backend flow details.
#[async_trait]
pub trait Backend: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    #[allow(dead_code)]
    fn is_available(&self) -> bool;
    #[allow(dead_code)]
    async fn search(&self, query: &str) -> Vec<SearchResult>;
    async fn install(&self, package: &str);
    #[allow(dead_code)]
    async fn upgrade(&self);
    async fn audit(&self, package: &str);
    #[allow(dead_code)]
    async fn gpg_check(&self, package: &str);
}

#[allow(dead_code)]
pub fn build_and_install(pkgdir: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
    let status = Command::new("makepkg")
        .arg("-si")
        .arg("--noconfirm")
        .current_dir(pkgdir)
        .status()
        .context("failed to execute makepkg")?;
    if !status.success() {
        return Err("makepkg failed".into());
    }
    Ok(())
}

pub struct AurBackend;
impl AurBackend {
    pub fn new() -> Self {
        AurBackend
    }
}
impl Default for AurBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for AurBackend {
    fn name(&self) -> &'static str {
        "AUR"
    }
    fn is_available(&self) -> bool {
        true // Always available, no yay/paru fallback
    }
    async fn search(&self, query: &str) -> Vec<SearchResult> {
        crate::aur::search(query).await.unwrap_or_default()
    }
    async fn install(&self, package: &str) {
        let log = crate::tui::LogPane::default();
        log.push(&format!(
            "[reap][backend] Installing {} using native AUR logic",
            package
        ));
        // Use Default for InstallOptions
        let opts = crate::core::InstallOptions::default();
        let _ = crate::core::install_aur_native(package, &log, &opts)
            .await
            .context("AUR native install failed");
    }
    async fn upgrade(&self) {
        let _ = crate::aur::upgrade_all().await;
    }
    async fn audit(&self, package: &str) {
        crate::utils::audit_package(package);
    }
    async fn gpg_check(&self, package: &str) {
        crate::gpg::check_key(package).await;
    }
}

pub struct PacmanBackend;
#[async_trait]
impl Backend for PacmanBackend {
    fn name(&self) -> &'static str {
        "Pacman"
    }
    fn is_available(&self) -> bool {
        std::process::Command::new("which")
            .arg("pacman")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    async fn search(&self, query: &str) -> Vec<SearchResult> {
        crate::core::unified_search(query)
            .now_or_never()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.source == crate::core::Source::Pacman)
            .collect()
    }
    async fn install(&self, package: &str) {
        crate::pacman::install(package);
    }
    async fn upgrade(&self) { /* handled in aur::upgrade for now */
    }
    async fn audit(&self, package: &str) {
        println!("[reap] Pacman audit for {} (not implemented)", package);
    }
    async fn gpg_check(&self, package: &str) {
        println!("[reap] Pacman GPG check for {} (not implemented)", package);
    }
}

pub struct FlatpakBackend;

impl FlatpakBackend {
    pub fn new() -> Self {
        FlatpakBackend
    }
}
impl Default for FlatpakBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for FlatpakBackend {
    fn name(&self) -> &'static str {
        "flatpak"
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn search(&self, query: &str) -> Vec<SearchResult> {
        crate::flatpak::search(query)
    }
    async fn install(&self, package: &str) {
        match crate::flatpak::install_flatpak(package).await {
            Ok(_) => println!("[reap][backend] Installed {}", package),
            Err(e) => eprintln!("[reap][backend] Install failed for {}: {:?}", package, e),
        }
    }
    async fn upgrade(&self) {
        match crate::flatpak::upgrade_flatpak().await {
            Ok(_) => println!("[reap][backend] Upgrade all succeeded"),
            Err(e) => eprintln!("[reap][backend] Upgrade all failed: {:?}", e),
        }
    }
    async fn audit(&self, _package: &str) {
        println!("Audit not implemented for Flatpak yet.");
    }
    async fn gpg_check(&self, _package: &str) {
        println!("GPG check not implemented for Flatpak yet.");
    }
}

/// TapBackend: Planned backend for custom binary or remote sources.
#[derive(Default)]
pub struct TapBackend;
impl TapBackend {
    #[allow(dead_code)]
    pub fn new() -> Self {
        TapBackend
    }
    /// Scan ~/.config/reap/taps/*.toml or ~/.local/share/reap/taps/ for registered taps.
    #[allow(dead_code)]
    pub fn discover_taps() -> Vec<(String, String, u32)> {
        let mut taps = Vec::new();
        let config_dir = dirs::config_dir().unwrap_or_default().join("reap/taps");
        if let Ok(entries) = std::fs::read_dir(&config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    if let Ok(toml) = std::fs::read_to_string(&path) {
                        if let Ok(val) = toml::Value::from_str(&toml) {
                            let name = val
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let url = val
                                .get("url")
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string();
                            let priority = val
                                .get("priority")
                                .and_then(|p| p.as_integer())
                                .unwrap_or(50) as u32;
                            if !name.is_empty() && !url.is_empty() {
                                taps.push((name, url, priority));
                            }
                        }
                    }
                }
            }
        }
        taps
    }
    /// Check if a tap contains the requested package (stub: always false for now)
    #[allow(dead_code)]
    pub fn tap_has_package(_tap_url: &str, _pkg: &str) -> bool {
        false // TODO: Implement actual check
    }
}

pub struct AptBackend;
#[async_trait]
impl Backend for AptBackend {
    fn name(&self) -> &'static str {
        "Apt"
    }
    fn is_available(&self) -> bool {
        std::process::Command::new("which")
            .arg("apt")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let output = std::process::Command::new("apt-cache")
            .arg("search")
            .arg(query)
            .output();
        let mut results = Vec::new();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let mut parts = line.splitn(2, ' ');
                if let Some(name) = parts.next() {
                    let desc = parts.next().unwrap_or("").trim().to_string();
                    results.push(SearchResult {
                        name: name.to_string(),
                        version: String::from("?"),
                        description: desc,
                        source: crate::core::Source::Pacman, // TODO: add Debian source
                    });
                }
            }
        }
        results
    }
    async fn install(&self, package: &str) {
        let _ = std::process::Command::new("sudo")
            .arg("apt")
            .arg("install")
            .arg("-y")
            .arg(package)
            .status();
    }
    async fn upgrade(&self) {
        let _ = std::process::Command::new("sudo")
            .arg("apt")
            .arg("update")
            .status();
        let _ = std::process::Command::new("sudo")
            .arg("apt")
            .arg("upgrade")
            .arg("-y")
            .status();
    }
    async fn audit(&self, _package: &str) {
        println!("Audit not implemented for Apt yet.");
    }
    async fn gpg_check(&self, _package: &str) {
        println!("GPG check not implemented for Apt yet.");
    }
}

// Backend selection is now always native for AUR, Flatpak, Pacman, and (future) Tap.
#[allow(dead_code)]
pub enum BackendImpl {
    Aur(AurBackend),
    Flatpak(FlatpakBackend),
    Pacman(PacmanBackend),
    Apt(AptBackend),
}

impl BackendImpl {
    #[allow(dead_code)]
    pub async fn search(&self, query: &str) -> Vec<SearchResult> {
        match self {
            BackendImpl::Aur(b) => b.search(query).await,
            BackendImpl::Flatpak(b) => b.search(query).await,
            BackendImpl::Pacman(b) => b.search(query).await,
            BackendImpl::Apt(b) => b.search(query).await,
        }
    }
    #[allow(dead_code)]
    pub async fn install(&self, pkg: &str) {
        match self {
            BackendImpl::Aur(b) => b.install(pkg).await,
            BackendImpl::Flatpak(b) => b.install(pkg).await,
            BackendImpl::Pacman(b) => b.install(pkg).await,
            BackendImpl::Apt(b) => b.install(pkg).await,
        }
    }
    #[allow(dead_code)]
    pub async fn upgrade(&self) {
        match self {
            BackendImpl::Aur(b) => b.upgrade().await,
            BackendImpl::Flatpak(b) => b.upgrade().await,
            BackendImpl::Pacman(b) => b.upgrade().await,
            BackendImpl::Apt(b) => b.upgrade().await,
        }
    }
    #[allow(dead_code)]
    pub async fn audit(&self, pkg: &str) {
        match self {
            BackendImpl::Aur(b) => b.audit(pkg).await,
            BackendImpl::Flatpak(b) => b.audit(pkg).await,
            BackendImpl::Pacman(b) => b.audit(pkg).await,
            BackendImpl::Apt(b) => b.audit(pkg).await,
        }
    }
    #[allow(dead_code)]
    pub async fn gpg_check(&self, pkg: &str) {
        match self {
            BackendImpl::Aur(b) => b.gpg_check(pkg).await,
            BackendImpl::Flatpak(b) => b.gpg_check(pkg).await,
            BackendImpl::Pacman(b) => b.gpg_check(pkg).await,
            BackendImpl::Apt(b) => b.gpg_check(pkg).await,
        }
    }
}
