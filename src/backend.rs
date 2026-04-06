use crate::aur::SearchResult;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::FutureExt;
use std::error::Error;
use std::path::Path;
use std::process::Command;

/// Backend trait for all supported package sources.
///
/// - AurBackend: Handles AUR installs via install_aur_native (no yay/paru fallback).
/// - PacmanBackend: Handles official repo installs via pacman CLI, and upgrades.
/// - FlatpakBackend: Handles Flatpak installs/upgrades via flatpak CLI.
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
    let config = crate::config::Config::load();
    let mut cmd = Command::new("makepkg");

    // Use makepkg_flags from config instead of hardcoded values
    for flag in &config.build.makepkg_flags {
        cmd.arg(flag);
    }
    // Always add -i for install
    cmd.arg("-i");

    let status = cmd
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
    async fn upgrade(&self) {
        // Pacman upgrades handled via aur::upgrade which includes repo packages
    }
    async fn audit(&self, _package: &str) {
        // Pacman packages are signed by Arch maintainers - rely on pacman's verification
    }
    async fn gpg_check(&self, _package: &str) {
        // Pacman handles GPG verification internally via pacman-key
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
        crate::flatpak::is_available()
    }
    async fn search(&self, query: &str) -> Vec<SearchResult> {
        crate::flatpak::search(query).unwrap_or_default()
    }
    async fn install(&self, package: &str) {
        match crate::flatpak::install(package) {
            Ok(_) => println!("[reap][backend] Installed {}", package),
            Err(e) => eprintln!("[reap][backend] Install failed for {}: {:?}", package, e),
        }
    }
    async fn upgrade(&self) {
        match crate::flatpak::upgrade() {
            Ok(_) => println!("[reap][backend] Upgrade all succeeded"),
            Err(e) => eprintln!("[reap][backend] Upgrade all failed: {:?}", e),
        }
    }
    async fn audit(&self, package: &str) {
        match crate::flatpak::audit(package) {
            Ok(audit) => crate::flatpak::display_audit(&audit),
            Err(e) => eprintln!("[flatpak] Audit failed: {}", e),
        }
    }
    async fn gpg_check(&self, _package: &str) {
        println!("GPG check not applicable for Flatpak (uses different verification).");
    }
}

// Backend selection is now always native for AUR, Flatpak, and Pacman.
#[allow(dead_code)]
pub enum BackendImpl {
    Aur(AurBackend),
    Flatpak(FlatpakBackend),
    Pacman(PacmanBackend),
}

impl BackendImpl {
    #[allow(dead_code)]
    pub async fn search(&self, query: &str) -> Vec<SearchResult> {
        match self {
            BackendImpl::Aur(b) => b.search(query).await,
            BackendImpl::Flatpak(b) => b.search(query).await,
            BackendImpl::Pacman(b) => b.search(query).await,
        }
    }
    #[allow(dead_code)]
    pub async fn install(&self, pkg: &str) {
        match self {
            BackendImpl::Aur(b) => b.install(pkg).await,
            BackendImpl::Flatpak(b) => b.install(pkg).await,
            BackendImpl::Pacman(b) => b.install(pkg).await,
        }
    }
    #[allow(dead_code)]
    pub async fn upgrade(&self) {
        match self {
            BackendImpl::Aur(b) => b.upgrade().await,
            BackendImpl::Flatpak(b) => b.upgrade().await,
            BackendImpl::Pacman(b) => b.upgrade().await,
        }
    }
    #[allow(dead_code)]
    pub async fn audit(&self, pkg: &str) {
        match self {
            BackendImpl::Aur(b) => b.audit(pkg).await,
            BackendImpl::Flatpak(b) => b.audit(pkg).await,
            BackendImpl::Pacman(b) => b.audit(pkg).await,
        }
    }
    #[allow(dead_code)]
    pub async fn gpg_check(&self, pkg: &str) {
        match self {
            BackendImpl::Aur(b) => b.gpg_check(pkg).await,
            BackendImpl::Flatpak(b) => b.gpg_check(pkg).await,
            BackendImpl::Pacman(b) => b.gpg_check(pkg).await,
        }
    }
}
