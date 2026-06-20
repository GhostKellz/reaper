//! Transaction Journal for Rollback Support
//!
//! Records package operations (install/upgrade/remove) with enough detail
//! to support rollback planning and execution.
//!
//! Storage: `~/.local/share/reap/history/transactions/`

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::core::Source;

// =============================================================================
// Transaction Types
// =============================================================================

/// A recorded package operation transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    /// Unique transaction ID (tx_YYYYMMDD_HHMMSS_XXXX)
    pub id: String,
    /// When the transaction started
    pub started_at: DateTime<Utc>,
    /// When the transaction completed (None if in progress or failed early)
    pub completed_at: Option<DateTime<Utc>>,
    /// Type of operation performed
    pub operation: TransactionOperation,
    /// Packages explicitly requested by the user
    pub requested_packages: Vec<String>,
    /// All packages affected (including dependencies)
    pub affected_packages: Vec<PackageChange>,
    /// Final status of the transaction
    pub status: TransactionStatus,
    /// Rollback eligibility status
    pub rollback_status: RollbackStatus,
    /// Active profile at time of transaction
    pub active_profile: Option<String>,
    /// History of rollback attempts for this transaction
    #[serde(default)]
    pub rollback_attempts: Vec<RollbackAttempt>,
}

/// Type of package operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionOperation {
    Install,
    Upgrade,
    Remove,
    Reinstall,
    /// Upgrade all packages (-Syu style)
    UpgradeAll,
}

impl std::fmt::Display for TransactionOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionOperation::Install => write!(f, "Install"),
            TransactionOperation::Upgrade => write!(f, "Upgrade"),
            TransactionOperation::Remove => write!(f, "Remove"),
            TransactionOperation::Reinstall => write!(f, "Reinstall"),
            TransactionOperation::UpgradeAll => write!(f, "UpgradeAll"),
        }
    }
}

/// Changes made to a single package during a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageChange {
    /// Package name
    pub name: String,
    /// Source of the package
    pub source: Source,
    /// Version before this transaction (None if fresh install)
    pub previous_version: Option<String>,
    /// Version after this transaction (None if removal)
    pub new_version: Option<String>,
    /// Path to artifact for previous version (for rollback)
    pub previous_artifact: Option<PathBuf>,
    /// Path to artifact for new version
    pub new_artifact: Option<PathBuf>,
    /// Change type for this specific package
    pub change_type: PackageChangeType,
}

/// Type of change for an individual package
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackageChangeType {
    /// Fresh installation
    Install,
    /// Version upgrade
    Upgrade,
    /// Version downgrade
    Downgrade,
    /// Reinstall same version
    Reinstall,
    /// Package removal
    Remove,
    /// Installed as dependency
    DependencyInstall,
}

impl std::fmt::Display for PackageChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageChangeType::Install => write!(f, "install"),
            PackageChangeType::Upgrade => write!(f, "upgrade"),
            PackageChangeType::Downgrade => write!(f, "downgrade"),
            PackageChangeType::Reinstall => write!(f, "reinstall"),
            PackageChangeType::Remove => write!(f, "remove"),
            PackageChangeType::DependencyInstall => write!(f, "dep-install"),
        }
    }
}

/// Transaction completion status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction is currently in progress
    InProgress,
    /// Transaction completed successfully
    Completed,
    /// Transaction failed
    Failed(String),
    /// Transaction was interrupted
    Interrupted,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::InProgress => write!(f, "in-progress"),
            TransactionStatus::Completed => write!(f, "completed"),
            TransactionStatus::Failed(reason) => write!(f, "failed: {}", reason),
            TransactionStatus::Interrupted => write!(f, "interrupted"),
        }
    }
}

/// Rollback eligibility status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RollbackStatus {
    /// All packages can be rolled back
    Rollbackable,
    /// Some packages can be rolled back
    PartiallyRollbackable {
        rollbackable_count: usize,
        not_rollbackable_count: usize,
        reasons: Vec<String>,
    },
    /// No packages can be rolled back
    NotRollbackable { reasons: Vec<String> },
    /// Already rolled back
    RolledBack { rolled_back_at: DateTime<Utc> },
    /// Eligibility not yet computed
    Unknown,
}

/// A recorded rollback attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackAttempt {
    /// When the rollback was attempted
    pub attempted_at: DateTime<Utc>,
    /// Result of the attempt
    pub result: RollbackAttemptResult,
    /// Packages successfully restored (downgraded or reinstalled)
    pub packages_restored: Vec<String>,
    /// Packages successfully removed
    pub packages_removed: Vec<String>,
    /// Packages that failed with reasons
    pub packages_failed: Vec<(String, String)>,
    /// Whether verification passed
    pub verification_passed: bool,
}

/// Result of a rollback attempt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RollbackAttemptResult {
    /// All packages rolled back successfully and verified
    Success,
    /// Some packages failed but others succeeded
    PartialSuccess,
    /// All operations failed
    Failed { reason: String },
}

// =============================================================================
// Transaction Journal
// =============================================================================

/// Manages transaction record persistence
pub struct TransactionJournal {
    transactions_dir: PathBuf,
}

impl TransactionJournal {
    pub fn new() -> Self {
        Self::at(crate::paths::transactions_dir())
    }

    /// Create a journal rooted at an explicit transactions directory.
    ///
    /// Production code uses [`TransactionJournal::new`]; tests use this to
    /// point the journal at a temporary directory so they never touch the
    /// real `~/.local/share/reap` history.
    pub fn at(transactions_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&transactions_dir);
        Self { transactions_dir }
    }

    /// Start a new transaction and return a builder
    pub fn begin_transaction(
        &self,
        operation: TransactionOperation,
        requested_packages: Vec<String>,
    ) -> TransactionBuilder {
        TransactionBuilder::new(operation, requested_packages)
    }

    /// Save a transaction record
    pub fn save_transaction(&self, record: &TransactionRecord) -> Result<()> {
        let file_path = self.transactions_dir.join(format!("{}.json", record.id));
        let content = serde_json::to_string_pretty(record)?;
        fs::write(file_path, content)?;
        Ok(())
    }

    /// Load a transaction by ID
    pub fn load_transaction(&self, id: &str) -> Result<TransactionRecord> {
        let file_path = self.transactions_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&file_path)
            .map_err(|_| anyhow::anyhow!("Transaction not found: {}", id))?;
        let record: TransactionRecord = serde_json::from_str(&content)?;
        Ok(record)
    }

    /// List all transactions, ordered by date (newest first)
    pub fn list_transactions(&self, limit: Option<usize>) -> Result<Vec<TransactionRecord>> {
        let mut records = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.transactions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json")
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(record) = serde_json::from_str::<TransactionRecord>(&content)
                {
                    records.push(record);
                }
            }
        }

        // Sort by started_at descending (newest first)
        records.sort_by_key(|record| std::cmp::Reverse(record.started_at));

        if let Some(n) = limit {
            records.truncate(n);
        }

        Ok(records)
    }

    /// Get transactions for a specific package
    pub fn transactions_for_package(&self, pkg: &str) -> Result<Vec<TransactionRecord>> {
        let all = self.list_transactions(None)?;
        Ok(all
            .into_iter()
            .filter(|t| t.affected_packages.iter().any(|p| p.name == pkg))
            .collect())
    }
}

