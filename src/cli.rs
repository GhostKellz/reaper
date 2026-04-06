use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "reap",
    version = "0.8.0",
    about = "Reaper - A fast, secure AUR helper for Arch Linux",
    long_about = "Reaper: Secure, unified Rust-powered meta package manager\n\n\
USAGE EXAMPLES:\n  \
  reap install <pkg>           Install a package\n  \
  reap search <term>           Search for packages\n  \
  reap remove <pkg>            Remove a package\n  \
  reap upgrade                 Upgrade all packages\n  \
  reap tap add <name> <url>    Add a tap repository\n  \
  reap doctor                  Run system diagnostics\n\n\
PACMAN-STYLE FLAGS:\n  \
  reap -S <pkg>                Install packages (like pacman -S)\n  \
  reap -Ss <term>              Search packages (like pacman -Ss)\n  \
  reap -R <pkg>                Remove packages (like pacman -R)\n  \
  reap -Syu                    Sync and upgrade (like pacman -Syu)\n  \
  reap -Qu                     Query upgradable packages\n\n\
Config: ~/.config/reaper/reap.toml"
)]
pub struct Cli {
    /// Sync operation (install packages) - pacman -S style
    #[arg(short = 'S', long = "sync", help = "Sync/Install packages (pacman -S)")]
    pub sync: bool,

    /// Remove packages - pacman -R style
    #[arg(
        short = 'R',
        long = "remove-flag",
        help = "Remove packages (pacman -R)"
    )]
    pub remove_flag: bool,

    /// Query packages - pacman -Q style
    #[arg(short = 'Q', long = "query", help = "Query packages (pacman -Q)")]
    pub query: bool,

    /// Refresh database - pacman -y style
    #[arg(short = 'y', long = "refresh", help = "Refresh package database")]
    pub refresh: bool,

    /// Upgrade - pacman -u style
    #[arg(short = 'u', long = "upgrade-flag", help = "Upgrade packages")]
    pub upgrade_flag: bool,

    /// Search - pacman -s style (used with -S for -Ss)
    #[arg(short = 's', long = "search-flag", help = "Search for packages")]
    pub search_flag: bool,

    /// Clean cache - pacman -c style
    #[arg(short = 'c', long = "clean-flag", help = "Clean package cache")]
    pub clean_flag: bool,

    /// Package arguments (for pacman-style usage)
    #[arg(trailing_var_arg = true)]
    pub packages: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    /// Convert pacman-style flags to an equivalent command
    pub fn resolve_pacman_flags(&self) -> Option<PacmanAction> {
        // -Syu: sync database + upgrade
        if self.sync && self.refresh && self.upgrade_flag {
            return Some(PacmanAction::SyncUpgrade);
        }

        // -Ss: search
        if self.sync && self.search_flag {
            return Some(PacmanAction::Search(self.packages.clone()));
        }

        // -Sy: refresh database
        if self.sync && self.refresh && !self.upgrade_flag {
            return Some(PacmanAction::RefreshDb);
        }

        // -Su: upgrade only
        if self.sync && self.upgrade_flag && !self.refresh {
            return Some(PacmanAction::Upgrade);
        }

        // -S: install
        if self.sync && !self.packages.is_empty() {
            return Some(PacmanAction::Install(self.packages.clone()));
        }

        // -R: remove
        if self.remove_flag && !self.packages.is_empty() {
            return Some(PacmanAction::Remove(self.packages.clone()));
        }

        // -Qu: query upgradable
        if self.query && self.upgrade_flag {
            return Some(PacmanAction::QueryUpgradable);
        }

        // -Q: query installed
        if self.query && !self.packages.is_empty() {
            return Some(PacmanAction::QueryInstalled(self.packages.clone()));
        }

        // -Sc: clean cache
        if self.sync && self.clean_flag {
            return Some(PacmanAction::CleanCache);
        }

        None
    }
}

/// Actions derived from pacman-style flags
#[derive(Debug, Clone)]
pub enum PacmanAction {
    /// -Syu: sync and upgrade all
    SyncUpgrade,
    /// -Sy: refresh database
    RefreshDb,
    /// -Su: upgrade packages
    Upgrade,
    /// -S <pkg>: install packages
    Install(Vec<String>),
    /// -Ss <term>: search packages
    Search(Vec<String>),
    /// -R <pkg>: remove packages
    Remove(Vec<String>),
    /// -Qu: query upgradable
    QueryUpgradable,
    /// -Q <pkg>: query if installed
    QueryInstalled(Vec<String>),
    /// -Sc: clean cache
    CleanCache,
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
        #[arg(long, help = "Skip signature verification (use with caution)")]
        insecure: bool,
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
    /// Upgrade all Flatpak applications (alias for flatpak upgrade)
    FlatpakUpgrade,
    /// Audit a package
    Audit { pkg: String },
    /// Transaction rollback management
    Rollback {
        #[command(subcommand)]
        cmd: RollbackCmd,
    },
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
    /// Flatpak application management
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
    /// Debian package (.deb) handling
    Dpkg {
        #[command(subcommand)]
        cmd: DpkgCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum DpkgCmd {
    /// Show information about a .deb file
    Info {
        /// Path to the .deb file
        path: String,
    },
    /// Install a .deb file (converts to Arch package using debtap)
    Install {
        /// Path to the .deb file
        path: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum FlatpakCmd {
    /// Search for Flatpak applications
    Search {
        /// Search query
        query: String,
    },
    /// Install a Flatpak application
    Install {
        /// Application ID (e.g., org.mozilla.firefox)
        app_id: String,
        #[arg(long, help = "Also setup Flathub remote if not configured")]
        setup_flathub: bool,
    },
    /// Remove a Flatpak application
    Remove {
        /// Application ID to remove
        app_id: String,
    },
    /// Update Flatpak metadata/appstream data
    Update,
    /// List installed Flatpak applications
    List,
    /// Upgrade all Flatpak applications
    Upgrade,
    /// Show detailed information about an application
    Info {
        /// Application ID
        app_id: String,
    },
    /// Security audit of a Flatpak application's permissions
    Audit {
        /// Application ID to audit
        app_id: String,
    },
    /// List configured Flatpak remotes
    Remotes,
    /// Check for available updates
    CheckUpdates,
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
}

#[derive(Subcommand, Debug)]
pub enum TrustCmd {
    /// Analyze package trust score
    Score { pkg: String },
    /// Scan all installed AUR packages for trust scores
    Scan,
    /// Show aggregate trust statistics
    Stats,
    /// Update trust database
    Update,
}

#[derive(Subcommand, Debug)]
pub enum RollbackCmd {
    /// List recent transactions
    List {
        /// Maximum number of transactions to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Filter by package name
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Show details of a specific transaction
    Show {
        /// Transaction ID
        txid: String,
    },
    /// Preview what a rollback would do (dry-run)
    DryRun {
        /// Transaction ID to preview rollback for
        txid: String,
    },
    /// Execute a rollback to restore previous package state
    Apply {
        /// Transaction ID to rollback
        txid: String,
        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
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
    /// Scan all installed AUR packages for security issues
    ScanAll,
    /// Show security statistics
    Stats,
    /// Update security rules
    UpdateRules,
}
