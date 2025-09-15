# 🔌 Reaper API Reference

This document provides API reference for developers working with or extending Reaper.

## 🛡️ Trust Engine API

### TrustEngine
```rust
use reaper::trust::TrustEngine;

let trust_engine = TrustEngine::new();

// Analyze package trust
let trust_score = trust_engine.compute_trust_score("firefox", &Source::Aur).await;
println!("Trust: {}", trust_engine.display_trust_badge(trust_score.overall_score));

// Cache management
trust_engine.cache_trust_score(&trust_score)?;
let cached = trust_engine.get_cached_trust_score("firefox");
```

### TrustScore Structure
```rust
pub struct TrustScore {
    pub package: String,
    pub signature_valid: bool,
    pub publisher_verified: bool,
    pub community_votes: u32,
    pub maintainer_reputation: f32,
    pub last_audit_date: Option<DateTime<Utc>>,
    pub security_flags: Vec<SecurityFlag>,
    pub overall_score: f32, // 0.0 - 10.0
}
```

### Security Flags
```rust
pub enum SecurityFlag {
    UnverifiedSignature,    // No valid PGP signature
    UnknownPublisher,       // Publisher not verified
    RecentVulnerability,    // Known security issues
    SuspiciousFiles,        // Suspicious file operations
    NetworkAccess,          // Makes network requests
    SystemAccess,           // Requires system permissions
    OutdatedDependencies,   // Dependencies have vulnerabilities
}
```

## 👤 Profile Management API

### ProfileManager
```rust
use reaper::profiles::{ProfileManager, ProfileConfig};

let mut manager = ProfileManager::new();

// Create profiles
let dev_profile = profiles::create_developer_profile();
manager.create_profile(&dev_profile)?;

// Switch profiles
manager.switch_profile("developer")?;
let active = manager.get_active_profile()?;

// List and manage
let profiles = manager.list_profiles()?;
manager.delete_profile("old-profile")?;
```

### ProfileConfig Structure
```rust
pub struct ProfileConfig {
    pub name: String,
    pub backend_order: Vec<String>,         // ["tap", "aur", "flatpak"]
    pub auto_install_deps: Vec<String>,     // Auto-installed dependencies
    pub pinned_packages: Vec<String>,       // Packages to never upgrade
    pub ignored_packages: Vec<String>,      // Packages to ignore
    pub parallel_jobs: Option<usize>,       // Parallel build jobs
    pub fast_mode: Option<bool>,            // Skip some verification
    pub strict_signatures: Option<bool>,    // Require valid signatures
    pub auto_resolve_deps: Option<bool>,    // Automatic dependency resolution
}
```

## 🔧 Enhanced AUR API

### EnhancedAurManager
```rust
use reaper::enhanced_aur::{EnhancedAurManager, PkgbuildInfo};

let mut aur = EnhancedAurManager::new();

// Fetch and parse PKGBUILD
let pkgbuild = aur.fetch_pkgbuild("firefox").await?;
println!("Dependencies: {:?}", pkgbuild.dependencies);

// Conflict detection
let conflicts = aur.resolve_dependencies_advanced(&["firefox", "firefox-esr"]).await?;
for conflict in conflicts {
    println!("Conflict: {:?}", conflict);
}

// Interactive editing
aur.edit_pkgbuild("custom-package")?;
```

### PkgbuildInfo Structure
```rust
pub struct PkgbuildInfo {
    pub package: String,
    pub version: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub make_dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub provides: Vec<String>,
    pub source_files: Vec<String>,
    pub integrity_checks: Vec<String>,
}
```

### Dependency Conflict Types
```rust
pub enum ConflictType {
    FileConflict(String),           // File path conflict
    PackageConflict,                // Direct package conflict
    VersionConflict(String, String), // Version requirement conflict
    CircularDependency,             // Circular dependency loop
}
```

## ⭐ Interactive & Rating API

### InteractiveManager
```rust
use reaper::interactive::{InteractiveManager, PackageRating};

let mut interactive = InteractiveManager::new();

// Get package ratings
let rating = interactive.get_package_rating("firefox").await?;
println!("Rating: {}", interactive.display_rating(&rating));

// User interactions
let confirmed = interactive.confirm_action("Continue?", true);
let choice = interactive.select_from_list(&options, "Choose package:");

// Submit ratings
interactive.submit_user_rating("firefox", 5, Some("Great browser!".to_string()))?;
```