impl Default for TransactionJournal {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Transaction Builder
// =============================================================================

/// Builder pattern for constructing transactions
pub struct TransactionBuilder {
    id: String,
    started_at: DateTime<Utc>,
    operation: TransactionOperation,
    requested_packages: Vec<String>,
    affected_packages: Vec<PackageChange>,
    active_profile: Option<String>,
}

impl TransactionBuilder {
    pub fn new(operation: TransactionOperation, requested_packages: Vec<String>) -> Self {
        Self {
            id: generate_transaction_id(),
            started_at: Utc::now(),
            operation,
            requested_packages,
            affected_packages: Vec::new(),
            active_profile: crate::config::Config::load().active_profile,
        }
    }

    /// Add a package change to this transaction
    pub fn add_package_change(&mut self, change: PackageChange) -> &mut Self {
        self.affected_packages.push(change);
        self
    }

    /// Finalize as completed transaction
    pub fn complete(self) -> TransactionRecord {
        let rollback_status = compute_rollback_eligibility(&self.affected_packages);

        TransactionRecord {
            id: self.id,
            started_at: self.started_at,
            completed_at: Some(Utc::now()),
            operation: self.operation,
            requested_packages: self.requested_packages,
            affected_packages: self.affected_packages,
            status: TransactionStatus::Completed,
            rollback_status,
            active_profile: self.active_profile,
            rollback_attempts: Vec::new(),
        }
    }

    /// Finalize as a failed transaction.
    ///
    /// Constructs the `Failed` status that the history UI already renders, used
    /// when the underlying install/upgrade result is an error so the journal
    /// never records a failed operation as `Completed`.
    pub fn fail(self, reason: String) -> TransactionRecord {
        TransactionRecord {
            id: self.id,
            started_at: self.started_at,
            completed_at: Some(Utc::now()),
            operation: self.operation,
            requested_packages: self.requested_packages,
            affected_packages: self.affected_packages,
            status: TransactionStatus::Failed(reason.clone()),
            rollback_status: RollbackStatus::NotRollbackable {
                reasons: vec![format!("Transaction failed: {}", reason)],
            },
            active_profile: self.active_profile,
            rollback_attempts: Vec::new(),
        }
    }
}

/// Generate a unique transaction ID
fn generate_transaction_id() -> String {
    let now = Utc::now();
    // Use nanoseconds for uniqueness instead of rand crate
    let nanos = now.timestamp_subsec_nanos() % 10000;
    format!("tx_{}_{:04}", now.format("%Y%m%d_%H%M%S"), nanos)
}

// =============================================================================
// Eligibility Logic
// =============================================================================

/// Compute rollback eligibility for a set of package changes
pub fn compute_rollback_eligibility(changes: &[PackageChange]) -> RollbackStatus {
    if changes.is_empty() {
        return RollbackStatus::NotRollbackable {
            reasons: vec!["No packages were affected".to_string()],
        };
    }

    let mut rollbackable = Vec::new();
    let mut not_rollbackable = Vec::new();
    let mut reasons = Vec::new();

    for change in changes {
        match is_package_rollbackable(change) {
            Ok(()) => rollbackable.push(&change.name),
            Err(reason) => {
                not_rollbackable.push(&change.name);
                reasons.push(format!("{}: {}", change.name, reason));
            }
        }
    }

    if not_rollbackable.is_empty() {
        RollbackStatus::Rollbackable
    } else if rollbackable.is_empty() {
        RollbackStatus::NotRollbackable { reasons }
    } else {
        RollbackStatus::PartiallyRollbackable {
            rollbackable_count: rollbackable.len(),
            not_rollbackable_count: not_rollbackable.len(),
            reasons,
        }
    }
}

/// Check if a single package change can be rolled back
fn is_package_rollbackable(change: &PackageChange) -> Result<(), String> {
    match change.change_type {
        PackageChangeType::Remove => {
            // For removals, we need the artifact to reinstall
            if change.previous_artifact.is_none() {
                return Err("Previous artifact path not recorded".to_string());
            }
            let artifact_path = change.previous_artifact.as_ref().unwrap();
            if !artifact_path.exists() {
                return Err(format!("Artifact missing: {}", artifact_path.display()));
            }
        }
        PackageChangeType::Install | PackageChangeType::DependencyInstall => {
            // Fresh installs roll back by removing - always possible
        }
        PackageChangeType::Upgrade
        | PackageChangeType::Downgrade
        | PackageChangeType::Reinstall => {
            // Need previous artifact to restore
            if change.previous_artifact.is_none() {
                return Err("Previous artifact path not recorded".to_string());
            }
            let artifact_path = change.previous_artifact.as_ref().unwrap();
            if !artifact_path.exists() {
                return Err(format!("Artifact missing: {}", artifact_path.display()));
            }
        }
    }
    Ok(())
}

// =============================================================================
// Artifact Discovery
// =============================================================================

/// The system directories searched for rollback artifacts, in priority order:
/// the pacman cache, Reaper's AUR cache, and retained AUR artifacts.
pub fn artifact_search_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/var/cache/pacman/pkg"),
        crate::paths::aur_cache_dir(),
        aur_artifacts_dir(),
    ]
}

/// Find an artifact for a package version within the given directories.
///
/// Directory injection keeps this testable: production passes
/// [`artifact_search_dirs`], tests pass a temporary directory.
pub fn find_artifact_in_dirs(pkg: &str, version: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    // Try common patterns: pkg-version-release-arch.pkg.tar.zst
    // Note: version might include release, or release might be separate
    let patterns = [
        format!("{}-{}-*.pkg.tar.zst", pkg, version),
        format!("{}-{}-*.pkg.tar.xz", pkg, version),
        format!("{}-{}-*.pkg.tar.gz", pkg, version),
        format!("{}-{}*.pkg.tar.zst", pkg, version),
        format!("{}-{}*.pkg.tar.xz", pkg, version),
    ];

    for dir in dirs {
        for pattern in &patterns {
            let full_pattern = dir.join(pattern);
            if let Ok(entries) = glob::glob(&full_pattern.to_string_lossy()) {
                for entry in entries.flatten() {
                    if entry.exists() {
                        return Some(entry);
                    }
                }
            }
        }
    }

    None
}

