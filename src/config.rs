use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use toml_edit::{DocumentMut, value};

/// Unified configuration for Reaper.
///
/// # Loading Precedence (lowest to highest)
///
/// 1. Built-in defaults (`Config::default()`)
/// 2. Config file (`~/.config/reaper/reap.toml`)
/// 3. Active profile overrides (`~/.config/reaper/profiles/<name>.toml`)
/// 4. Environment variable overrides (`REAP_*`)
/// 5. CLI flag overrides (handled at call site)
///
/// # Example Config File
///
/// ```toml
/// backend_order = ["tap", "aur", "pacman"]
/// auto_resolve_deps = true
/// parallel = 4
/// noconfirm = false
///
/// [devel]
/// auto_check = true
/// check_interval_hours = 24
///
/// [build]
/// use_chroot = false
/// clean_after_build = true
///
/// [security]
/// verify_signatures = true
/// trust_threshold = 7.0
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    // === Source Resolution ===
    /// Order of package sources to try (tap, aur, pacman, flatpak)
    pub backend_order: Vec<String>,
    /// Automatically resolve and install dependencies
    pub auto_resolve_deps: bool,

    // === Install Behavior ===
    /// Skip confirmation prompts
    pub noconfirm: bool,
    /// Number of parallel jobs for install/upgrade
    pub parallel: usize,
    /// Packages to ignore during upgrades
    pub ignored_packages: Vec<String>,
    /// Pinned packages (don't upgrade)
    pub pinned_packages: Vec<String>,

    // === UI/UX ===
    /// Verbose logging
    pub log_verbose: bool,
    /// Color theme (dark, light)
    pub theme: String,
    /// Show tips at startup
    pub show_tips: bool,

    // === Feature Flags ===
    /// Enable caching
    pub enable_cache: bool,
    /// Enable Lua hooks (experimental)
    pub enable_lua_hooks: bool,

    // === Subsystem Configs ===
    /// Development package tracking
    pub devel: DevelConfig,
    /// Build configuration
    pub build: BuildConfig,
    /// Security settings
    pub security: SecurityConfig,

    // === Profile ===
    /// Currently active profile name (None = default behavior)
    pub active_profile: Option<String>,
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
    pub trust_cache_ttl_hours: i64,
}

impl Config {
    /// Load configuration with full precedence chain.
    ///
    /// Order: defaults → config file → active profile → env vars
    pub fn load() -> Self {
        Self::load_from_sources().unwrap_or_else(|e| {
            eprintln!("[config] Warning: Failed to load config: {}", e);
            Self::default()
        })
    }

    /// Load configuration from all sources with proper precedence.
    pub fn load_from_sources() -> Result<Self> {
        let mut config = Self::default();

        // 1. Load from user config file
        let user_config_path = crate::paths::USER_CONFIG.clone();
        if user_config_path.exists()
            && let Ok(content) = fs::read_to_string(&user_config_path)
            && let Ok(file_config) = toml::from_str::<ConfigFile>(&content)
        {
            config.merge_file_config(file_config);
        }

        // 2. Load active profile and apply overrides
        config.apply_active_profile();

        // 3. Apply environment variable overrides
        config.apply_env_overrides();

        // 4. Validate final configuration
        config.validate()?;

        Ok(config)
    }

    /// Check if a package is ignored (for upgrade logic)
    pub fn is_ignored(&self, pkg: &str) -> bool {
        self.ignored_packages.iter().any(|p| p == pkg)
    }

    /// Check if a package is pinned (for upgrade logic)
    #[allow(dead_code)]
    pub fn is_pinned(&self, pkg: &str) -> bool {
        self.pinned_packages.iter().any(|p| p == pkg)
    }

