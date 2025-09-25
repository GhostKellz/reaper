use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use toml_edit::{DocumentMut, value};
use anyhow::{anyhow, Result};
use dirs::config_dir;

#[derive(Debug, Clone)]
pub struct ReapConfig {
    /// Packages to ignore during upgrades
    pub ignored_packages: Vec<String>,
    /// Number of parallel jobs for install/upgrade
    pub parallel: usize,
    /// Development package tracking
    pub devel: DevelConfig,
    /// Build configuration
    pub build: BuildConfig,
    /// Security settings
    pub security: SecurityConfig,
}

#[derive(Debug, Clone)]
pub struct DevelConfig {
    pub auto_check: bool,
    pub check_interval_hours: u64,
    pub vcs_types: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub use_chroot: bool,
    pub chroot_dir: PathBuf,
    pub makepkg_flags: Vec<String>,
    pub clean_after_build: bool,
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub verify_signatures: bool,
    pub strict_mode: bool,
    pub scan_pkgbuilds: bool,
    pub trust_threshold: f64,
}

impl ReapConfig {
    pub fn load() -> Self {
        Self::load_from_sources().unwrap_or_else(|e| {
            eprintln!("[config] Warning: Failed to load config: {}", e);
            Self::default()
        })
    }

    /// Load configuration from multiple sources with precedence:
    /// 1. Command line arguments (highest)
    /// 2. Environment variables
    /// 3. User config file (~/.config/reap/reap.toml)
    /// 4. Global config file (/etc/reap/reap.toml)
    /// 5. Profile-specific configs
    /// 6. Defaults (lowest)
    pub fn load_from_sources() -> Result<Self> {
        let mut config = Self::default();

        // Load global config first
        if let Ok(global_config) = Self::load_global_config() {
            config.merge_with_global(global_config);
        }

        // Load user config (overrides global)
        if let Ok(user_config) = Self::load_user_config() {
            config.merge_with_user(user_config);
        }

        // Load active profile config (overrides user)
        if let Ok(profile_config) = Self::load_active_profile_config() {
            config.merge_with_profile(profile_config);
        }

        // Apply environment variable overrides
        config.apply_env_overrides();

        // Validate final configuration
        config.validate()?;

        Ok(config)
    }

    /// Check if a package is ignored (used in upgrade/install logic)
    pub fn is_ignored(&self, pkg: &str) -> bool {
        self.ignored_packages.iter().any(|p| p == pkg)
    }

