use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use toml_edit::{DocumentMut, value};

#[derive(Debug, Clone)]
pub struct ReapConfig {
    /// Packages to ignore during upgrades
    pub ignored_packages: Vec<String>,
    /// Number of parallel jobs for install/upgrade
    pub parallel: usize,
}

impl ReapConfig {
    pub fn load() -> Self {
        let global = GlobalConfig::load();
        ReapConfig {
            ignored_packages: vec![],
            parallel: global
                .enable_cache
                .map(|x| if x { 4 } else { 2 })
                .unwrap_or(2),
        }
    }
    /// Check if a package is ignored (used in upgrade/install logic)
    pub fn is_ignored(&self, pkg: &str) -> bool {
        self.ignored_packages.iter().any(|p| p == pkg)
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
    doc[key] = value(value_str);
    let _ = fs::write(&path, doc.to_string());
}

pub fn get_config_key(key: &str) -> Option<String> {
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

pub fn reset_config() {
    let path = config_path();
    let _ = fs::write(&path, toml::to_string(&GlobalConfig::default()).unwrap());
}

pub fn show_config() {
    let path = config_path();
    if path.exists() {
        if let Ok(contents) = fs::read_to_string(&path) {
            println!("{}", contents);
        }
    } else {
        println!("No config file found at {}", path.display());
    }
}

pub fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("reap/reap.toml")
}

// Config precedence: CLI flag > ~/.config/reap/reap.toml > default