    fn merge_file_config(&mut self, file: ConfigFile) {
        // Core settings
        if let Some(order) = file.backend_order {
            self.backend_order = order;
        }
        if let Some(resolve) = file.auto_resolve_deps {
            self.auto_resolve_deps = resolve;
        }
        if let Some(nc) = file.noconfirm {
            self.noconfirm = nc;
        }
        if let Some(parallel) = file.parallel {
            self.parallel = parallel;
        }
        if let Some(ignored) = file.ignored_packages {
            self.ignored_packages = ignored;
        }
        if let Some(pinned) = file.pinned_packages {
            self.pinned_packages = pinned;
        }
        if let Some(verbose) = file.log_verbose {
            self.log_verbose = verbose;
        }
        if let Some(theme) = file.theme {
            self.theme = theme;
        }
        if let Some(tips) = file.show_tips {
            self.show_tips = tips;
        }
        if let Some(cache) = file.enable_cache {
            self.enable_cache = cache;
        }
        if let Some(lua) = file.enable_lua_hooks {
            self.enable_lua_hooks = lua;
        }

        // Devel settings
        if let Some(devel) = file.devel {
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

        // Build settings
        if let Some(build) = file.build {
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

        // Security settings
        if let Some(security) = file.security {
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
            if let Some(ttl) = security.trust_cache_ttl_hours {
                self.security.trust_cache_ttl_hours = ttl;
            }
        }
    }

    fn apply_active_profile(&mut self) {
        let profiles_dir = crate::paths::profiles_dir();
        let active_path = profiles_dir.join(".active");

        // Read active profile name from marker file
        let profile_name = if active_path.exists() {
            fs::read_to_string(&active_path)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "default".to_string())
        } else {
            "default".to_string()
        };

        // Skip if default (no overrides needed)
        if profile_name == "default" {
            return;
        }

        // Load profile file
        let profile_path = profiles_dir.join(format!("{}.toml", profile_name));
        if !profile_path.exists() {
            return;
        }

        if let Ok(content) = fs::read_to_string(&profile_path)
            && let Ok(profile) = toml::from_str::<ProfileFile>(&content)
        {
            self.active_profile = Some(profile_name);

            // Apply profile overrides
            if !profile.backend_order.is_empty() {
                self.backend_order = profile.backend_order;
            }
            if !profile.ignored_packages.is_empty() {
                // Merge with existing ignored packages
                for pkg in profile.ignored_packages {
                    if !self.ignored_packages.contains(&pkg) {
                        self.ignored_packages.push(pkg);
                    }
                }
            }
            if !profile.pinned_packages.is_empty() {
                for pkg in profile.pinned_packages {
                    if !self.pinned_packages.contains(&pkg) {
                        self.pinned_packages.push(pkg);
                    }
                }
            }
            if let Some(parallel) = profile.parallel_jobs {
                self.parallel = parallel;
            }
            if let Some(strict) = profile.strict_signatures {
                self.security.strict_mode = strict;
            }
            if let Some(resolve) = profile.auto_resolve_deps {
                self.auto_resolve_deps = resolve;
            }
        }
    }

    fn apply_env_overrides(&mut self) {
        // REAP_PARALLEL_JOBS
        if let Ok(parallel) = std::env::var("REAP_PARALLEL_JOBS")
            && let Ok(jobs) = parallel.parse::<usize>()
        {
            self.parallel = jobs;
        }

        // REAP_IGNORED_PACKAGES (comma-separated)
        if let Ok(ignored) = std::env::var("REAP_IGNORED_PACKAGES") {
            self.ignored_packages = ignored.split(',').map(|s| s.trim().to_string()).collect();
        }

        // REAP_BACKEND_ORDER (comma-separated)
        if let Ok(order) = std::env::var("REAP_BACKEND_ORDER") {
            self.backend_order = order.split(',').map(|s| s.trim().to_string()).collect();
        }

        // REAP_USE_CHROOT
        if let Ok(use_chroot) = std::env::var("REAP_USE_CHROOT") {
            self.build.use_chroot = use_chroot.parse().unwrap_or(false);
        }

        // REAP_STRICT_MODE
        if let Ok(strict_mode) = std::env::var("REAP_STRICT_MODE") {
            self.security.strict_mode = strict_mode.parse().unwrap_or(false);
        }

        // REAP_NOCONFIRM
        if let Ok(noconfirm) = std::env::var("REAP_NOCONFIRM") {
            self.noconfirm = noconfirm.parse().unwrap_or(false);
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

        if !self.build.chroot_dir.exists()
            && self.build.use_chroot
            && let Err(e) = fs::create_dir_all(&self.build.chroot_dir)
        {
            return Err(anyhow!("Cannot create chroot directory: {}", e));
        }

        Ok(())
    }
}

impl Default for Config {
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
            parallel: 2,
            ignored_packages: vec![],
            pinned_packages: vec![],
            log_verbose: true,
            theme: "dark".to_string(),
            show_tips: false,
            enable_cache: true,
            enable_lua_hooks: false,
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
                chroot_dir: crate::paths::chroot_dir(),
                makepkg_flags: vec!["-s".to_string(), "--noconfirm".to_string()],
                clean_after_build: true,
            },
            security: SecurityConfig {
                verify_signatures: true,
                strict_mode: false,
                scan_pkgbuilds: true,
                trust_threshold: 7.0,
                trust_cache_ttl_hours: 24,
            },
            active_profile: None,
        }
    }
}