    fn load_global_config() -> Result<GlobalConfig> {
        let path = PathBuf::from("/etc/reap/reap.toml");
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            toml::from_str(&content).map_err(|e| anyhow!("Invalid global config: {}", e))
        } else {
            Ok(GlobalConfig::default())
        }
    }

    fn load_user_config() -> Result<GlobalConfig> {
        let config_path = config_dir()
            .ok_or_else(|| anyhow!("Cannot determine config directory"))?
            .join("reap")
            .join("reap.toml");

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            toml::from_str(&content).map_err(|e| anyhow!("Invalid user config: {}", e))
        } else {
            Ok(GlobalConfig::default())
        }
    }

    fn load_active_profile_config() -> Result<GlobalConfig> {
        // TODO: Implement profile loading when profiles module is enhanced
        Ok(GlobalConfig::default())
    }

    fn merge_with_global(&mut self, global: GlobalConfig) {
        // Merge global configuration settings
        if let Some(cache) = global.enable_cache {
            self.parallel = if cache { 4 } else { 2 };
        }

        if let Some(devel) = global.devel {
            if let Some(auto_check) = devel.auto_check {
                self.devel.auto_check = auto_check;
            }
            if let Some(interval) = devel.check_interval_hours {
                self.devel.check_interval_hours = interval;
            }
            if let Some(vcs_types) = devel.vcs_types {
                self.devel.vcs_types = vcs_types;
            }
        }

        if let Some(build) = global.build {
            if let Some(use_chroot) = build.use_chroot {
                self.build.use_chroot = use_chroot;
            }
            if let Some(chroot_dir) = build.chroot_dir {
                self.build.chroot_dir = chroot_dir;
            }
            if let Some(makepkg_flags) = build.makepkg_flags {
                self.build.makepkg_flags = makepkg_flags;
            }
            if let Some(clean_after) = build.clean_after_build {
                self.build.clean_after_build = clean_after;
            }
        }

        if let Some(security) = global.security {
            if let Some(verify_sigs) = security.verify_signatures {
                self.security.verify_signatures = verify_sigs;
            }
            if let Some(strict) = security.strict_mode {
                self.security.strict_mode = strict;
            }
            if let Some(scan) = security.scan_pkgbuilds {
                self.security.scan_pkgbuilds = scan;
            }
            if let Some(threshold) = security.trust_threshold {
                self.security.trust_threshold = threshold;
            }
        }
    }

    fn merge_with_user(&mut self, user: GlobalConfig) {
        // User config takes precedence over global
        if let Some(cache) = user.enable_cache {
            self.parallel = if cache { 6 } else { 2 }; // Higher parallelism for user config
        }
        self.merge_with_global(user); // Same merge logic, user takes precedence
    }

    fn merge_with_profile(&mut self, profile: GlobalConfig) {
        // Profile config takes precedence over user
        self.merge_with_global(profile);
    }

    fn apply_env_overrides(&mut self) {
        // Apply environment variable overrides
        if let Ok(parallel) = std::env::var("REAP_PARALLEL_JOBS") {
            if let Ok(jobs) = parallel.parse::<usize>() {
                self.parallel = jobs;
            }
        }

        if let Ok(ignored) = std::env::var("REAP_IGNORED_PACKAGES") {
            self.ignored_packages = ignored.split(',').map(|s| s.trim().to_string()).collect();
        }

        if let Ok(use_chroot) = std::env::var("REAP_USE_CHROOT") {
            self.build.use_chroot = use_chroot.parse().unwrap_or(false);
        }

        if let Ok(strict_mode) = std::env::var("REAP_STRICT_MODE") {
            self.security.strict_mode = strict_mode.parse().unwrap_or(false);
        }
    }

    fn validate(&self) -> Result<()> {
        if self.parallel == 0 {
            return Err(anyhow!("Parallel jobs must be greater than 0"));
        }

        if self.parallel > 32 {
            return Err(anyhow!("Parallel jobs should not exceed 32 for stability"));
        }

        if self.security.trust_threshold < 0.0 || self.security.trust_threshold > 10.0 {
            return Err(anyhow!("Trust threshold must be between 0.0 and 10.0"));
        }

        if !self.build.chroot_dir.exists() && self.build.use_chroot {
            // Try to create chroot directory
            if let Err(e) = fs::create_dir_all(&self.build.chroot_dir) {
                return Err(anyhow!("Cannot create chroot directory: {}", e));
            }
        }

        Ok(())
    }
}