/// Find artifact path for a package version in the system caches.
pub fn find_artifact_in_cache(pkg: &str, version: &str) -> Option<PathBuf> {
    find_artifact_in_dirs(pkg, version, &artifact_search_dirs())
}

// =============================================================================
// Recording Helpers
// =============================================================================

/// Capture the current state of a package before modification
pub fn capture_pre_state(pkg: &str, source: &Source) -> PackageChange {
    let current_version = crate::pacman::get_version(pkg);
    let previous_artifact = current_version
        .as_ref()
        .and_then(|v| find_artifact_in_cache(pkg, v));

    PackageChange {
        name: pkg.to_string(),
        source: source.clone(),
        previous_version: current_version,
        new_version: None,
        previous_artifact,
        new_artifact: None,
        change_type: PackageChangeType::Install, // Will be updated in capture_post_state
    }
}

/// Update a package change record after operation completes
pub fn capture_post_state(change: &mut PackageChange) {
    // Get new version from pacman
    change.new_version = crate::pacman::get_version(&change.name);

    // Find artifact for new version
    if let Some(ref new_ver) = change.new_version {
        change.new_artifact = find_artifact_in_cache(&change.name, new_ver);
    }

    // Determine change type based on versions
    change.change_type = match (&change.previous_version, &change.new_version) {
        (None, Some(_)) => PackageChangeType::Install,
        (Some(_), None) => PackageChangeType::Remove,
        (Some(old), Some(new)) if old == new => PackageChangeType::Reinstall,
        (Some(old), Some(new)) => {
            if crate::version::is_older(old, new) {
                PackageChangeType::Upgrade
            } else {
                PackageChangeType::Downgrade
            }
        }
        (None, None) => PackageChangeType::Install, // Edge case
    };
}

// =============================================================================
// Rollback Execution
// =============================================================================

/// Result of a rollback operation
#[derive(Debug)]
pub struct RollbackResult {
    pub success: bool,
    pub packages_restored: Vec<String>,
    pub packages_removed: Vec<String>,
    pub packages_failed: Vec<(String, String)>, // (name, reason)
    pub verification_passed: bool,
}

/// Plan for rolling back a transaction
#[derive(Debug)]
pub struct RollbackPlan {
    pub downgrades: Vec<(String, String, PathBuf)>, // (pkg_name, expected_version, artifact_path)
    pub reinstalls: Vec<(String, String, PathBuf)>, // (pkg_name, expected_version, artifact_path)
    pub removals: Vec<String>,                      // pkg_name
    pub unavailable: Vec<(String, String)>,         // (pkg_name, reason)
    pub analysis: RollbackAnalysis,                 // Dependency/conflict analysis
}

impl RollbackPlan {
    /// Check if rollback has warnings that should be shown to user
    pub fn has_warnings(&self) -> bool {
        !self.analysis.warnings.is_empty()
    }

    /// Check if rollback has critical warnings (dependency breaks, conflicts)
    /// Note: Critical warnings don't prevent rollback, but indicate high risk
    pub fn has_critical_warnings(&self) -> bool {
        self.analysis.warnings.iter().any(|w| w.is_critical())
    }
}

/// Analysis of rollback impact on the system
#[derive(Debug, Default)]
pub struct RollbackAnalysis {
    pub warnings: Vec<RollbackWarning>,
    pub dependency_closure: Vec<String>, // Additional packages affected
    pub provider_changes: Vec<ProviderChange>,
}

/// Warning types for rollback operations
#[derive(Debug, Clone)]
pub enum RollbackWarning {
    /// Downgrading this package would break a dependency
    DependencyBreak {
        package: String,
        dependent: String,
        required_version: String,
        rollback_version: String,
    },
    /// A package that depends on the target will also need attention
    AffectedDependent { package: String, dependent: String },
    /// Removing this package may leave orphaned dependencies
    OrphanedDependencies {
        package: String,
        orphans: Vec<String>,
    },
    /// Version conflict with another installed package
    VersionConflict {
        package: String,
        conflicting_package: String,
        reason: String,
    },
    /// Mixed source rollback (repo + AUR in same transaction)
    MixedSourceTransaction {
        repo_packages: Vec<String>,
        aur_packages: Vec<String>,
    },
}

impl RollbackWarning {
    /// Check if this is a critical warning (high risk of system breakage)
    /// Critical warnings are shown prominently but don't prevent rollback
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            RollbackWarning::DependencyBreak { .. } | RollbackWarning::VersionConflict { .. }
        )
    }

    /// Get a human-readable description of the warning
    pub fn description(&self) -> String {
        match self {
            RollbackWarning::DependencyBreak {
                package,
                dependent,
                required_version,
                rollback_version,
            } => format!(
                "{} requires {} {} but rollback would install {}",
                dependent, package, required_version, rollback_version
            ),
            RollbackWarning::AffectedDependent { package, dependent } => {
                format!("{} depends on {} and may be affected", dependent, package)
            }
            RollbackWarning::OrphanedDependencies { package, orphans } => {
                format!("Removing {} may orphan: {}", package, orphans.join(", "))
            }
            RollbackWarning::VersionConflict {
                package,
                conflicting_package,
                reason,
            } => format!(
                "{} conflicts with {}: {}",
                package, conflicting_package, reason
            ),
            RollbackWarning::MixedSourceTransaction {
                repo_packages,
                aur_packages,
            } => format!(
                "Mixed sources: {} repo packages, {} AUR packages",
                repo_packages.len(),
                aur_packages.len()
            ),
        }
    }
}

/// Represents a change in package providers
#[derive(Debug, Clone)]
pub struct ProviderChange {
    pub capability: String,
    pub old_provider: String,
}

