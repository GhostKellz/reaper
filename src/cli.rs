use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "reap",
    version = "0.6.0",
    about = "Reaper: Secure, unified Rust-powered meta package manager\n\nUSAGE EXAMPLES:\n  reap install <pkg> --fast\n  reap install <pkg> --strict\n  reap install <pkg> --insecure\n  reap tap add mytap https://github.com/me/mytap.git\n  reap doctor --fix\n\nConfig precedence: CLI flag > ~/.config/reap/reap.toml > default\nSee README.md for more.",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    #[arg(short = 'S', long = "sync", value_name = "PKG", num_args = 0.., conflicts_with_all = ["remove", "search", "local"])]
    pub sync: Option<Vec<String>>,
    #[arg(short = 'R', long = "remove", value_name = "PKG", num_args = 0.., conflicts_with_all = ["sync", "search", "upgrade", "local"], help = "Remove a package")]
    pub remove: Option<Vec<String>>,
    #[arg(long = "local", value_name = "PATH", num_args = 0.., conflicts_with_all = ["sync", "remove", "search", "upgrade"])]
    pub local: Option<Vec<String>>,
    #[arg(short = 'Q', long = "search", value_name = "TERM", num_args = 0.., conflicts_with_all = ["sync", "remove", "upgrade", "local"], help = "Search for a package")]
    pub search: Option<Vec<String>>,
    #[arg(short = 'y', long = "refresh", help = "Refresh package database")]
    pub refresh: bool,
    #[arg(short = 'u', long = "upgrade", help = "Upgrade packages")]
    pub upgrade: bool,
    #[arg(long = "syncdb", help = "Sync package database")]
    pub syncdb: bool,
    #[arg(short = 'U', long = "upgradeall", help = "Upgrade all packages")]
    pub upgradeall: bool,
    #[arg(long = "install", value_name = "PKG", help = "Install a package")]
    pub install: Option<String>,
    #[arg(
        long = "backend",
        value_name = "BACKEND",
        default_value = "aur",
        help = "Select backend: aur, flatpak"
    )]
    pub backend: String,
    #[arg(long = "edit", help = "Edit PKGBUILD before building")]
    pub edit: bool,
    #[arg(long = "noconfirm", help = "Skip confirmation prompts")]
    pub noconfirm: bool,
    #[arg(long = "dry-run", help = "Show what would be done, but do not install")]
    pub dry_run: bool,
    #[arg(
        long = "downgrade",
        value_name = "PKG=VER",
        help = "Downgrade package to a specific version"
    )]
    pub downgrade: Option<String>,
    #[arg(long = "diff", help = "Show PKGBUILD diff before install/upgrade")]
    pub diff: bool,
    #[arg(
        long = "resolve-deps",
        help = "Automatically install missing dependencies before build"
    )]
    pub resolve_deps: bool,
    #[arg(
        long = "insecure",
        help = "Skip GPG verification for tap installs (not recommended)"
    )]
    pub insecure: bool,
    #[arg(
        long = "gpg-keyserver",
        value_name = "URL",
        help = "Set GPG keyserver for key fetch"
    )]
    pub gpg_keyserver: Option<String>,
    #[arg(long = "audit", help = "Audit/log actions without executing them")]
    pub audit: bool,
    #[arg(long = "yes", help = "Assume yes for all prompts (non-interactive)")]
    pub yes: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install a package
    Install {
        pkg: String,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        binary_only: bool,
        #[arg(long)]
        diff: bool,
    },
    /// Install multiple packages in parallel
    BatchInstall {
        pkgs: Vec<String>,
        #[arg(long)]
        parallel: bool,
    },
    /// Remove one or more packages
    Remove { pkgs: Vec<String> },
    /// Install local packages
    Local { pkgs: Vec<String> },
    /// Search for packages
    Search { terms: Vec<String> },
    /// Check for package updates
    Update,
    /// Upgrade all packages
    Upgrade { parallel: bool },
    /// Parallel upgrade specific packages
    ParallelUpgrade { pkgs: Vec<String> },
    /// Upgrade all packages
    UpgradeAll,
    /// Upgrade Flatpak packages
    FlatpakUpgrade,
    /// Audit a package
    Audit { pkg: String },
    /// Rollback a package
    Rollback { pkg: String },
    /// Sync package database
    SyncDb,
    /// Pin a package
    Pin { pkg: String },
    /// Launch the interactive TUI
    Tui,
    /// Clean package cache
    Clean,
    /// Run system doctor
    Doctor,
    /// Performance and caching operations
    Perf {
        #[command(subcommand)]
        cmd: PerfCmd,
    },
    /// Security operations
    Security {
        #[command(subcommand)]
        cmd: SecurityCmd,
    },
    /// GPG key refresh
    Gpg {
        #[command(subcommand)]
        cmd: GpgCmd,
    },
    /// Flatpak commands
    Flatpak {
        #[command(subcommand)]
        cmd: FlatpakCmd,
    },
    /// Tap repository management
    Tap {
        #[command(subcommand)]
        cmd: TapCmd,
    },
    /// Generate shell completion
    Completion { shell: String },
    /// Backup current config to backup directory
    Backup,
    /// List orphaned packages
    Orphan {
        #[arg(long = "remove", help = "Uninstall orphaned packages")]
        remove: bool,
        #[arg(long = "all", help = "Include orphaned pacman packages, not just AUR")]
        all: bool,
    },
    /// Manage global configuration
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Profile management
    Profile {
        #[command(subcommand)]
        cmd: ProfileCmd,
    },
    /// Trust and security analysis
    Trust {
        #[command(subcommand)]
        cmd: TrustCmd,
    },
    /// Rate a package
    Rate {
        pkg: String,
        #[arg(short, long, help = "Rating from 1-5 stars")]
        rating: u8,
        #[arg(short, long, help = "Optional comment")]
        comment: Option<String>,
    },
    /// Enhanced AUR operations
    Aur {
        #[command(subcommand)]
        cmd: AurCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum FlatpakCmd {
    Search { query: String },
    Install { pkg: String },
    Remove { pkg: String },
    Update,
    List,
    Upgrade,
    Audit { pkg: String },
}

#[derive(Subcommand, Debug)]
pub enum GpgCmd {
    Refresh,
    Import { keyid: String },
    Show { keyid: String },
    Check { keyid: String },
    VerifyPkgbuild { path: String },
    SetKeyserver { url: String },
    CheckKeyserver { url: String },
}

#[derive(Subcommand, Debug)]
pub enum TapCmd {
    Add {
        name: String,
        url: String,
        #[arg(long)]
        priority: u32,
    },
    Remove {
        name: String,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Update,
    Sync,
    List,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Set a config key
    Set { key: String, value: String },
    /// Get a config key
    Get { key: String },
    /// Reset config to defaults
    Reset,
    /// Show full config
    Show,
}

#[derive(Subcommand, Debug)]
pub enum ProfileCmd {
    /// Create a new profile
    Create {
        name: String,
        #[arg(long, help = "Use predefined template (developer, gaming, minimal)")]
        template: Option<String>,
    },
    /// Switch to a profile
    Switch { name: String },
    /// List all profiles
    List,
    /// Show profile details
    Show { name: String },
    /// Delete a profile
    Delete { name: String },
    /// Edit profile settings
    Edit {
        name: String,
        #[arg(long)]
        backend_order: Option<String>,
        #[arg(long)]
        parallel_jobs: Option<usize>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TrustCmd {
    /// Analyze package trust score
    Score { pkg: String },
    /// Scan all installed packages
    Scan,
    /// Show trust statistics
    Stats,
    /// Update trust database
    Update,
}

#[derive(Subcommand, Debug)]
pub enum AurCmd {
    /// Fetch and analyze PKGBUILD
    Fetch { pkg: String },
    /// Edit PKGBUILD interactively
    Edit { pkg: String },
    /// Check dependencies and conflicts
    Deps {
        pkg: String,
        #[arg(long, help = "Check for conflicts")]
        conflicts: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum PerfCmd {
    /// Warm cache with popular packages
    WarmCache,
    /// Parallel search test
    ParallelSearch { queries: Vec<String> },
    /// Parallel PKGBUILD fetch
    ParallelFetch { packages: Vec<String> },
    /// Show cache statistics
    CacheStats,
    /// Clear all caches
    ClearCache,
}

#[derive(Subcommand, Debug)]
pub enum SecurityCmd {
    /// Audit PKGBUILD for security issues
    Audit { pkg: String },
    /// Scan all installed packages for security
    ScanAll,
    /// Show security statistics
    Stats,
    /// Update security rules
    UpdateRules,
}
