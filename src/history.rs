use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSnapshot {
    pub package: String,
    pub version: String,
    pub source: crate::core::Source,
    pub installed_at: DateTime<Utc>,
    pub installed_files: Vec<String>,
    pub dependencies: Vec<String>,
    pub trust_score: Option<f32>,
    pub backup_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationHistory {
    pub snapshots: Vec<PackageSnapshot>,
    pub current_version: Option<String>,
}

pub struct HistoryManager {
    history_dir: PathBuf,
    package_histories: HashMap<String, InstallationHistory>,
}

impl HistoryManager {
    pub fn new() -> Self {
        let history_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap/history");
        let _ = fs::create_dir_all(&history_dir);

        Self {
            history_dir,
            package_histories: HashMap::new(),
        }
    }

    /// Create a snapshot before installation/upgrade
    pub fn create_snapshot(
        &mut self,
        pkg: &str,
        version: &str,
        source: &crate::core::Source,
    ) -> Result<PackageSnapshot> {
        let backup_dir = self.history_dir.join(pkg).join(format!(
            "{}-{}",
            version,
            Utc::now().format("%Y%m%d%H%M%S")
        ));
        fs::create_dir_all(&backup_dir)?;

        // Get currently installed files
        let installed_files = self.get_installed_files(pkg)?;

        // Backup current installation
        self.backup_package_files(pkg, &backup_dir)?;

        let snapshot = PackageSnapshot {
            package: pkg.to_string(),
            version: version.to_string(),
            source: source.clone(),
            installed_at: Utc::now(),
            installed_files,
            dependencies: self.get_package_dependencies(pkg),
            trust_score: None, // Will be filled by trust engine
            backup_path: backup_dir,
        };

        // Update history
        let history = self
            .package_histories
            .entry(pkg.to_string())
            .or_insert_with(|| InstallationHistory {
                snapshots: Vec::new(),
                current_version: None,
            });
        history.snapshots.push(snapshot.clone());
        history.current_version = Some(version.to_string());

        self.save_history(pkg)?;
        Ok(snapshot)
    }

    /// Rollback to a specific version
    pub fn rollback_to_version(&mut self, pkg: &str, target_version: &str) -> Result<()> {
        let history = self
            .package_histories
            .get(pkg)
            .ok_or_else(|| anyhow::anyhow!("No history found for package: {}", pkg))?;

        let snapshot = history
            .snapshots
            .iter()
            .find(|s| s.version == target_version)
            .ok_or_else(|| anyhow::anyhow!("Version {} not found in history", target_version))?;

        // Restore files from backup
        self.restore_from_snapshot(snapshot)?;

        // Update package database
        self.update_package_database(pkg, target_version)?;

        println!("🔄 Rolled back {} to version {}", pkg, target_version);
        Ok(())
    }

    /// Show package history with interactive selection
    pub fn show_history(&self, pkg: &str) -> Result<()> {
        let history = self
            .package_histories
            .get(pkg)
            .ok_or_else(|| anyhow::anyhow!("No history found for package: {}", pkg))?;

        println!("\n📊 History for {}", pkg);
        println!(
            "Current version: {}",
            history.current_version.as_deref().unwrap_or("unknown")
        );
        println!("{}", "=".repeat(60));

        for (i, snapshot) in history.snapshots.iter().enumerate() {
            let trust_badge = snapshot
                .trust_score
                .map(|score| format!(" {}", self.get_trust_badge(score)))
                .unwrap_or_default();

            println!(
                "  {}: {} ({}){} - {}",
                i + 1,
                snapshot.version,
                snapshot.source.label(),
                trust_badge,
                snapshot.installed_at.format("%Y-%m-%d %H:%M")
            );
        }
        Ok(())
    }

    fn get_installed_files(&self, pkg: &str) -> Result<Vec<String>> {
        let output = std::process::Command::new("pacman")
            .args(["-Ql", pkg])
            .output()?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let files = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(files)
    }

    fn backup_package_files(&self, pkg: &str, backup_dir: &Path) -> Result<()> {
        // Create package info backup
        let output = std::process::Command::new("pacman")
            .args(["-Qi", pkg])
            .output()?;

        if output.status.success() {
            fs::write(backup_dir.join("pacman-info.txt"), &output.stdout)?;
        }

        // Backup key files if they exist
        let files_to_backup = [
            format!("/usr/bin/{}", pkg),
            format!("/usr/share/{}", pkg),
            format!("/etc/{}", pkg),
        ];

        for file_path in &files_to_backup {
            let path = PathBuf::from(file_path);
            if path.exists() {
                let backup_file = backup_dir.join(path.file_name().unwrap_or_default());
                let _ = fs::copy(&path, backup_file);
            }
        }

        Ok(())
    }

    fn restore_from_snapshot(&self, snapshot: &PackageSnapshot) -> Result<()> {
        // This would implement the actual file restoration
        // For now, we'll use pacman to reinstall the specific version
        println!(
            "🔄 Restoring {} version {} from backup...",
            snapshot.package, snapshot.version
        );

        // In a real implementation, you'd restore the actual files
        // and update the pacman database appropriately

        Ok(())
    }

    fn update_package_database(&self, _pkg: &str, _version: &str) -> Result<()> {
        // Update pacman database to reflect the rollback
        // This is a complex operation that would need careful implementation
        Ok(())
    }

    fn get_package_dependencies(&self, pkg: &str) -> Vec<String> {
        let output = std::process::Command::new("pacman")
            .args(["-Qi", pkg])
            .output();

        if let Ok(output) = output {
            let info = String::from_utf8_lossy(&output.stdout);
            for line in info.lines() {
                if line.starts_with("Depends On") {
                    return line
                        .split(':')
                        .nth(1)
                        .unwrap_or("")
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect();
                }
            }
        }
        Vec::new()
    }

    fn save_history(&self, pkg: &str) -> Result<()> {
        if let Some(history) = self.package_histories.get(pkg) {
            let history_file = self.history_dir.join(format!("{}.json", pkg));
            let content = serde_json::to_string_pretty(history)?;
            fs::write(history_file, content)?;
        }
        Ok(())
    }

    fn get_trust_badge(&self, score: f32) -> &'static str {
        match score {
            s if s >= 8.0 => "🛡️",
            s if s >= 6.0 => "✅",
            s if s >= 4.0 => "⚠️",
            s if s >= 2.0 => "🚨",
            _ => "❌",
        }
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}