impl Default for ReapConfig {
    fn default() -> Self {
        Self {
            ignored_packages: vec![],
            parallel: 2,
            devel: DevelConfig {
                auto_check: true,
                check_interval_hours: 24,
                vcs_types: vec![
                    "git".to_string(),
                    "svn".to_string(),
                    "hg".to_string(),
                    "bzr".to_string(),
                ],
            },
            build: BuildConfig {
                use_chroot: false,
                chroot_dir: PathBuf::from("/tmp/reap-chroot"),
                makepkg_flags: vec!["-s".to_string(), "--noconfirm".to_string()],
                clean_after_build: true,
            },
            security: SecurityConfig {
                verify_signatures: true,
                strict_mode: false,
                scan_pkgbuilds: true,
                trust_threshold: 7.0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub backend_order: Vec<String>,
    pub auto_resolve_deps: bool,
    pub noconfirm: bool,
    pub log_verbose: bool,
    pub theme: Option<String>,
    pub show_tips: Option<bool>,
    pub enable_cache: Option<bool>,
    pub enable_lua_hooks: Option<bool>,
    pub devel: Option<DevelGlobalConfig>,
    pub build: Option<BuildGlobalConfig>,
    pub security: Option<SecurityGlobalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelGlobalConfig {
    pub auto_check: Option<bool>,
    pub check_interval_hours: Option<u64>,
    pub vcs_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildGlobalConfig {
    pub use_chroot: Option<bool>,
    pub chroot_dir: Option<PathBuf>,
    pub makepkg_flags: Option<Vec<String>>,
    pub clean_after_build: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityGlobalConfig {
    pub verify_signatures: Option<bool>,
    pub strict_mode: Option<bool>,
    pub scan_pkgbuilds: Option<bool>,
    pub trust_threshold: Option<f64>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            backend_order: vec![
                "tap".to_string(),
                "aur".to_string(),
                "pacman".to_string(),
                "flatpak".to_string(),
            ],
            auto_resolve_deps: true,
            noconfirm: true,
            log_verbose: true,
            theme: Some("dark".to_string()),
            show_tips: Some(false),
            enable_cache: Some(true),
            enable_lua_hooks: Some(false),
            devel: None,
            build: None,
            security: None,
        }
    }
}

impl GlobalConfig {
    pub fn load() -> Self {
        let config_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap/reap.toml");

        if config_path.exists() {
            println!("[config] Found config at {}", config_path.display());

            if let Ok(contents) = fs::read_to_string(&config_path) {
                match toml::from_str::<GlobalConfig>(&contents) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        eprintln!("[config] Failed to parse: {e}");
                    }
                }
            }
        }

        println!("[config] Using default config.");
        GlobalConfig::default()
    }
}

pub fn get_config_key(key: &str) -> Option<String> {
    let config = ReapConfig::load();
    match key {
        "parallel" => Some(config.parallel.to_string()),
        "devel.auto_check" => Some(config.devel.auto_check.to_string()),
        "devel.check_interval_hours" => Some(config.devel.check_interval_hours.to_string()),
        "build.use_chroot" => Some(config.build.use_chroot.to_string()),
        "build.clean_after_build" => Some(config.build.clean_after_build.to_string()),
        "security.verify_signatures" => Some(config.security.verify_signatures.to_string()),
        "security.strict_mode" => Some(config.security.strict_mode.to_string()),
        "security.trust_threshold" => Some(config.security.trust_threshold.to_string()),
        _ => {
            let path = config_path();
            if path.exists() {
                if let Some(Ok(doc)) = fs::read_to_string(&path)
                    .ok()
                    .map(|s| s.parse::<DocumentMut>())
                {
                    if let Some(val) = doc.get(key) {
                        return Some(val.to_string());
                    }
                }
            }
            None
        }
    }
}

pub fn set_config_key(key: &str, value_str: &str) {
    let path = config_path();
    let mut doc = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.parse::<DocumentMut>().ok())
            .unwrap_or_default()
    } else {
        DocumentMut::new()
    };

    // Create nested structure for dotted keys
    let keys: Vec<&str> = key.split('.').collect();
    if keys.len() == 2 {
        if !doc.contains_key(keys[0]) {
            doc[keys[0]] = toml_edit::table();
        }
        if let Some(table) = doc[keys[0]].as_table_mut() {
            table[keys[1]] = value(value_str);
        }
    } else {
        doc[key] = value(value_str);
    }

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let _ = fs::write(&path, doc.to_string());
    println!("[config] Set {} = {}", key, value_str);
}

pub fn reset_config() {
    let path = config_path();
    let _ = fs::write(&path, toml::to_string(&GlobalConfig::default()).unwrap());
    println!("[config] Configuration reset to defaults");
}

pub fn show_config() {
    let path = config_path();
    if path.exists() {
        if let Ok(contents) = fs::read_to_string(&path) {
            println!("{}", contents);
        }
    } else {
        println!("No config file found at {}", path.display());
        println!("Current runtime configuration:");
        let config = ReapConfig::load();
        println!("  parallel_jobs: {}", config.parallel);
        println!("  devel.auto_check: {}", config.devel.auto_check);
        println!("  build.use_chroot: {}", config.build.use_chroot);
        println!("  security.verify_signatures: {}", config.security.verify_signatures);
    }
}

pub fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("reap/reap.toml")
}

// Config precedence: CLI flag > env vars > user config > global config > defaults