use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub name: String,
    pub backend_order: Vec<String>,
    pub auto_install_deps: Vec<String>,
    pub pinned_packages: Vec<String>,
    pub ignored_packages: Vec<String>,
    pub parallel_jobs: Option<usize>,
    pub fast_mode: Option<bool>,
    pub strict_signatures: Option<bool>,
    pub auto_resolve_deps: Option<bool>,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            backend_order: vec!["tap".to_string(), "aur".to_string(), "pacman".to_string()],
            auto_install_deps: vec![],
            pinned_packages: vec![],
            ignored_packages: vec![],
            parallel_jobs: Some(4),
            fast_mode: Some(false),
            strict_signatures: Some(false),
            auto_resolve_deps: Some(true),
        }
    }
}

pub struct ProfileManager {
    profiles_dir: PathBuf,
    active_profile: String,
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileManager {
    pub fn new() -> Self {
        let profiles_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap/profiles");
        let _ = fs::create_dir_all(&profiles_dir);

        Self {
            profiles_dir,
            active_profile: "default".to_string(),
        }
    }

    pub fn create_profile(&self, profile: &ProfileConfig) -> Result<()> {
        let profile_path = self.profiles_dir.join(format!("{}.toml", profile.name));
        let toml_content = toml::to_string_pretty(profile)?;
        fs::write(profile_path, toml_content)?;
        println!("[profiles] Created profile: {}", profile.name);
        Ok(())
    }

    pub fn load_profile(&self, name: &str) -> Result<ProfileConfig> {
        let profile_path = self.profiles_dir.join(format!("{}.toml", name));
        if !profile_path.exists() {
            return Ok(ProfileConfig::default());
        }

        let content = fs::read_to_string(profile_path)?;
        let profile: ProfileConfig = toml::from_str(&content)?;
        Ok(profile)
    }
    pub fn switch_profile(&mut self, name: &str) -> Result<()> {
        let _profile = self.load_profile(name)?;
        self.active_profile = name.to_string();

        // Update active profile marker
        let active_path = self.profiles_dir.join(".active");
        fs::write(active_path, name)?;

        println!("[profiles] Switched to profile: {}", name);
        Ok(())
    }

    pub fn get_active_profile(&self) -> Result<ProfileConfig> {
        self.load_profile(&self.active_profile)
    }

    pub fn list_profiles(&self) -> Result<Vec<String>> {
        let mut profiles = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.profiles_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "toml" {
                        if let Some(name) = entry.path().file_stem() {
                            profiles.push(name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        Ok(profiles)
    }

    pub fn delete_profile(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(anyhow::anyhow!("Cannot delete default profile"));
        }

        let profile_path = self.profiles_dir.join(format!("{}.toml", name));
        if profile_path.exists() {
            fs::remove_file(profile_path)?;
            println!("[profiles] Deleted profile: {}", name);
        }
        Ok(())
    }
}

// Predefined profiles for common use cases
pub fn create_developer_profile() -> ProfileConfig {
    ProfileConfig {
        name: "developer".to_string(),
        backend_order: vec!["tap".to_string(), "aur".to_string(), "flatpak".to_string()],
        auto_install_deps: vec![
            "base-devel".to_string(),
            "git".to_string(),
            "rust".to_string(),
            "nodejs".to_string(),
            "python".to_string(),
        ],
        pinned_packages: vec!["linux-lts".to_string()],
        parallel_jobs: Some(8),
        fast_mode: Some(false),
        strict_signatures: Some(true),
        auto_resolve_deps: Some(true),
        ..Default::default()
    }
}

pub fn create_gaming_profile() -> ProfileConfig {
    ProfileConfig {
        name: "gaming".to_string(),
        backend_order: vec![
            "flatpak".to_string(),
            "aur".to_string(),
            "chaotic-aur".to_string(),
        ],
        auto_install_deps: vec![
            "steam".to_string(),
            "lutris".to_string(),
            "wine".to_string(),
            "gamemode".to_string(),
        ],
        fast_mode: Some(true),
        strict_signatures: Some(false),
        parallel_jobs: Some(6),
        ..Default::default()
    }
}

pub fn create_minimal_profile() -> ProfileConfig {
    ProfileConfig {
        name: "minimal".to_string(),
        backend_order: vec!["pacman".to_string(), "aur".to_string()],
        auto_install_deps: vec![],
        parallel_jobs: Some(2),
        fast_mode: Some(true),
        strict_signatures: Some(false),
        auto_resolve_deps: Some(false),
        ..Default::default()
    }
}