### PackageRating Structure
```rust
pub struct PackageRating {
    pub package: String,
    pub aur_votes: u32,
    pub aur_popularity: f64,
    pub user_rating: Option<u8>,        // 1-5 stars
    pub community_rating: f64,
    pub reviews: Vec<PackageReview>,
    pub last_updated: DateTime<Utc>,
}
```

## 🎨 TUI Integration API

### TUI Components
```rust
use reaper::tui::{SearchTab, BuildProgress, LogPane};

// Search with trust and ratings
let mut search_tab = SearchTab::new();
search_tab.do_search(&mut interactive_manager).await;
search_tab.render_with_ratings(frame, area, &trust_engine, &interactive_manager);

// Build progress monitoring
let mut progress = BuildProgress::new();
progress.update().await;

// Logging system
let log_pane = Arc::new(LogPane::new());
log_pane.push("[info] Operation completed");
```

## 🔄 Backend Integration API

### Backend Trait Implementation
```rust
use reaper::backend::{Backend, SearchResult, AuditResult};
use async_trait::async_trait;

pub struct CustomBackend;

#[async_trait]
impl Backend for CustomBackend {
    async fn search(&self, query: &str) -> Vec<SearchResult> {
        // Custom search implementation
        vec![]
    }
    
    async fn install(&self, pkg: &str) -> Result<()> {
        // Custom install implementation
        Ok(())
    }
    
    async fn audit(&self, pkg: &str) -> AuditResult {
        // Custom audit implementation
        AuditResult::default()
    }
}
```

## 🔧 Configuration API

### Config Management
```rust
use reaper::config::ReapConfig;

// Load configuration
let config = ReapConfig::load();

// Modify settings
let mut config = config;
config.parallel = 8;
config.fast_mode = true;
config.save()?;

// Profile-specific config
let profile_config = profile_manager.get_active_profile()?;
let effective_parallel = profile_config.parallel_jobs.unwrap_or(config.parallel);
```

## 🔐 GPG Verification API

### GPG Operations
```rust
use reaper::gpg;

// Verify package signature
let is_valid = gpg::verify_pkgbuild(&package_path);

// Key management
gpg::refresh_keys().await?;
gpg::import_key(&key_data)?;
```

## 🪝 Hooks System API

### Hook Implementation
```rust
use reaper::hooks::{HookContext, pre_install, post_install};

// Hook context
let ctx = HookContext {
    pkg: "firefox".to_string(),
    version: Some("95.0".to_string()),
    source: Some("aur".to_string()),
    install_path: Some("/usr/bin/firefox".into()),
    tap: None,
};

// Execute hooks
pre_install(&ctx);
// ... installation logic ...
post_install(&ctx);
```

## 📊 Utilities API

### Common Utilities
```rust
use reaper::utils;

// Package management
utils::pin_package("firefox")?;
utils::unpin_package("firefox")?;
let pinned = utils::get_pinned_packages();

// File operations
let size = utils::get_package_size("firefox")?;
utils::cleanup_cache()?;

// System integration
let packages = utils::get_installed_packages();
utils::sync_databases().await?;
```

## 🚀 Async Operations

### Async Patterns
```rust
use tokio;

// Concurrent operations
let results = tokio::join!(
    trust_engine.compute_trust_score("firefox", &Source::Aur),
    aur_manager.fetch_pkgbuild("firefox"),
    interactive.get_package_rating("firefox")
);

// Parallel processing
let futures: Vec<_> = packages.iter()
    .map(|pkg| trust_engine.compute_trust_score(pkg, &Source::Aur))
    .collect();
let scores = futures::future::join_all(futures).await;
```

## 🔧 Error Handling

### Error Types
```rust
use reaper::errors::{ReapError, TrustError, ProfileError};

// Result types
type Result<T> = std::result::Result<T, ReapError>;

// Error handling
match operation() {
    Ok(result) => println!("Success: {:?}", result),
    Err(ReapError::TrustError(e)) => eprintln!("Trust error: {}", e),
    Err(ReapError::ProfileError(e)) => eprintln!("Profile error: {}", e),
    Err(e) => eprintln!("Other error: {}", e),
}
```

This API reference provides the foundation for extending Reaper with custom functionality while maintaining compatibility with the core security and profile systems.