/// Create a rollback plan from a transaction record.
///
/// `search_dirs` are the directories scanned to recover artifacts whose paths
/// were not recorded (production passes [`artifact_search_dirs`]).
pub fn create_rollback_plan(record: &TransactionRecord, search_dirs: &[PathBuf]) -> RollbackPlan {
    let mut plan = RollbackPlan {
        downgrades: Vec::new(),
        reinstalls: Vec::new(),
        removals: Vec::new(),
        unavailable: Vec::new(),
        analysis: RollbackAnalysis::default(),
    };

    for change in &record.affected_packages {
        match change.change_type {
            PackageChangeType::Install | PackageChangeType::DependencyInstall => {
                // Fresh installs are rolled back by removing
                plan.removals.push(change.name.clone());
            }
            PackageChangeType::Remove => {
                // Removals are rolled back by reinstalling from artifact
                if let Some(ref artifact) = change.previous_artifact {
                    if artifact.exists() {
                        let version = change
                            .previous_version
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string());
                        plan.reinstalls
                            .push((change.name.clone(), version, artifact.clone()));
                    } else {
                        plan.unavailable.push((
                            change.name.clone(),
                            format!("Artifact missing: {}", artifact.display()),
                        ));
                    }
                } else {
                    plan.unavailable
                        .push((change.name.clone(), "No artifact recorded".to_string()));
                }
            }
            PackageChangeType::Upgrade
            | PackageChangeType::Downgrade
            | PackageChangeType::Reinstall => {
                // Version changes are rolled back by installing previous version
                if let Some(ref artifact) = change.previous_artifact {
                    if artifact.exists() {
                        let version = change
                            .previous_version
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string());
                        plan.downgrades
                            .push((change.name.clone(), version, artifact.clone()));
                    } else {
                        // Try to find artifact in cache again (might have been restored)
                        if let Some(ref prev_ver) = change.previous_version {
                            if let Some(found) =
                                find_artifact_in_dirs(&change.name, prev_ver, search_dirs)
                            {
                                plan.downgrades.push((
                                    change.name.clone(),
                                    prev_ver.clone(),
                                    found,
                                ));
                            } else {
                                plan.unavailable.push((
                                    change.name.clone(),
                                    format!("Artifact missing: {}", artifact.display()),
                                ));
                            }
                        } else {
                            plan.unavailable.push((
                                change.name.clone(),
                                "Previous version unknown".to_string(),
                            ));
                        }
                    }
                } else if let Some(ref prev_ver) = change.previous_version {
                    // No artifact recorded but we know the version - try to find it
                    if let Some(found) = find_artifact_in_dirs(&change.name, prev_ver, search_dirs)
                    {
                        plan.downgrades
                            .push((change.name.clone(), prev_ver.clone(), found));
                    } else {
                        plan.unavailable.push((
                            change.name.clone(),
                            format!("Cannot find artifact for version {}", prev_ver),
                        ));
                    }
                } else {
                    plan.unavailable.push((
                        change.name.clone(),
                        "No artifact or version recorded".to_string(),
                    ));
                }
            }
        }
    }

    // Analyze the plan for dependency issues and conflicts
    plan.analysis = analyze_rollback_plan(&plan, record);

    plan
}

/// Analyze a rollback plan for potential issues
fn analyze_rollback_plan(plan: &RollbackPlan, record: &TransactionRecord) -> RollbackAnalysis {
    let mut analysis = RollbackAnalysis::default();

    // Track source types for mixed transaction warning
    let mut repo_packages = Vec::new();
    let mut aur_packages = Vec::new();

    for change in &record.affected_packages {
        match &change.source {
            Source::Aur => aur_packages.push(change.name.clone()),
            Source::Pacman | Source::ChaoticAUR | Source::GhostctlAUR => {
                repo_packages.push(change.name.clone())
            }
            _ => {}
        }
    }

    // Warn about mixed source transactions
    if !repo_packages.is_empty() && !aur_packages.is_empty() {
        analysis
            .warnings
            .push(RollbackWarning::MixedSourceTransaction {
                repo_packages: repo_packages.clone(),
                aur_packages: aur_packages.clone(),
            });
    }

    // Analyze downgrades for dependency breaks
    for (pkg, _expected_ver, _artifact) in &plan.downgrades {
        // Find the target version we're downgrading to
        let target_version = record
            .affected_packages
            .iter()
            .find(|c| &c.name == pkg)
            .and_then(|c| c.previous_version.clone());

        if let Some(target_ver) = target_version {
            // Check what depends on this package
            let dependents = crate::pacman::get_reverse_depends(pkg);
            for dependent in dependents {
                // Get the dependent's requirements
                let deps = crate::pacman::get_depends(&dependent);
                for dep in deps {
                    let (dep_name, constraint) = crate::pacman::parse_dependency(&dep);
                    if dep_name == *pkg {
                        if let Some(ref req) = constraint {
                            // Check if downgrade version satisfies the constraint
                            if !crate::pacman::version_satisfies(&target_ver, req) {
                                analysis.warnings.push(RollbackWarning::DependencyBreak {
                                    package: pkg.clone(),
                                    dependent: dependent.clone(),
                                    required_version: req.clone(),
                                    rollback_version: target_ver.clone(),
                                });
                            }
                        }
                        // Also add as affected dependent
                        analysis.warnings.push(RollbackWarning::AffectedDependent {
                            package: pkg.clone(),
                            dependent: dependent.clone(),
                        });
                    }
                }
            }

            // Check for provider changes - only report if something depends on the capability
            let current_provides = crate::pacman::get_provides(pkg);
            for provided in current_provides {
                // Check if anything actually depends on this capability
                let dependents = crate::pacman::get_reverse_depends(&provided);
                if !dependents.is_empty() {
                    analysis.provider_changes.push(ProviderChange {
                        capability: provided,
                        old_provider: pkg.clone(),
                    });
                }
            }
        }
    }

    // Analyze removals for orphaned dependencies
    for pkg in &plan.removals {
        // Get dependencies of package being removed
        let deps = crate::pacman::get_depends(pkg);
        let mut potential_orphans = Vec::new();

        for dep in deps {
            let (dep_name, _) = crate::pacman::parse_dependency(&dep);
            // Check if this dependency is used by other packages
            let other_dependents = crate::pacman::get_reverse_depends(&dep_name);
            // If only the package being removed depends on it, it may become orphaned
            if other_dependents.len() == 1 && other_dependents.first() == Some(pkg) {
                potential_orphans.push(dep_name);
            }
        }

        if !potential_orphans.is_empty() {
            analysis
                .warnings
                .push(RollbackWarning::OrphanedDependencies {
                    package: pkg.clone(),
                    orphans: potential_orphans,
                });
        }

        // Add to dependency closure
        analysis.dependency_closure.push(pkg.clone());
    }

    // Analyze reinstalls for conflicts
    for (pkg, _expected_ver, _artifact) in &plan.reinstalls {
        let conflicts = crate::pacman::get_conflicts(pkg);
        for conflict in conflicts {
            // Check if conflicting package is installed
            if crate::pacman::is_installed(&conflict) {
                analysis.warnings.push(RollbackWarning::VersionConflict {
                    package: pkg.clone(),
                    conflicting_package: conflict.clone(),
                    reason: format!("{} conflicts with {}", pkg, conflict),
                });
            }
        }
    }

    analysis
}

