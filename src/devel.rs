use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelInfo {
    pub packages: HashMap<String, DevelPackage>,
    pub last_update: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelPackage {
    pub name: String,
    pub vcs_type: VcsType,
    pub url: String,
    pub current_commit: Option<String>,
    pub current_tag: Option<String>,
    pub last_checked: DateTime<Utc>,
    pub install_version: String,
    pub build_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VcsType {
    Git,
    Svn,
    Hg,
    Bzr,
    Unknown,
}

impl VcsType {
    pub fn from_url(url: &str) -> Self {
        if url.contains(".git") || url.starts_with("git://") || url.starts_with("git+") {
            Self::Git
        } else if url.contains("svn") || url.starts_with("svn://") {
            Self::Svn
        } else if url.contains("hg") || url.starts_with("hg+") {
            Self::Hg
        } else if url.contains("bzr") || url.starts_with("bzr+") {
            Self::Bzr
        } else {
            Self::Unknown
        }
    }

    pub fn command(&self) -> &str {
        match self {
            Self::Git => "git",
            Self::Svn => "svn",
            Self::Hg => "hg",
            Self::Bzr => "bzr",
            Self::Unknown => "",
        }
    }
}

pub struct DevelManager {
    info_file: PathBuf,
    cache_dir: PathBuf,
}

impl DevelManager {
    pub fn new() -> Result<Self> {
        let cache_dir = crate::paths::devel_dir();

        fs::create_dir_all(&cache_dir)?;

        let info_file = cache_dir.join("devel_info.json");

        Ok(Self {
            info_file,
            cache_dir,
        })
    }

    pub fn load_devel_info(&self) -> Result<DevelInfo> {
        if self.info_file.exists() {
            let content = fs::read_to_string(&self.info_file)?;
            let info: DevelInfo = serde_json::from_str(&content)?;
            Ok(info)
        } else {
            Ok(DevelInfo {
                packages: HashMap::new(),
                last_update: Utc::now(),
            })
        }
    }

    pub fn save_devel_info(&self, info: &DevelInfo) -> Result<()> {
        let content = serde_json::to_string_pretty(info)?;
        fs::write(&self.info_file, content)?;
        Ok(())
    }

    pub fn add_devel_package(
        &mut self,
        name: &str,
        pkgbuild_path: &Path,
        install_version: &str,
    ) -> Result<()> {
        let mut info = self.load_devel_info()?;

        if let Some(vcs_info) = self.extract_vcs_info(pkgbuild_path)? {
            let build_dir = self.cache_dir.join(name);
            fs::create_dir_all(&build_dir)?;

            let devel_pkg = DevelPackage {
                name: name.to_string(),
                vcs_type: vcs_info.0,
                url: vcs_info.1,
                current_commit: None,
                current_tag: None,
                last_checked: Utc::now(),
                install_version: install_version.to_string(),
                build_dir,
            };

            info.packages.insert(name.to_string(), devel_pkg);
            self.save_devel_info(&info)?;

            println!("[devel] Added {} to development package tracking", name);
        }

        Ok(())
    }

    pub fn check_updates(&mut self) -> Result<Vec<String>> {
        let mut info = self.load_devel_info()?;
        let mut updated_packages = Vec::new();

        for (name, pkg) in info.packages.iter_mut() {
            match self.check_package_update(pkg) {
                Ok(has_update) => {
                    pkg.last_checked = Utc::now();
                    if has_update {
                        updated_packages.push(name.clone());
                        println!("[devel] {} has updates available", name);
                    }
                }
                Err(e) => {
                    eprintln!("[devel] Failed to check updates for {}: {}", name, e);
                }
            }
        }

        info.last_update = Utc::now();
        self.save_devel_info(&info)?;

        Ok(updated_packages)
    }

    pub fn get_outdated_packages(&self, max_age_hours: u64) -> Result<Vec<String>> {
        let info = self.load_devel_info()?;
        let mut outdated = Vec::new();
        let now = Utc::now();

        for (name, pkg) in &info.packages {
            let hours_since_check = now.signed_duration_since(pkg.last_checked).num_hours() as u64;

            if hours_since_check > max_age_hours {
                outdated.push(name.clone());
            }
        }

        Ok(outdated)
    }

    pub fn remove_devel_package(&mut self, name: &str) -> Result<()> {
        let mut info = self.load_devel_info()?;

        if let Some(pkg) = info.packages.remove(name) {
            // Clean up build directory
            if pkg.build_dir.exists() {
                fs::remove_dir_all(&pkg.build_dir)?;
            }

            self.save_devel_info(&info)?;
            println!("[devel] Removed {} from development package tracking", name);
        } else {
            return Err(anyhow!(
                "Package {} not found in development tracking",
                name
            ));
        }

        Ok(())
    }

    pub fn list_devel_packages(&self) -> Result<Vec<DevelPackage>> {
        let info = self.load_devel_info()?;
        Ok(info.packages.into_values().collect())
    }

    fn extract_vcs_info(&self, pkgbuild_path: &Path) -> Result<Option<(VcsType, String)>> {
        let content = fs::read_to_string(pkgbuild_path)?;

        // Look for source array in PKGBUILD
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("source")
                && line.contains('=')
                && let Some(sources_part) = line.split('=').nth(1)
            {
                // Extract URLs from source array
                let sources = sources_part.trim_matches(['(', ')', '"', '\'']).trim();
                for source in sources.split_whitespace() {
                    let source = source.trim_matches(['"', '\'']);
                    if self.is_vcs_url(source) {
                        let vcs_type = VcsType::from_url(source);
                        if vcs_type != VcsType::Unknown {
                            return Ok(Some((vcs_type, source.to_string())));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    fn is_vcs_url(&self, url: &str) -> bool {
        url.starts_with("git://")
            || url.starts_with("git+")
            || url.starts_with("svn://")
            || url.starts_with("hg+")
            || url.starts_with("bzr+")
            || url.contains(".git")
            || url.contains("github.com")
            || url.contains("gitlab.com")
            || url.contains("bitbucket.org")
    }

    pub fn check_package_update(&self, pkg: &mut DevelPackage) -> Result<bool> {
        match pkg.vcs_type {
            VcsType::Git => self.check_git_update(pkg),
            VcsType::Svn => self.check_svn_update(pkg),
            VcsType::Hg => self.check_hg_update(pkg),
            VcsType::Bzr => self.check_bzr_update(pkg),
            VcsType::Unknown => {
                eprintln!("[devel] Unknown VCS type for {}", pkg.name);
                Ok(false)
            }
        }
    }

    fn check_git_update(&self, pkg: &mut DevelPackage) -> Result<bool> {
        // First, try to check remote ref without cloning (fast path)
        if let Some(remote_commit) = self.get_git_remote_ref(&pkg.url)? {
            if let Some(ref local_commit) = pkg.current_commit {
                if local_commit != &remote_commit {
                    println!(
                        "[devel] {} has new commits: {} -> {}",
                        pkg.name,
                        &local_commit[..8.min(local_commit.len())],
                        &remote_commit[..8.min(remote_commit.len())]
                    );
                    pkg.current_commit = Some(remote_commit);
                    return Ok(true);
                }
                return Ok(false);
            } else {
                // First time tracking - store current remote ref
                pkg.current_commit = Some(remote_commit);
                return Ok(false);
            }
        }

        // Fallback: clone/fetch if ls-remote fails (e.g., auth required)
        if !pkg.build_dir.join(".git").exists() {
            println!("[devel] Cloning {} repository", pkg.name);
            let output = Command::new("git")
                .args(["clone", "--depth", "1", &pkg.url])
                .arg(&pkg.build_dir)
                .output()?;

            if !output.status.success() {
                return Err(anyhow!(
                    "Failed to clone repository: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        } else {
            // Fetch latest changes
            let output = Command::new("git")
                .args(["fetch", "--depth", "1"])
                .current_dir(&pkg.build_dir)
                .output()?;

            if !output.status.success() {
                return Err(anyhow!(
                    "Failed to fetch repository: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }

        // Get current commit hash
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&pkg.build_dir)
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to get commit hash"));
        }

        let current_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let has_update = if let Some(ref old_commit) = pkg.current_commit {
            old_commit != &current_commit
        } else {
            true // First time checking
        };

        pkg.current_commit = Some(current_commit);
        Ok(has_update)
    }

    /// Get remote HEAD ref without cloning using git ls-remote
    fn get_git_remote_ref(&self, url: &str) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(["ls-remote", "--heads", "--refs", url, "HEAD"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // Format: <hash>\t<ref>
                if let Some(line) = stdout.lines().next()
                    && let Some(hash) = line.split_whitespace().next()
                {
                    return Ok(Some(hash.to_string()));
                }
                // Try without HEAD filter for repos that don't advertise HEAD
                let output2 = Command::new("git").args(["ls-remote", url]).output()?;
                if output2.status.success() {
                    let stdout2 = String::from_utf8_lossy(&output2.stdout);
                    // Look for refs/heads/main or refs/heads/master
                    for line in stdout2.lines() {
                        if (line.contains("refs/heads/main") || line.contains("refs/heads/master"))
                            && let Some(hash) = line.split_whitespace().next()
                        {
                            return Ok(Some(hash.to_string()));
                        }
                    }
                    // Fallback to first ref
                    if let Some(line) = stdout2.lines().next()
                        && let Some(hash) = line.split_whitespace().next()
                    {
                        return Ok(Some(hash.to_string()));
                    }
                }
                Ok(None)
            }
            _ => Ok(None), // ls-remote failed, caller should use fallback
        }
    }

    fn check_svn_update(&self, pkg: &mut DevelPackage) -> Result<bool> {
        // Get remote revision using svn info
        let output = Command::new("svn")
            .args(["info", "--show-item", "revision", &pkg.url])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let remote_rev = String::from_utf8_lossy(&out.stdout).trim().to_string();

                if let Some(ref local_rev) = pkg.current_commit {
                    if local_rev != &remote_rev {
                        println!(
                            "[devel] {} has new SVN revision: {} -> {}",
                            pkg.name, local_rev, remote_rev
                        );
                        pkg.current_commit = Some(remote_rev);
                        return Ok(true);
                    }
                    Ok(false)
                } else {
                    // First time tracking
                    pkg.current_commit = Some(remote_rev);
                    Ok(false)
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow!("SVN info failed for {}: {}", pkg.name, stderr))
            }
            Err(e) => Err(anyhow!("Failed to run svn: {}", e)),
        }
    }

    fn check_hg_update(&self, pkg: &mut DevelPackage) -> Result<bool> {
        // Get remote tip using hg identify
        let output = Command::new("hg")
            .args(["identify", "--id", "--rev", "tip", &pkg.url])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let remote_id = String::from_utf8_lossy(&out.stdout).trim().to_string();

                if let Some(ref local_id) = pkg.current_commit {
                    if local_id != &remote_id {
                        println!(
                            "[devel] {} has new Mercurial changeset: {} -> {}",
                            pkg.name, local_id, remote_id
                        );
                        pkg.current_commit = Some(remote_id);
                        return Ok(true);
                    }
                    Ok(false)
                } else {
                    // First time tracking
                    pkg.current_commit = Some(remote_id);
                    Ok(false)
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow!(
                    "Mercurial identify failed for {}: {}",
                    pkg.name,
                    stderr
                ))
            }
            Err(e) => Err(anyhow!("Failed to run hg: {}", e)),
        }
    }

    fn check_bzr_update(&self, pkg: &mut DevelPackage) -> Result<bool> {
        // Get remote revision using bzr revno
        let output = Command::new("bzr").args(["revno", &pkg.url]).output();

        match output {
            Ok(out) if out.status.success() => {
                let remote_revno = String::from_utf8_lossy(&out.stdout).trim().to_string();

                if let Some(ref local_revno) = pkg.current_commit {
                    if local_revno != &remote_revno {
                        println!(
                            "[devel] {} has new Bazaar revision: {} -> {}",
                            pkg.name, local_revno, remote_revno
                        );
                        pkg.current_commit = Some(remote_revno);
                        return Ok(true);
                    }
                    Ok(false)
                } else {
                    // First time tracking
                    pkg.current_commit = Some(remote_revno);
                    Ok(false)
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow!("Bazaar revno failed for {}: {}", pkg.name, stderr))
            }
            Err(e) => Err(anyhow!("Failed to run bzr: {}", e)),
        }
    }
}

// Helper function to fetch development package info during installation
pub fn fetch_devel_info(
    package_name: &str,
    pkgbuild_path: &Path,
    install_version: &str,
) -> Result<()> {
    let mut devel_manager = DevelManager::new()?;
    devel_manager.add_devel_package(package_name, pkgbuild_path, install_version)
}

// Helper function to load development package information
pub fn load_devel_info() -> Result<DevelInfo> {
    let devel_manager = DevelManager::new()?;
    devel_manager.load_devel_info()
}

// Helper function to save development package information
pub fn save_devel_info(info: &DevelInfo) -> Result<()> {
    let devel_manager = DevelManager::new()?;
    devel_manager.save_devel_info(info)
}

// Check if a package is a development package (contains VCS sources)
pub fn is_devel_package(pkgbuild_path: &Path) -> Result<bool> {
    let devel_manager = DevelManager::new()?;
    Ok(devel_manager.extract_vcs_info(pkgbuild_path)?.is_some())
}

/// Check if a package name indicates it's a development package
/// Common suffixes: -git, -svn, -hg, -bzr, -cvs, -darcs
pub fn is_devel_package_name(name: &str) -> bool {
    let devel_suffixes = ["-git", "-svn", "-hg", "-bzr", "-cvs", "-darcs"];
    devel_suffixes.iter().any(|suffix| name.ends_with(suffix))
}

/// Check for updates on a single tracked devel package
pub fn check_single_package_update(name: &str) -> Result<Option<String>> {
    let manager = DevelManager::new()?;
    let mut info = manager.load_devel_info()?;

    let result = if let Some(pkg) = info.packages.get_mut(name) {
        let has_update = manager.check_package_update(pkg)?;
        pkg.last_checked = Utc::now();
        if has_update {
            pkg.current_commit.clone()
        } else {
            None
        }
    } else {
        None
    };

    manager.save_devel_info(&info)?;
    Ok(result)
}

/// Get all tracked devel packages that have updates available
pub fn get_all_devel_updates() -> Result<Vec<(String, String)>> {
    let manager = DevelManager::new()?;
    let mut info = manager.load_devel_info()?;
    let mut updates = Vec::new();

    for (name, pkg) in info.packages.iter_mut() {
        match manager.check_package_update(pkg) {
            Ok(true) => {
                if let Some(ref commit) = pkg.current_commit {
                    updates.push((name.clone(), commit.clone()));
                }
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("[devel] Failed to check {}: {}", name, e);
            }
        }
        pkg.last_checked = Utc::now();
    }

    info.last_update = Utc::now();
    manager.save_devel_info(&info)?;

    Ok(updates)
}

/// Parse VCS sources from PKGBUILD content (for external use)
pub fn parse_vcs_sources(pkgbuild_content: &str) -> Vec<(VcsType, String)> {
    let mut sources = Vec::new();

    for line in pkgbuild_content.lines() {
        let line = line.trim();
        if line.starts_with("source")
            && line.contains('=')
            && let Some(sources_part) = line.split('=').nth(1)
        {
            let sources_str = sources_part.trim_matches(['(', ')', '"', '\'']).trim();
            for source in sources_str.split_whitespace() {
                let source = source.trim_matches(['"', '\'']);
                let vcs_type = VcsType::from_url(source);
                if vcs_type != VcsType::Unknown {
                    sources.push((vcs_type, source.to_string()));
                }
            }
        }
    }

    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_vcs_type_detection() {
        assert_eq!(
            VcsType::from_url("git://github.com/user/repo.git"),
            VcsType::Git
        );
        assert_eq!(
            VcsType::from_url("https://github.com/user/repo.git"),
            VcsType::Git
        );
        assert_eq!(
            VcsType::from_url("git+https://github.com/user/repo.git"),
            VcsType::Git
        );
        assert_eq!(
            VcsType::from_url("svn://svn.example.com/repo"),
            VcsType::Svn
        );
        assert_eq!(
            VcsType::from_url("hg+https://hg.example.com/repo"),
            VcsType::Hg
        );
        assert_eq!(VcsType::from_url("bzr+lp:project"), VcsType::Bzr);
        assert_eq!(
            VcsType::from_url("https://example.com/file.tar.gz"),
            VcsType::Unknown
        );
    }

    #[test]
    fn test_devel_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        // SAFETY: This test runs single-threaded and the env var is only used
        // to set up the test environment before DevelManager::new() is called.
        unsafe {
            std::env::set_var("XDG_CACHE_HOME", temp_dir.path());
        }

        let manager = DevelManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_is_devel_package_name() {
        // Should detect -git packages
        assert!(is_devel_package_name("yay-git"));
        assert!(is_devel_package_name("neovim-git"));
        assert!(is_devel_package_name("linux-zen-git"));

        // Should detect other VCS suffixes
        assert!(is_devel_package_name("project-svn"));
        assert!(is_devel_package_name("repo-hg"));
        assert!(is_devel_package_name("code-bzr"));
        assert!(is_devel_package_name("app-cvs"));
        assert!(is_devel_package_name("tool-darcs"));

        // Should NOT match regular packages
        assert!(!is_devel_package_name("firefox"));
        assert!(!is_devel_package_name("git"));
        assert!(!is_devel_package_name("github-cli"));
        assert!(!is_devel_package_name("gitui"));
    }

    #[test]
    fn test_parse_vcs_sources() {
        let pkgbuild = r#"
pkgname=example-git
pkgver=1.0.0
source=("git+https://github.com/user/repo.git")
sha256sums=('SKIP')
"#;
        let sources = parse_vcs_sources(pkgbuild);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].0, VcsType::Git);
        assert!(sources[0].1.contains("github.com"));

        // Multiple sources on single line
        let pkgbuild_multi = r#"
source=("git+https://github.com/user/repo.git" "svn://svn.example.com/trunk" "https://example.com/patch.tar.gz")
"#;
        let sources = parse_vcs_sources(pkgbuild_multi);
        assert_eq!(sources.len(), 2); // tar.gz is not VCS
        assert_eq!(sources[0].0, VcsType::Git);
        assert_eq!(sources[1].0, VcsType::Svn);
    }

    #[test]
    fn test_vcs_command() {
        assert_eq!(VcsType::Git.command(), "git");
        assert_eq!(VcsType::Svn.command(), "svn");
        assert_eq!(VcsType::Hg.command(), "hg");
        assert_eq!(VcsType::Bzr.command(), "bzr");
        assert_eq!(VcsType::Unknown.command(), "");
    }
}