// === Config File Schema (TOML deserialization) ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    pub backend_order: Option<Vec<String>>,
    pub auto_resolve_deps: Option<bool>,
    pub noconfirm: Option<bool>,
    pub parallel: Option<usize>,
    pub ignored_packages: Option<Vec<String>>,
    pub pinned_packages: Option<Vec<String>>,
    pub log_verbose: Option<bool>,
    pub theme: Option<String>,
    pub show_tips: Option<bool>,
    pub enable_cache: Option<bool>,
    pub enable_lua_hooks: Option<bool>,
    pub devel: Option<DevelFileConfig>,
    pub build: Option<BuildFileConfig>,
    pub security: Option<SecurityFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelFileConfig {
    pub auto_check: Option<bool>,
    pub check_interval_hours: Option<u64>,
    pub vcs_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildFileConfig {
    pub use_chroot: Option<bool>,
    pub chroot_dir: Option<PathBuf>,
    pub makepkg_flags: Option<Vec<String>>,
    pub clean_after_build: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFileConfig {
    pub verify_signatures: Option<bool>,
    pub strict_mode: Option<bool>,
    pub scan_pkgbuilds: Option<bool>,
    pub trust_threshold: Option<f64>,
    pub trust_cache_ttl_hours: Option<i64>,
}

// === Profile File Schema ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub backend_order: Vec<String>,
    #[serde(default)]
    pub auto_install_deps: Vec<String>,
    #[serde(default)]
    pub pinned_packages: Vec<String>,
    #[serde(default)]
    pub ignored_packages: Vec<String>,
    pub parallel_jobs: Option<usize>,
    pub fast_mode: Option<bool>,
    pub strict_signatures: Option<bool>,
    pub auto_resolve_deps: Option<bool>,
}

// === Legacy Aliases (for backward compatibility during transition) ===

/// Alias for backward compatibility. Use `Config` instead.
#[allow(dead_code)]
pub type ReapConfig = Config;

/// Alias for backward compatibility. Use `Config` instead.
#[allow(dead_code)]
pub type GlobalConfig = Config;

// === Config CLI Helpers ===

pub fn get_config_key(key: &str) -> Option<String> {
    let config = Config::load();
    match key {
        "parallel" => Some(config.parallel.to_string()),
        "backend_order" => Some(config.backend_order.join(",")),
        "auto_resolve_deps" => Some(config.auto_resolve_deps.to_string()),
        "noconfirm" => Some(config.noconfirm.to_string()),
        "devel.auto_check" => Some(config.devel.auto_check.to_string()),
        "devel.check_interval_hours" => Some(config.devel.check_interval_hours.to_string()),
        "build.use_chroot" => Some(config.build.use_chroot.to_string()),
        "build.clean_after_build" => Some(config.build.clean_after_build.to_string()),
        "security.verify_signatures" => Some(config.security.verify_signatures.to_string()),
        "security.strict_mode" => Some(config.security.strict_mode.to_string()),
        "security.trust_threshold" => Some(config.security.trust_threshold.to_string()),
        "security.trust_cache_ttl_hours" => Some(config.security.trust_cache_ttl_hours.to_string()),
        _ => {
            // Fallback: try to read directly from TOML
            let path = config_path();
            if path.exists()
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(doc) = content.parse::<DocumentMut>()
                && let Some(val) = doc.get(key)
            {
                return Some(val.to_string());
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

    // Map dotted keys to their actual structure in ConfigFile
    // The ConfigFile uses flat top-level keys and nested sections for devel/build/security
    let (section, actual_key) = match key {
        // Flat top-level keys (map common aliases)
        "parallel" | "install.parallel" => (None, "parallel"),
        "noconfirm" | "install.noconfirm" => (None, "noconfirm"),
        "backend_order" | "sources.backend_order" => (None, "backend_order"),
        "auto_resolve_deps" | "sources.auto_resolve_deps" => (None, "auto_resolve_deps"),
        "log_verbose" => (None, "log_verbose"),
        "theme" => (None, "theme"),
        "show_tips" => (None, "show_tips"),
        "enable_cache" => (None, "enable_cache"),
        // Nested sections
        "devel.auto_check" => (Some("devel"), "auto_check"),
        "devel.check_interval_hours" => (Some("devel"), "check_interval_hours"),
        "build.use_chroot" => (Some("build"), "use_chroot"),
        "build.clean_after_build" => (Some("build"), "clean_after_build"),
        "security.verify_signatures" => (Some("security"), "verify_signatures"),
        "security.strict_mode" => (Some("security"), "strict_mode"),
        "security.trust_threshold" => (Some("security"), "trust_threshold"),
        "security.trust_cache_ttl_hours" => (Some("security"), "trust_cache_ttl_hours"),
        // Unknown key - try as top-level
        _ => (None, key),
    };

    // Parse value to appropriate type
    let typed_value: toml_edit::Item = if let Ok(n) = value_str.parse::<i64>() {
        value(n)
    } else if let Ok(f) = value_str.parse::<f64>() {
        value(f)
    } else if value_str == "true" {
        value(true)
    } else if value_str == "false" {
        value(false)
    } else {
        value(value_str)
    };

    // Set the value in the appropriate location
    if let Some(section_name) = section {
        if !doc.contains_key(section_name) {
            doc[section_name] = toml_edit::table();
        }
        if let Some(table) = doc[section_name].as_table_mut() {
            table[actual_key] = typed_value;
        }
    } else {
        doc[actual_key] = typed_value;
    }

    // Ensure directory exists
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        eprintln!("[config] Failed to create config directory: {}", e);
        return;
    }

    match fs::write(&path, doc.to_string()) {
        Ok(()) => println!("[config] Set {} = {} (saved to {:?})", key, value_str, path),
        Err(e) => eprintln!("[config] Failed to write config file: {}", e),
    }
}

pub fn reset_config() {
    let path = config_path();
    let default_config = ConfigFile {
        backend_order: Some(vec![
            "tap".to_string(),
            "aur".to_string(),
            "pacman".to_string(),
            "flatpak".to_string(),
        ]),
        auto_resolve_deps: Some(true),
        noconfirm: Some(true),
        parallel: Some(2),
        log_verbose: Some(true),
        theme: Some("dark".to_string()),
        show_tips: Some(false),
        enable_cache: Some(true),
        enable_lua_hooks: Some(false),
        ..Default::default()
    };
    let _ = fs::write(&path, toml::to_string_pretty(&default_config).unwrap());
    println!("[config] Configuration reset to defaults");
}

pub fn show_config() {
    let path = config_path();
    let config = Config::load();

    println!("=== Reaper Configuration ===\n");

    if let Some(profile) = &config.active_profile {
        println!("Active Profile: {}", profile);
    } else {
        println!("Active Profile: (default)");
    }
    println!();

    println!("[sources]");
    println!("  backend_order = {:?}", config.backend_order);
    println!("  auto_resolve_deps = {}", config.auto_resolve_deps);
    println!();

    println!("[install]");
    println!("  parallel = {}", config.parallel);
    println!("  noconfirm = {}", config.noconfirm);
    if !config.ignored_packages.is_empty() {
        println!("  ignored_packages = {:?}", config.ignored_packages);
    }
    if !config.pinned_packages.is_empty() {
        println!("  pinned_packages = {:?}", config.pinned_packages);
    }
    println!();

    println!("[devel]");
    println!("  auto_check = {}", config.devel.auto_check);
    println!(
        "  check_interval_hours = {}",
        config.devel.check_interval_hours
    );
    println!();

    println!("[build]");
    println!("  use_chroot = {}", config.build.use_chroot);
    println!("  clean_after_build = {}", config.build.clean_after_build);
    println!();

    println!("[security]");
    println!(
        "  verify_signatures = {}",
        config.security.verify_signatures
    );
    println!("  strict_mode = {}", config.security.strict_mode);
    println!("  trust_threshold = {}", config.security.trust_threshold);
    println!(
        "  trust_cache_ttl_hours = {}",
        config.security.trust_cache_ttl_hours
    );
    println!();

    println!("Config file: {}", path.display());
}

pub fn config_path() -> std::path::PathBuf {
    crate::paths::USER_CONFIG.clone()
}