/// Execute a rollback plan
pub fn execute_rollback(plan: &RollbackPlan) -> RollbackResult {
    use std::process::Command;

    let mut result = RollbackResult {
        success: true,
        packages_restored: Vec::new(),
        packages_removed: Vec::new(),
        packages_failed: Vec::new(),
        verification_passed: false,
    };

    // Phase 1: Downgrades (install previous versions)
    for (pkg, expected_ver, artifact) in &plan.downgrades {
        println!(
            "[rollback] Downgrading {} to {} from {}...",
            pkg,
            expected_ver,
            artifact.display()
        );

        let status = Command::new("sudo")
            .args(["pacman", "-U", "--noconfirm"])
            .arg(artifact)
            .status();

        match status {
            Ok(s) if s.success() => {
                println!(
                    "[rollback] Successfully downgraded {} to {}",
                    pkg, expected_ver
                );
                result.packages_restored.push(pkg.clone());
            }
            Ok(_) => {
                let msg = format!("pacman -U failed for {}", artifact.display());
                eprintln!("[rollback] Failed to downgrade {}: {}", pkg, msg);
                result.packages_failed.push((pkg.clone(), msg));
                result.success = false;
            }
            Err(e) => {
                let msg = format!("Failed to execute pacman: {}", e);
                eprintln!("[rollback] {}", msg);
                result.packages_failed.push((pkg.clone(), msg));
                result.success = false;
            }
        }
    }

    // Phase 2: Reinstalls (restore removed packages)
    for (pkg, expected_ver, artifact) in &plan.reinstalls {
        println!(
            "[rollback] Reinstalling {} {} from {}...",
            pkg,
            expected_ver,
            artifact.display()
        );

        let status = Command::new("sudo")
            .args(["pacman", "-U", "--noconfirm"])
            .arg(artifact)
            .status();

        match status {
            Ok(s) if s.success() => {
                println!(
                    "[rollback] Successfully reinstalled {} {}",
                    pkg, expected_ver
                );
                result.packages_restored.push(pkg.clone());
            }
            Ok(_) => {
                let msg = format!("pacman -U failed for {}", artifact.display());
                eprintln!("[rollback] Failed to reinstall {}: {}", pkg, msg);
                result.packages_failed.push((pkg.clone(), msg));
                result.success = false;
            }
            Err(e) => {
                let msg = format!("Failed to execute pacman: {}", e);
                eprintln!("[rollback] {}", msg);
                result.packages_failed.push((pkg.clone(), msg));
                result.success = false;
            }
        }
    }

    // Phase 3: Removals (remove freshly installed packages)
    if !plan.removals.is_empty() {
        println!("[rollback] Removing {} package(s)...", plan.removals.len());

        let status = Command::new("sudo")
            .args(["pacman", "-R", "--noconfirm"])
            .args(&plan.removals)
            .status();

        match status {
            Ok(s) if s.success() => {
                for pkg in &plan.removals {
                    println!("[rollback] Successfully removed {}", pkg);
                    result.packages_removed.push(pkg.clone());
                }
            }
            Ok(_) => {
                // Try removing one by one to identify failures
                for pkg in &plan.removals {
                    let single_status = Command::new("sudo")
                        .args(["pacman", "-R", "--noconfirm", pkg])
                        .status();

                    match single_status {
                        Ok(s) if s.success() => {
                            result.packages_removed.push(pkg.clone());
                        }
                        _ => {
                            let msg = "pacman -R failed".to_string();
                            result.packages_failed.push((pkg.clone(), msg));
                            result.success = false;
                        }
                    }
                }
            }
            Err(e) => {
                let msg = format!("Failed to execute pacman: {}", e);
                eprintln!("[rollback] {}", msg);
                for pkg in &plan.removals {
                    result.packages_failed.push((pkg.clone(), msg.clone()));
                }
                result.success = false;
            }
        }
    }

    // Phase 4: Verify installed versions
    result.verification_passed = verify_rollback(plan, &result);

    result
}

/// Verify that rollback achieved the expected state
fn verify_rollback(plan: &RollbackPlan, result: &RollbackResult) -> bool {
    let mut all_verified = true;

    // Verify downgrades - packages should be at the expected previous version
    for (pkg, expected_ver, _) in &plan.downgrades {
        if result.packages_restored.contains(pkg) {
            if let Some(actual_ver) = crate::pacman::get_version(pkg) {
                if &actual_ver == expected_ver || expected_ver == "unknown" {
                    println!("[rollback] Verified: {} at version {}", pkg, actual_ver);
                } else {
                    eprintln!(
                        "[rollback] Warning: {} at version {} (expected {})",
                        pkg, actual_ver, expected_ver
                    );
                    all_verified = false;
                }
            } else {
                eprintln!("[rollback] Warning: Cannot verify version for {}", pkg);
                all_verified = false;
            }
        }
    }

    // Verify reinstalls - packages should be at the expected version
    for (pkg, expected_ver, _) in &plan.reinstalls {
        if result.packages_restored.contains(pkg) {
            if let Some(actual_ver) = crate::pacman::get_version(pkg) {
                if &actual_ver == expected_ver || expected_ver == "unknown" {
                    println!(
                        "[rollback] Verified: {} reinstalled at version {}",
                        pkg, actual_ver
                    );
                } else {
                    eprintln!(
                        "[rollback] Warning: {} at version {} (expected {})",
                        pkg, actual_ver, expected_ver
                    );
                    all_verified = false;
                }
            } else {
                eprintln!("[rollback] Warning: {} not found after reinstall", pkg);
                all_verified = false;
            }
        }
    }

    // Verify removals - packages should not be installed
    for pkg in &plan.removals {
        if result.packages_removed.contains(pkg) {
            if crate::pacman::get_version(pkg).is_none() {
                println!("[rollback] Verified: {} is removed", pkg);
            } else {
                eprintln!("[rollback] Warning: {} still installed after removal", pkg);
                all_verified = false;
            }
        }
    }

    all_verified
}

/// Classify a rollback attempt from its result.
///
/// A clean run requires *both* that no package failed and that post-rollback
/// verification passed. A verification failure on an otherwise-applied rollback
/// is therefore reported as `PartialSuccess`, never `Success`, so the journal
/// reflects that the on-disk state was not confirmed.
fn classify_rollback_attempt(result: &RollbackResult) -> RollbackAttemptResult {
    if result.packages_failed.is_empty() && result.verification_passed {
        RollbackAttemptResult::Success
    } else if result.packages_restored.is_empty() && result.packages_removed.is_empty() {
        RollbackAttemptResult::Failed {
            reason: "All operations failed".to_string(),
        }
    } else {
        RollbackAttemptResult::PartialSuccess
    }
}

