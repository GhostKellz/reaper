use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub url: String,
    pub priority: u32,
    pub enabled: bool,
    pub sig_level: SigLevel,
    pub servers: Vec<String>,
    pub include_path: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub last_updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SigLevel {
    Never,
    Optional,
    Required,
    TrustedOnly,
    TrustAll,
}

impl std::fmt::Display for SigLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SigLevel::Never => write!(f, "Never"),
            SigLevel::Optional => write!(f, "Optional"),
            SigLevel::Required => write!(f, "Required"),
            SigLevel::TrustedOnly => write!(f, "TrustedOnly"),
            SigLevel::TrustAll => write!(f, "TrustAll"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub architecture: Vec<String>,
    pub repository: String,
    pub url: Option<String>,
    pub licenses: Vec<String>,
    pub groups: Vec<String>,
    pub depends: Vec<String>,
    pub optional_depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub download_size: u64,
    pub installed_size: u64,
    pub packager: String,
    pub build_date: DateTime<Utc>,
    pub install_date: Option<DateTime<Utc>>,
}

pub struct RepositoryManager {
    repositories: Arc<RwLock<HashMap<String, Repository>>>,
    package_cache: Arc<RwLock<HashMap<String, Vec<Package>>>>,
    config_path: PathBuf,
    custom_repos: Arc<RwLock<Vec<Repository>>>,
}

