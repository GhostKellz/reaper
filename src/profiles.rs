use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use crate::config::ProfileFile;

/// Manages user profiles for Reaper.
///
/// Profiles allow users to quickly switch between different configuration
/// presets (e.g., developer, gaming, minimal). Profile settings override
/// the base config file when active.
///
/// Profile files are stored in `~/.config/reap/profiles/`.
/// The active profile is tracked in `~/.config/reap/profiles/.active`.
pub struct ProfileManager {
    profiles_dir: PathBuf,
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileManager {
    pub fn new() -> Self {
        let profiles_dir = crate::paths::profiles_dir();
        let _ = fs::create_dir_all(&profiles_dir);

        Self { profiles_dir }
    }

    /// Create a new profile from a ProfileFile.
    pub fn create_profile(&self, profile: &ProfileFile) -> Result<()> {
        let profile_path = self.profiles_dir.join(format!("{}.toml", profile.name));
        let toml_content = toml::to_string_pretty(profile)?;
        fs::write(profile_path, toml_content)?;
        println!("[profiles] Created profile: {}", profile.name);
        Ok(())
    }

    /// Load a profile by name.
    pub fn load_profile(&self, name: &str) -> Result<ProfileFile> {
        let profile_path = self.profiles_dir.join(format!("{}.toml", name));
        if !profile_path.exists() {
            return Ok(default_profile());
        }

        let content = fs::read_to_string(profile_path)?;
        let profile: ProfileFile = toml::from_str(&content)?;
        Ok(profile)
    }

    /// Switch to a different profile.
    pub fn switch_profile(&self, name: &str) -> Result<()> {
        // Verify profile exists (or is "default")
        if name != "default" {
            let profile_path = self.profiles_dir.join(format!("{}.toml", name));
            if !profile_path.exists() {
                return Err(anyhow::anyhow!("Profile '{}' not found", name));
            }
        }

        // Update active profile marker
        let active_path = self.profiles_dir.join(".active");
        fs::write(active_path, name)?;

        println!("[profiles] Switched to profile: {}", name);
        Ok(())
    }

    /// Get the name of the currently active profile.
    pub fn get_active_profile_name(&self) -> String {
        let active_path = self.profiles_dir.join(".active");
        if active_path.exists() {
            fs::read_to_string(&active_path)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "default".to_string())
        } else {
            "default".to_string()
        }
    }

    /// Get the currently active profile.
    pub fn get_active_profile(&self) -> Result<ProfileFile> {
        let name = self.get_active_profile_name();
        self.load_profile(&name)
    }

    /// List all available profiles.
    pub fn list_profiles(&self) -> Result<Vec<String>> {
        let mut profiles = vec!["default".to_string()];

        if let Ok(entries) = fs::read_dir(&self.profiles_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension()
                    && ext == "toml"
                    && let Some(name) = entry.path().file_stem()
                {
                    let name_str = name.to_string_lossy().to_string();
                    if name_str != "default" {
                        profiles.push(name_str);
                    }
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// Delete a profile.
    pub fn delete_profile(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(anyhow::anyhow!("Cannot delete default profile"));
        }

        let profile_path = self.profiles_dir.join(format!("{}.toml", name));
        if profile_path.exists() {
            fs::remove_file(profile_path)?;
            println!("[profiles] Deleted profile: {}", name);

            // If this was the active profile, switch to default
            if self.get_active_profile_name() == name {
                self.switch_profile("default")?;
            }
        } else {
            return Err(anyhow::anyhow!("Profile '{}' not found", name));
        }
        Ok(())
    }
}

/// Default profile configuration.
fn default_profile() -> ProfileFile {
    ProfileFile {
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

// === Predefined Profile Templates ===

/// Create a developer-focused profile.
pub fn create_developer_profile() -> ProfileFile {
    ProfileFile {
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
        ignored_packages: vec![],
        parallel_jobs: Some(8),
        fast_mode: Some(false),
        strict_signatures: Some(true),
        auto_resolve_deps: Some(true),
    }
}

/// Create a gaming-focused profile.
pub fn create_gaming_profile() -> ProfileFile {
    ProfileFile {
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
        pinned_packages: vec![],
        ignored_packages: vec![],
        parallel_jobs: Some(6),
        fast_mode: Some(true),
        strict_signatures: Some(false),
        auto_resolve_deps: Some(true),
    }
}

/// Create a minimal profile with conservative settings.
pub fn create_minimal_profile() -> ProfileFile {
    ProfileFile {
        name: "minimal".to_string(),
        backend_order: vec!["pacman".to_string(), "aur".to_string()],
        auto_install_deps: vec![],
        pinned_packages: vec![],
        ignored_packages: vec![],
        parallel_jobs: Some(2),
        fast_mode: Some(true),
        strict_signatures: Some(false),
        auto_resolve_deps: Some(false),
    }
}

// === Legacy Re-exports for Compatibility ===

/// Legacy alias - use ProfileFile instead.
pub type ProfileConfig = ProfileFile;