/// Record a rollback attempt in the transaction history
pub fn record_rollback_attempt(
    journal: &TransactionJournal,
    record: &mut TransactionRecord,
    result: &RollbackResult,
) -> Result<()> {
    let now = Utc::now();

    let attempt_result = classify_rollback_attempt(result);

    // Create the attempt record
    let attempt = RollbackAttempt {
        attempted_at: now,
        result: attempt_result.clone(),
        packages_restored: result.packages_restored.clone(),
        packages_removed: result.packages_removed.clone(),
        packages_failed: result.packages_failed.clone(),
        verification_passed: result.verification_passed,
    };

    // Add to attempt history
    record.rollback_attempts.push(attempt);

    // Update rollback status based on the result
    record.rollback_status = match attempt_result {
        RollbackAttemptResult::Success => RollbackStatus::RolledBack {
            rolled_back_at: now,
        },
        RollbackAttemptResult::PartialSuccess => RollbackStatus::PartiallyRollbackable {
            rollbackable_count: result.packages_restored.len() + result.packages_removed.len(),
            not_rollbackable_count: result.packages_failed.len(),
            reasons: result
                .packages_failed
                .iter()
                .map(|(pkg, reason)| format!("{}: {}", pkg, reason))
                .collect(),
        },
        RollbackAttemptResult::Failed { ref reason } => RollbackStatus::NotRollbackable {
            reasons: vec![reason.clone()],
        },
    };

    journal.save_transaction(record)
}

// =============================================================================
// AUR Artifact Retention
// =============================================================================

/// Directory for retained AUR artifacts
pub fn aur_artifacts_dir() -> PathBuf {
    crate::paths::DATA_DIR.join("artifacts").join("aur")
}