impl RepositoryManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = PathBuf::from("/etc/pacman.conf");
        let mut manager = Self {
            repositories: Arc::new(RwLock::new(HashMap::new())),
            package_cache: Arc::new(RwLock::new(HashMap::new())),
            config_path,
            custom_repos: Arc::new(RwLock::new(Vec::new())),
        };

        manager.load_repositories()?;
        Ok(manager)
    }

    fn load_repositories(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut repos = HashMap::new();

        repos.insert(
            "core".to_string(),
            Repository {
                name: "core".to_string(),
                url: "https://archlinux.org/packages/core/".to_string(),
                priority: 1,
                enabled: true,
                sig_level: SigLevel::Required,
                servers: vec!["https://mirror.archlinux.org/$repo/os/$arch".to_string()],
                include_path: Some(PathBuf::from("/etc/pacman.d/mirrorlist")),
                cache_dir: Some(PathBuf::from("/var/cache/pacman/pkg")),
                last_updated: None,
            },
        );

        repos.insert(
            "extra".to_string(),
            Repository {
                name: "extra".to_string(),
                url: "https://archlinux.org/packages/extra/".to_string(),
                priority: 2,
                enabled: true,
                sig_level: SigLevel::Required,
                servers: vec!["https://mirror.archlinux.org/$repo/os/$arch".to_string()],
                include_path: Some(PathBuf::from("/etc/pacman.d/mirrorlist")),
                cache_dir: Some(PathBuf::from("/var/cache/pacman/pkg")),
                last_updated: None,
            },
        );

        repos.insert(
            "multilib".to_string(),
            Repository {
                name: "multilib".to_string(),
                url: "https://archlinux.org/packages/multilib/".to_string(),
                priority: 3,
                enabled: true,
                sig_level: SigLevel::Required,
                servers: vec!["https://mirror.archlinux.org/$repo/os/$arch".to_string()],
                include_path: Some(PathBuf::from("/etc/pacman.d/mirrorlist")),
                cache_dir: Some(PathBuf::from("/var/cache/pacman/pkg")),
                last_updated: None,
            },
        );

        self.parse_pacman_conf(&mut repos)?;

        let custom_repos = self.load_custom_repos()?;
        for repo in custom_repos {
            repos.insert(repo.name.clone(), repo);
        }

        *self.repositories.write().unwrap() = repos;
        Ok(())
    }

    fn parse_pacman_conf(
        &self,
        repos: &mut HashMap<String, Repository>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&self.config_path)?;
        let mut current_repo: Option<String> = None;
        let mut current_servers: Vec<String> = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                if let Some(repo_name) = current_repo.take() {
                    if let Some(repo) = repos.get_mut(&repo_name) {
                        repo.servers.extend(current_servers.clone());
                    }
                    current_servers.clear();
                }

                let repo_name = line[1..line.len() - 1].to_string();
                if repo_name != "options" {
                    current_repo = Some(repo_name);
                }
            } else if line.starts_with("Server") {
                if let Some(server) = line.split('=').nth(1) {
                    current_servers.push(server.trim().to_string());
                }
            } else if line.starts_with("Include")
                && let Some(include) = line.split('=').nth(1)
                && let Some(ref repo_name) = current_repo
                && let Some(repo) = repos.get_mut(repo_name)
            {
                repo.include_path = Some(PathBuf::from(include.trim()));
            }
        }

        if let Some(repo_name) = current_repo
            && let Some(repo) = repos.get_mut(&repo_name)
        {
            repo.servers.extend(current_servers);
        }

        Ok(())
    }

    fn load_custom_repos(&self) -> Result<Vec<Repository>, Box<dyn std::error::Error>> {
        let config_dir = crate::paths::CONFIG_DIR.join("repos");

        if !config_dir.exists() {
            return Ok(Vec::new());
        }

        let mut custom_repos = Vec::new();

        for entry in fs::read_dir(config_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)?;
                let repo: Repository = serde_json::from_str(&content)?;
                custom_repos.push(repo);
            }
        }

        Ok(custom_repos)
    }

    pub fn add_repository(&self, repo: Repository) -> Result<(), Box<dyn std::error::Error>> {
        let mut repos = self.repositories.write().unwrap();

        if repos.contains_key(&repo.name) {
            return Err(format!("Repository '{}' already exists", repo.name).into());
        }

        println!("{} Adding repository: {}", "→".blue(), repo.name.bold());
        repos.insert(repo.name.clone(), repo.clone());

        let mut custom_repos = self.custom_repos.write().unwrap();
        custom_repos.push(repo.clone());

        self.save_custom_repo(&repo)?;
        Ok(())
    }

    fn save_custom_repo(&self, repo: &Repository) -> Result<(), Box<dyn std::error::Error>> {
        let config_dir = crate::paths::CONFIG_DIR.join("repos");

        fs::create_dir_all(&config_dir)?;

        let repo_file = config_dir.join(format!("{}.json", repo.name));
        let content = serde_json::to_string_pretty(repo)?;
        fs::write(repo_file, content)?;

        Ok(())
    }

    pub fn remove_repository(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut repos = self.repositories.write().unwrap();

        if !repos.contains_key(name) {
            return Err(format!("Repository '{}' not found", name).into());
        }

        if ["core", "extra", "multilib"].contains(&name) {
            return Err("Cannot remove system repositories".into());
        }

        println!("{} Removing repository: {}", "→".red(), name.bold());
        repos.remove(name);

        let config_dir = crate::paths::CONFIG_DIR.join("repos");

        let repo_file = config_dir.join(format!("{}.json", name));
        if repo_file.exists() {
            fs::remove_file(repo_file)?;
        }

        Ok(())
    }

    pub fn set_priority(
        &self,
        name: &str,
        priority: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut repos = self.repositories.write().unwrap();

        let repo = repos
            .get_mut(name)
            .ok_or(format!("Repository '{}' not found", name))?;

        repo.priority = priority;

        if !["core", "extra", "multilib"].contains(&name) {
            self.save_custom_repo(repo)?;
        }

        Ok(())
    }

    pub fn enable_repository(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut repos = self.repositories.write().unwrap();

        let repo = repos
            .get_mut(name)
            .ok_or(format!("Repository '{}' not found", name))?;

        repo.enabled = true;
        println!("{} Enabled repository: {}", "✓".green(), name.bold());

        Ok(())
    }

    pub fn disable_repository(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut repos = self.repositories.write().unwrap();

        if ["core", "extra"].contains(&name) {
            return Err("Cannot disable essential repositories".into());
        }

        let repo = repos
            .get_mut(name)
            .ok_or(format!("Repository '{}' not found", name))?;

        repo.enabled = false;
        println!("{} Disabled repository: {}", "✗".red(), name.bold());

        Ok(())
    }

    pub async fn sync_repository(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Check if repo exists and is enabled, using a block to limit lock scope
        {
            let repos = self.repositories.read().unwrap();
            let repo = repos
                .get(name)
                .ok_or(format!("Repository '{}' not found", name))?;

            if !repo.enabled {
                return Err(format!("Repository '{}' is disabled", name).into());
            }
        }

        println!("{} Syncing repository: {}", "→".blue(), name.bold());

        let output = Command::new("pacman").args(["-Sy", name]).output()?;

        if !output.status.success() {
            return Err(format!(
                "Failed to sync repository: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        {
            let mut repos = self.repositories.write().unwrap();
            if let Some(repo) = repos.get_mut(name) {
                repo.last_updated = Some(Utc::now());
            }
        }

        self.refresh_package_cache(name).await?;

        println!("{} Repository '{}' synced successfully", "✓".green(), name);
        Ok(())
    }

    pub async fn sync_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let enabled_repos: Vec<String> = {
            let repos = self.repositories.read().unwrap();
            repos
                .iter()
                .filter(|(_, r)| r.enabled)
                .map(|(name, _)| name.clone())
                .collect()
        };

        for repo_name in enabled_repos {
            if let Err(e) = self.sync_repository(&repo_name).await {
                eprintln!("{} Failed to sync {}: {}", "✗".red(), repo_name, e);
            }
        }

        Ok(())
    }

    async fn refresh_package_cache(
        &self,
        repo_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let output = Command::new("pacman").args(["-Sl", repo_name]).output()?;

        if !output.status.success() {
            return Ok(());
        }

        let mut packages = Vec::new();
        let output_str = String::from_utf8_lossy(&output.stdout);

        for line in output_str.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let package = Package {
                    name: parts[1].to_string(),
                    version: parts[2].to_string(),
                    repository: repo_name.to_string(),
                    description: String::new(),
                    architecture: vec!["x86_64".to_string()],
                    url: None,
                    licenses: Vec::new(),
                    groups: Vec::new(),
                    depends: Vec::new(),
                    optional_depends: Vec::new(),
                    provides: Vec::new(),
                    conflicts: Vec::new(),
                    replaces: Vec::new(),
                    download_size: 0,
                    installed_size: 0,
                    packager: String::new(),
                    build_date: Utc::now(),
                    install_date: None,
                };
                packages.push(package);
            }
        }

        let mut cache = self.package_cache.write().unwrap();
        cache.insert(repo_name.to_string(), packages);

        Ok(())
    }

    pub fn search_packages(&self, query: &str) -> Vec<Package> {
        let cache = self.package_cache.read().unwrap();
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        for packages in cache.values() {
            for package in packages {
                if package.name.to_lowercase().contains(&query_lower)
                    || package.description.to_lowercase().contains(&query_lower)
                {
                    results.push(package.clone());
                }
            }
        }

        let repos = self.repositories.read().unwrap();
        results.sort_by_key(|p| repos.get(&p.repository).map(|r| r.priority).unwrap_or(999));

        results
    }

    pub fn get_package_from_repos(&self, name: &str) -> Option<Package> {
        let cache = self.package_cache.read().unwrap();
        let repos = self.repositories.read().unwrap();

        let mut candidates: Vec<(Package, u32)> = Vec::new();

        for (repo_name, packages) in cache.iter() {
            if let Some(repo) = repos.get(repo_name) {
                if !repo.enabled {
                    continue;
                }

                if let Some(package) = packages.iter().find(|p| p.name == name) {
                    candidates.push((package.clone(), repo.priority));
                }
            }
        }

        candidates.sort_by_key(|(_, priority)| *priority);
        candidates.into_iter().next().map(|(package, _)| package)
    }

    pub fn list_repositories(&self) {
        let repos = self.repositories.read().unwrap();
        let mut sorted_repos: Vec<_> = repos.values().collect();
        sorted_repos.sort_by_key(|r| r.priority);

        println!("{}", "=== Repository Configuration ===".bold().blue());
        println!();

        for repo in sorted_repos {
            let status = if repo.enabled {
                "ENABLED".green()
            } else {
                "DISABLED".red()
            };

            println!(
                "{} {} [Priority: {}] {}",
                if repo.enabled {
                    "●".green()
                } else {
                    "○".red()
                },
                repo.name.bold(),
                repo.priority.to_string().yellow(),
                status
            );

            if let Some(ref include) = repo.include_path {
                println!("  Include: {}", include.display());
            }

            if !repo.servers.is_empty() {
                println!("  Servers: {}", repo.servers.len());
            }

            if let Some(ref updated) = repo.last_updated {
                println!("  Last sync: {}", updated.format("%Y-%m-%d %H:%M:%S"));
            }

            println!("  Signature: {}", repo.sig_level);
            println!();
        }
    }

    pub fn verify_repositories(&self) -> Result<(), Box<dyn std::error::Error>> {
        let repos = self.repositories.read().unwrap();

        println!("{} Verifying repositories...", "→".blue());

        for (name, repo) in repos.iter() {
            if !repo.enabled {
                continue;
            }

            print!("  Checking {}: ", name);

            if repo.servers.is_empty() && repo.include_path.is_none() {
                println!("{}", "NO SERVERS".red());
                continue;
            }

            if let Some(ref include) = repo.include_path
                && !include.exists()
            {
                println!(
                    "{}",
                    format!("Include file not found: {}", include.display()).red()
                );
                continue;
            }

            println!("{}", "OK".green());
        }

        Ok(())
    }
}