/// Retain an AUR artifact for future rollback
pub fn retain_aur_artifact(
    pkg: &str,
    version: &str,
    build_dir: &std::path::Path,
) -> Option<PathBuf> {
    use std::fs;

    let artifacts_dir = aur_artifacts_dir();
    if fs::create_dir_all(&artifacts_dir).is_err() {
        return None;
    }

    // Find the built package in the build directory
    let patterns = [
        format!("{}-{}-*.pkg.tar.zst", pkg, version),
        format!("{}-{}-*.pkg.tar.xz", pkg, version),
    ];

    for pattern in &patterns {
        let full_pattern = build_dir.join(pattern);
        if let Ok(entries) = glob::glob(&full_pattern.to_string_lossy()) {
            for entry in entries.flatten() {
                if entry.exists() {
                    // Copy to artifacts directory
                    let dest = artifacts_dir.join(entry.file_name().unwrap_or_default());
                    if fs::copy(&entry, &dest).is_ok() {
                        println!(
                            "[artifact] Retained {} for rollback",
                            dest.file_name().unwrap_or_default().to_string_lossy()
                        );
                        return Some(dest);
                    }
                }
            }
        }
    }

    None
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_id_generation() {
        let id1 = generate_transaction_id();
        let id2 = generate_transaction_id();
        assert!(id1.starts_with("tx_"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_rollback_eligibility_fresh_install() {
        let changes = vec![PackageChange {
            name: "test-pkg".to_string(),
            source: Source::Aur,
            previous_version: None,
            new_version: Some("1.0".to_string()),
            previous_artifact: None,
            new_artifact: Some(PathBuf::from("/tmp/test.pkg.tar.zst")),
            change_type: PackageChangeType::Install,
        }];

        let status = compute_rollback_eligibility(&changes);
        assert!(matches!(status, RollbackStatus::Rollbackable));
    }

    #[test]
    fn test_rollback_eligibility_upgrade_without_artifact() {
        let changes = vec![PackageChange {
            name: "test-pkg".to_string(),
            source: Source::Aur,
            previous_version: Some("1.0".to_string()),
            new_version: Some("2.0".to_string()),
            previous_artifact: None, // Missing artifact
            new_artifact: Some(PathBuf::from("/tmp/test.pkg.tar.zst")),
            change_type: PackageChangeType::Upgrade,
        }];

        let status = compute_rollback_eligibility(&changes);
        assert!(matches!(status, RollbackStatus::NotRollbackable { .. }));
    }

    #[test]
    fn test_rollback_eligibility_empty() {
        let changes: Vec<PackageChange> = vec![];
        let status = compute_rollback_eligibility(&changes);
        assert!(matches!(status, RollbackStatus::NotRollbackable { .. }));
    }

    #[test]
    fn test_transaction_operation_display() {
        assert_eq!(format!("{}", TransactionOperation::Install), "Install");
        assert_eq!(format!("{}", TransactionOperation::Upgrade), "Upgrade");
        assert_eq!(format!("{}", TransactionOperation::Remove), "Remove");
        assert_eq!(
            format!("{}", TransactionOperation::UpgradeAll),
            "UpgradeAll"
        );
    }

    #[test]
    fn test_package_change_type_display() {
        assert_eq!(format!("{}", PackageChangeType::Install), "install");
        assert_eq!(format!("{}", PackageChangeType::Upgrade), "upgrade");
        assert_eq!(format!("{}", PackageChangeType::Remove), "remove");
    }

    // =========================================================================
    // Rollback Plan Scenario Tests
    // =========================================================================

    fn make_test_record(
        operation: TransactionOperation,
        changes: Vec<PackageChange>,
    ) -> TransactionRecord {
        TransactionRecord {
            id: "tx_test_001".to_string(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            operation,
            requested_packages: changes.iter().map(|c| c.name.clone()).collect(),
            affected_packages: changes,
            status: TransactionStatus::Completed,
            rollback_status: RollbackStatus::Unknown,
            active_profile: None,
            rollback_attempts: Vec::new(),
        }
    }

    #[test]
    fn test_rollback_plan_fresh_install_becomes_removal() {
        // Fresh install should be rolled back by removing the package
        let changes = vec![PackageChange {
            name: "new-pkg".to_string(),
            source: Source::Aur,
            previous_version: None,
            new_version: Some("1.0.0".to_string()),
            previous_artifact: None,
            new_artifact: Some(PathBuf::from("/tmp/new-pkg-1.0.0.pkg.tar.zst")),
            change_type: PackageChangeType::Install,
        }];

        let record = make_test_record(TransactionOperation::Install, changes);
        let plan = create_rollback_plan(&record, &[]);

        assert!(plan.downgrades.is_empty());
        assert!(plan.reinstalls.is_empty());
        assert_eq!(plan.removals.len(), 1);
        assert_eq!(plan.removals[0], "new-pkg");
        assert!(plan.unavailable.is_empty());
    }

    #[test]
    fn test_rollback_plan_removal_becomes_reinstall() {
        // Package removal should be rolled back by reinstalling from artifact
        // Create a temp file to simulate existing artifact
        let temp_dir = std::env::temp_dir();
        let artifact_path = temp_dir.join("removed-pkg-1.0.0-x86_64.pkg.tar.zst");
        std::fs::write(&artifact_path, "fake artifact").unwrap();

        let changes = vec![PackageChange {
            name: "removed-pkg".to_string(),
            source: Source::Pacman,
            previous_version: Some("1.0.0".to_string()),
            new_version: None,
            previous_artifact: Some(artifact_path.clone()),
            new_artifact: None,
            change_type: PackageChangeType::Remove,
        }];

        let record = make_test_record(TransactionOperation::Remove, changes);
        let plan = create_rollback_plan(&record, &[]);

        assert!(plan.downgrades.is_empty());
        assert_eq!(plan.reinstalls.len(), 1);
        assert_eq!(plan.reinstalls[0].0, "removed-pkg");
        assert!(plan.removals.is_empty());
        assert!(plan.unavailable.is_empty());

        // Cleanup
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[test]
    fn test_rollback_plan_upgrade_missing_artifact() {
        // Upgrade with missing artifact should be marked unavailable
        let changes = vec![PackageChange {
            name: "upgraded-pkg".to_string(),
            source: Source::Pacman,
            previous_version: Some("1.0.0".to_string()),
            new_version: Some("2.0.0".to_string()),
            previous_artifact: Some(PathBuf::from("/nonexistent/artifact.pkg.tar.zst")),
            new_artifact: Some(PathBuf::from("/tmp/upgraded-pkg-2.0.0.pkg.tar.zst")),
            change_type: PackageChangeType::Upgrade,
        }];

        let record = make_test_record(TransactionOperation::Upgrade, changes);
        let plan = create_rollback_plan(&record, &[]);

        // Since artifact doesn't exist and can't be found in cache
        assert_eq!(plan.unavailable.len(), 1);
        assert_eq!(plan.unavailable[0].0, "upgraded-pkg");
    }

    #[test]
    fn test_rollback_plan_upgrade_resolves_artifact_via_cache_fallback() {
        // An upgrade with no recorded previous_artifact must still be
        // rollbackable when the previous version is discoverable on disk.
        // This guards the dry-run/apply parity: the preview must rely on the
        // same find_artifact_in_cache fallback that execution uses, not just
        // on previous_artifact.exists().
        let artifacts_dir = tempfile::tempdir().unwrap();
        let artifact_path = artifacts_dir
            .path()
            .join("zz-fallback-pkg-1.0.0-x86_64.pkg.tar.zst");
        std::fs::write(&artifact_path, "fake artifact").unwrap();

        let changes = vec![PackageChange {
            name: "zz-fallback-pkg".to_string(),
            source: Source::Aur,
            previous_version: Some("1.0.0".to_string()),
            new_version: Some("2.0.0".to_string()),
            previous_artifact: None, // not recorded; must be found via fallback
            new_artifact: None,
            change_type: PackageChangeType::Upgrade,
        }];

        let record = make_test_record(TransactionOperation::Upgrade, changes);
        let plan = create_rollback_plan(&record, &[artifacts_dir.path().to_path_buf()]);

        assert!(
            plan.unavailable.is_empty(),
            "cache-discoverable artifact should not be unavailable: {:?}",
            plan.unavailable
        );
        assert_eq!(plan.downgrades.len(), 1);
        assert_eq!(plan.downgrades[0].0, "zz-fallback-pkg");
    }

    #[test]
    fn test_rollback_plan_mixed_transaction() {
        // Transaction with multiple change types
        let temp_dir = std::env::temp_dir();
        let artifact_path = temp_dir.join("mix-upgrade-0.9.0-x86_64.pkg.tar.zst");
        std::fs::write(&artifact_path, "fake artifact").unwrap();

        let changes = vec![
            PackageChange {
                name: "mix-install".to_string(),
                source: Source::Aur,
                previous_version: None,
                new_version: Some("1.0.0".to_string()),
                previous_artifact: None,
                new_artifact: None,
                change_type: PackageChangeType::Install,
            },
            PackageChange {
                name: "mix-upgrade".to_string(),
                source: Source::Pacman,
                previous_version: Some("0.9.0".to_string()),
                new_version: Some("1.0.0".to_string()),
                previous_artifact: Some(artifact_path.clone()),
                new_artifact: None,
                change_type: PackageChangeType::Upgrade,
            },
        ];

        let record = make_test_record(TransactionOperation::Install, changes);
        let plan = create_rollback_plan(&record, &[]);

        // mix-install should become removal
        assert_eq!(plan.removals.len(), 1);
        assert_eq!(plan.removals[0], "mix-install");

        // mix-upgrade should become downgrade
        assert_eq!(plan.downgrades.len(), 1);
        assert_eq!(plan.downgrades[0].0, "mix-upgrade");

        // Cleanup
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[test]
    fn test_rollback_plan_dependency_install() {
        // Dependencies installed with a package should also be removable
        let changes = vec![
            PackageChange {
                name: "main-pkg".to_string(),
                source: Source::Aur,
                previous_version: None,
                new_version: Some("1.0.0".to_string()),
                previous_artifact: None,
                new_artifact: None,
                change_type: PackageChangeType::Install,
            },
            PackageChange {
                name: "dep-pkg".to_string(),
                source: Source::Pacman,
                previous_version: None,
                new_version: Some("2.0.0".to_string()),
                previous_artifact: None,
                new_artifact: None,
                change_type: PackageChangeType::DependencyInstall,
            },
        ];

        let record = make_test_record(TransactionOperation::Install, changes);
        let plan = create_rollback_plan(&record, &[]);

        // Both should be in removals
        assert_eq!(plan.removals.len(), 2);
        assert!(plan.removals.contains(&"main-pkg".to_string()));
        assert!(plan.removals.contains(&"dep-pkg".to_string()));
    }

    #[test]
    fn test_rollback_plan_partial() {
        let temp_dir = std::env::temp_dir();
        let artifact_path = temp_dir.join("partial-ok-0.9.0-x86_64.pkg.tar.zst");
        std::fs::write(&artifact_path, "fake artifact").unwrap();

        let changes = vec![
            PackageChange {
                name: "partial-ok".to_string(),
                source: Source::Pacman,
                previous_version: Some("0.9.0".to_string()),
                new_version: Some("1.0.0".to_string()),
                previous_artifact: Some(artifact_path.clone()),
                new_artifact: None,
                change_type: PackageChangeType::Upgrade,
            },
            PackageChange {
                name: "partial-missing".to_string(),
                source: Source::Pacman,
                previous_version: Some("0.5.0".to_string()),
                new_version: Some("1.0.0".to_string()),
                previous_artifact: Some(PathBuf::from("/nonexistent/missing.pkg.tar.zst")),
                new_artifact: None,
                change_type: PackageChangeType::Upgrade,
            },
        ];

        let record = make_test_record(TransactionOperation::Upgrade, changes);
        let plan = create_rollback_plan(&record, &[]);

        // Should be partial: one artifact present (executable), one missing.
        assert!(!plan.unavailable.is_empty());
        let executable = plan.downgrades.len() + plan.reinstalls.len() + plan.removals.len();
        assert_eq!(executable, 1);

        // Cleanup
        let _ = std::fs::remove_file(&artifact_path);
    }

    // =========================================================================
    // Rollback Warning Tests
    // =========================================================================

    #[test]
    fn test_rollback_warning_is_critical() {
        let dep_break = RollbackWarning::DependencyBreak {
            package: "pkg".to_string(),
            dependent: "dep".to_string(),
            required_version: ">=2.0".to_string(),
            rollback_version: "1.0".to_string(),
        };
        assert!(dep_break.is_critical());

        let version_conflict = RollbackWarning::VersionConflict {
            package: "pkg".to_string(),
            conflicting_package: "other".to_string(),
            reason: "test".to_string(),
        };
        assert!(version_conflict.is_critical());

        let advisory = RollbackWarning::AffectedDependent {
            package: "pkg".to_string(),
            dependent: "dep".to_string(),
        };
        assert!(!advisory.is_critical());

        let mixed = RollbackWarning::MixedSourceTransaction {
            repo_packages: vec!["a".to_string()],
            aur_packages: vec!["b".to_string()],
        };
        assert!(!mixed.is_critical());
    }

    #[test]
    fn test_rollback_warning_descriptions() {
        let dep_break = RollbackWarning::DependencyBreak {
            package: "glibc".to_string(),
            dependent: "bash".to_string(),
            required_version: ">=2.17".to_string(),
            rollback_version: "2.15".to_string(),
        };
        let desc = dep_break.description();
        assert!(desc.contains("bash"));
        assert!(desc.contains("glibc"));
        assert!(desc.contains(">=2.17"));
        assert!(desc.contains("2.15"));

        let orphan = RollbackWarning::OrphanedDependencies {
            package: "removed".to_string(),
            orphans: vec!["orphan1".to_string(), "orphan2".to_string()],
        };
        let desc = orphan.description();
        assert!(desc.contains("orphan1"));
        assert!(desc.contains("orphan2"));
    }

    // =========================================================================
    // Transaction Journal Tests
    // =========================================================================

    #[test]
    fn test_transaction_builder() {
        let dir = tempfile::tempdir().unwrap();
        let journal = TransactionJournal::at(dir.path().to_path_buf());
        let mut builder =
            journal.begin_transaction(TransactionOperation::Install, vec!["test-pkg".to_string()]);

        builder.add_package_change(PackageChange {
            name: "test-pkg".to_string(),
            source: Source::Aur,
            previous_version: None,
            new_version: Some("1.0.0".to_string()),
            previous_artifact: None,
            new_artifact: None,
            change_type: PackageChangeType::Install,
        });

        let record = builder.complete();
        assert!(record.id.starts_with("tx_"));
        assert_eq!(record.status, TransactionStatus::Completed);
        assert_eq!(record.affected_packages.len(), 1);
    }

    #[test]
    fn test_transaction_builder_fail() {
        let dir = tempfile::tempdir().unwrap();
        let journal = TransactionJournal::at(dir.path().to_path_buf());
        let builder =
            journal.begin_transaction(TransactionOperation::Install, vec!["test-pkg".to_string()]);

        let record = builder.fail("Build failed".to_string());
        assert!(matches!(record.status, TransactionStatus::Failed(_)));
    }

    #[test]
    fn test_journal_at_isolates_to_its_directory() {
        // A journal created with `at` must read and write only within the
        // injected directory, never the real ~/.local/share/reap history.
        let dir = tempfile::tempdir().unwrap();
        let journal = TransactionJournal::at(dir.path().to_path_buf());

        let mut builder =
            journal.begin_transaction(TransactionOperation::Install, vec!["iso-pkg".to_string()]);
        builder.add_package_change(PackageChange {
            name: "iso-pkg".to_string(),
            source: Source::Aur,
            previous_version: None,
            new_version: Some("1.0.0".to_string()),
            previous_artifact: None,
            new_artifact: None,
            change_type: PackageChangeType::Install,
        });
        let record = builder.complete();
        journal.save_transaction(&record).unwrap();

        // The record is loadable from the same temp journal...
        let loaded = journal.load_transaction(&record.id).unwrap();
        assert_eq!(loaded.id, record.id);
        // ...and the file physically lives under the injected directory.
        assert!(dir.path().join(format!("{}.json", record.id)).exists());
        assert_eq!(journal.list_transactions(None).unwrap().len(), 1);
    }

    // =========================================================================
    // Rollback Attempt Classification Tests
    // =========================================================================

    fn rollback_result(
        restored: &[&str],
        removed: &[&str],
        failed: &[(&str, &str)],
        verification_passed: bool,
    ) -> RollbackResult {
        RollbackResult {
            success: failed.is_empty(),
            packages_restored: restored.iter().map(|s| s.to_string()).collect(),
            packages_removed: removed.iter().map(|s| s.to_string()).collect(),
            packages_failed: failed
                .iter()
                .map(|(p, r)| (p.to_string(), r.to_string()))
                .collect(),
            verification_passed,
        }
    }

    #[test]
    fn test_classify_rollback_attempt_success() {
        let result = rollback_result(&["pkg"], &[], &[], true);
        assert_eq!(
            classify_rollback_attempt(&result),
            RollbackAttemptResult::Success
        );
    }

    #[test]
    fn test_classify_rollback_attempt_partial_on_failed_package() {
        let result = rollback_result(&["pkg-a"], &[], &[("pkg-b", "pacman -U failed")], true);
        assert_eq!(
            classify_rollback_attempt(&result),
            RollbackAttemptResult::PartialSuccess
        );
    }

    #[test]
    fn test_classify_rollback_attempt_all_failed() {
        let result = rollback_result(&[], &[], &[("pkg", "artifact missing")], false);
        assert!(matches!(
            classify_rollback_attempt(&result),
            RollbackAttemptResult::Failed { .. }
        ));
    }

    #[test]
    fn test_classify_rollback_attempt_verification_failure_is_partial() {
        // Every package operation reported success, but post-rollback
        // verification did not confirm the expected on-disk state. This must
        // be PartialSuccess, not Success, so the journal records the doubt.
        let result = rollback_result(&["pkg"], &[], &[], false);
        assert_eq!(
            classify_rollback_attempt(&result),
            RollbackAttemptResult::PartialSuccess
        );
    }
}