pub struct RepositoryTransaction {
    changes: Vec<RepoChange>,
}

impl Default for RepositoryTransaction {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
enum RepoChange {
    Add(Repository),
    Remove(String),
    SetPriority(String, u32),
    Enable(String),
    Disable(String),
}

impl RepositoryTransaction {
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
        }
    }

    pub fn add_repository(mut self, repo: Repository) -> Self {
        self.changes.push(RepoChange::Add(repo));
        self
    }

    pub fn remove_repository(mut self, name: String) -> Self {
        self.changes.push(RepoChange::Remove(name));
        self
    }

    pub fn set_priority(mut self, name: String, priority: u32) -> Self {
        self.changes.push(RepoChange::SetPriority(name, priority));
        self
    }

    pub fn enable(mut self, name: String) -> Self {
        self.changes.push(RepoChange::Enable(name));
        self
    }

    pub fn disable(mut self, name: String) -> Self {
        self.changes.push(RepoChange::Disable(name));
        self
    }

    pub fn apply(self, manager: &RepositoryManager) -> Result<(), Box<dyn std::error::Error>> {
        for change in self.changes {
            match change {
                RepoChange::Add(repo) => manager.add_repository(repo)?,
                RepoChange::Remove(name) => manager.remove_repository(&name)?,
                RepoChange::SetPriority(name, priority) => manager.set_priority(&name, priority)?,
                RepoChange::Enable(name) => manager.enable_repository(&name)?,
                RepoChange::Disable(name) => manager.disable_repository(&name)?,
            }
        }
        Ok(())
    }
}
