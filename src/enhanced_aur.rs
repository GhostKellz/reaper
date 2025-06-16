use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
pub struct DependencyConflict {
    #[allow(dead_code)]
    pub package: String,
    #[allow(dead_code)]
    pub conflicting_with: String,
    #[allow(dead_code)]
    pub conflict_type: ConflictType,
    #[allow(dead_code)]
    pub resolution: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConflictType {
    FileConflict(String),
    PackageConflict,
    VersionConflict(String, String),
    CircularDependency,
}

pub struct EnhancedAurManager {
    cache_dir: PathBuf,
    pkgbuild_cache: HashMap<String, PkgbuildInfo>,
}

impl EnhancedAurManager {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap/aur");
        let _ = fs::create_dir_all(&cache_dir);

        Self {
            cache_dir,
            pkgbuild_cache: HashMap::new(),
        }
    }

    /// Manually retrieve PKGBUILD from AUR
    pub async fn fetch_pkgbuild(&mut self, package: &str) -> Result<PkgbuildInfo> {
        println!("[aur] Fetching PKGBUILD for {}", package);

        // Download PKGBUILD
        let pkgbuild_url = format!(
            "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}",
            package
        );
        let pkgbuild_content = reqwest::get(&pkgbuild_url).await?.text().await?;

        // Parse PKGBUILD
        let pkgbuild_info = self.parse_pkgbuild(package, &pkgbuild_content)?;

        // Cache it
        self.pkgbuild_cache
            .insert(package.to_string(), pkgbuild_info.clone());

        // Save to disk cache
        let cache_file = self.cache_dir.join(format!("{}.json", package));
        fs::write(cache_file, serde_json::to_string_pretty(&pkgbuild_info)?)?;

        Ok(pkgbuild_info)
    }

    /// Parse PKGBUILD content into structured info
    pub fn parse_pkgbuild(&self, package: &str, content: &str) -> Result<PkgbuildInfo> {
        let mut info = PkgbuildInfo {
            package: package.to_string(),
            version: String::new(),
            description: String::new(),
            dependencies: Vec::new(),
            make_dependencies: Vec::new(),
            conflicts: Vec::new(),
            provides: Vec::new(),
            source_files: Vec::new(),
            integrity_checks: Vec::new(),
        };

        let mut in_array = false;
        let mut current_array = String::new();
        let mut array_content = String::new();

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("pkgver=") {
                info.version = self.extract_value(trimmed, "pkgver=");
            } else if trimmed.starts_with("pkgdesc=") {
                info.description = self.extract_value(trimmed, "pkgdesc=");
            } else if trimmed.starts_with("depends=(") {
                if trimmed.ends_with(')') {
                    info.dependencies = self.parse_array_line(trimmed, "depends=");
                } else {
                    in_array = true;
                    current_array = "depends".to_string();
                    array_content = trimmed.trim_start_matches("depends=(").to_string();
                }
            } else if trimmed.starts_with("makedepends=(") {
                if trimmed.ends_with(')') {
                    info.make_dependencies = self.parse_array_line(trimmed, "makedepends=");
                } else {
                    in_array = true;
                    current_array = "makedepends".to_string();
                    array_content = trimmed.trim_start_matches("makedepends=(").to_string();
                }
            } else if trimmed.starts_with("conflicts=(") {
                if trimmed.ends_with(')') {
                    info.conflicts = self.parse_array_line(trimmed, "conflicts=");
                } else {
                    in_array = true;
                    current_array = "conflicts".to_string();
                    array_content = trimmed.trim_start_matches("conflicts=(").to_string();
                }
            } else if trimmed.starts_with("provides=(") {
                if trimmed.ends_with(')') {
                    info.provides = self.parse_array_line(trimmed, "provides=");
                } else {
                    in_array = true;
                    current_array = "provides".to_string();
                    array_content = trimmed.trim_start_matches("provides=(").to_string();
                }
            } else if in_array {
                array_content.push(' ');
                array_content.push_str(trimmed);
                if trimmed.ends_with(')') {
                    let items = self.parse_array_content(array_content.trim_end_matches(')'));
                    match current_array.as_str() {
                        "depends" => info.dependencies = items,
                        "makedepends" => info.make_dependencies = items,
                        "conflicts" => info.conflicts = items,
                        "provides" => info.provides = items,
                        _ => {}
                    }
                    in_array = false;
                    current_array.clear();
                    array_content.clear();
                }
            }
        }

        Ok(info)
    }

    fn extract_value(&self, line: &str, prefix: &str) -> String {
        line.trim_start_matches(prefix)
            .trim_matches('"')
            .trim_matches('\'')
            .to_string()
    }

    fn parse_array_line(&self, line: &str, prefix: &str) -> Vec<String> {
        let content = line
            .trim_start_matches(prefix)
            .trim_start_matches('(')
            .trim_end_matches(')');
        self.parse_array_content(content)
    }

    fn parse_array_content(&self, content: &str) -> Vec<String> {
        content
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
            .collect()
    }

    /// Advanced dependency resolution with conflict detection
    pub async fn resolve_dependencies_advanced(
        &mut self,
        packages: &[String],
    ) -> Result<Vec<DependencyConflict>> {
        let mut conflicts = Vec::new();
        let mut resolved_deps = HashSet::new();

        for package in packages {
            self.resolve_iterative(package, &mut resolved_deps, &mut conflicts)
                .await?;
        }

        Ok(conflicts)
    }

    async fn resolve_iterative(
        &mut self,
        root_package: &str,
        resolved: &mut HashSet<String>,
        conflicts: &mut Vec<DependencyConflict>,
    ) -> Result<()> {
        let mut stack = vec![root_package.to_string()];
        let mut checking = HashSet::new();

        while let Some(package) = stack.pop() {
            if resolved.contains(&package) {
                continue;
            }

            if checking.contains(&package) {
                conflicts.push(DependencyConflict {
                    package: package.clone(),
                    conflicting_with: package.clone(),
                    conflict_type: ConflictType::CircularDependency,
                    resolution: Some("Break circular dependency manually".to_string()),
                });
                continue;
            }

            checking.insert(package.clone());

            let pkgbuild = self.fetch_pkgbuild(&package).await?;

            // Check for conflicts with already resolved packages
            for conflict in &pkgbuild.conflicts {
                if resolved.contains(conflict) {
                    conflicts.push(DependencyConflict {
                        package: package.clone(),
                        conflicting_with: conflict.clone(),
                        conflict_type: ConflictType::PackageConflict,
                        resolution: Some(format!("Remove {} or choose alternative", conflict)),
                    });
                }
            }

            // Check for version conflicts
            conflicts.extend(
                self.check_version_conflicts(&package, &pkgbuild, resolved)
                    .await,
            );

            // Add dependencies to stack for processing
            for dep in &pkgbuild.dependencies {
                let clean_dep = self.clean_dependency_name(dep);
                if !resolved.contains(&clean_dep) && !checking.contains(&clean_dep) {
                    stack.push(clean_dep);
                }
            }

            resolved.insert(package);
        }

        Ok(())
    }

    /// Check for file conflicts
    pub fn check_file_conflicts(&self, packages: &[String]) -> Vec<DependencyConflict> {
        let mut conflicts = Vec::new();

        // Check for actual file conflicts by examining package contents
        for package in packages {
            if let Some(conflicting_files) = self.get_package_file_conflicts(package) {
                for (file_path, conflicting_pkg) in conflicting_files {
                    conflicts.push(DependencyConflict {
                        package: package.clone(),
                        conflicting_with: conflicting_pkg,
                        conflict_type: ConflictType::FileConflict(file_path),
                        resolution: Some(
                            "Use --force to override or remove conflicting package".to_string(),
                        ),
                    });
                }
            }
        }

        conflicts
    }

    fn get_package_file_conflicts(&self, package: &str) -> Option<Vec<(String, String)>> {
        // Query pacman for installed files and check for conflicts
        let output = Command::new("pacman")
            .args(&["-Ql", package])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let files = String::from_utf8_lossy(&output.stdout);
        let mut conflicts = Vec::new();

        for line in files.lines() {
            if let Some(file_path) = line.split_whitespace().nth(1) {
                // Check if this file is owned by another package
                if let Some(conflicting_pkg) = self.check_file_owner(file_path) {
                    if conflicting_pkg != package {
                        conflicts.push((file_path.to_string(), conflicting_pkg));
                    }
                }
            }
        }

        if conflicts.is_empty() {
            None
        } else {
            Some(conflicts)
        }
    }

    fn check_file_owner(&self, file_path: &str) -> Option<String> {
        let output = Command::new("pacman")
            .args(&["-Qo", file_path])
            .output()
            .ok()?;

        if output.status.success() {
            let owner_info = String::from_utf8_lossy(&output.stdout);
            // Parse output like: "/usr/bin/git is owned by git 2.41.0-1"
            owner_info.split_whitespace().nth(4).map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Get cached PKGBUILD info
    #[allow(dead_code)]
    pub fn get_cached_pkgbuild(&self, package: &str) -> Option<&PkgbuildInfo> {
        self.pkgbuild_cache.get(package)
    }

    /// Interactive PKGBUILD editor
    pub fn edit_pkgbuild(&self, package: &str) -> Result<()> {
        let pkgbuild_path = self.cache_dir.join(format!("{}/PKGBUILD", package));

        if !pkgbuild_path.exists() {
            return Err(anyhow::anyhow!("PKGBUILD not found for {}", package));
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
        let status = Command::new(editor).arg(&pkgbuild_path).status()?;

        if status.success() {
            println!("[aur] PKGBUILD edited successfully for {}", package);
        } else {
            return Err(anyhow::anyhow!("Editor exited with error"));
        }

        Ok(())
    }

    async fn check_version_conflicts(
        &self,
        package: &str,
        pkgbuild: &PkgbuildInfo,
        _resolved: &HashSet<String>,
    ) -> Vec<DependencyConflict> {
        let mut conflicts = Vec::new();

        // Check if installed version conflicts with required versions
        for dep in &pkgbuild.dependencies {
            if let Some((dep_name, required_version)) = self.parse_version_constraint(dep) {
                if let Some(installed_version) = self.get_installed_version(&dep_name) {
                    if !self.version_satisfies(&installed_version, &required_version) {
                        conflicts.push(DependencyConflict {
                            package: package.to_string(),
                            conflicting_with: dep_name.clone(),
                            conflict_type: ConflictType::VersionConflict(
                                installed_version,
                                required_version,
                            ),
                            resolution: Some(format!(
                                "Update {} to satisfy version requirement",
                                dep_name
                            )),
                        });
                    }
                }
            }
        }

        conflicts
    }

    fn parse_version_constraint(&self, dep: &str) -> Option<(String, String)> {
        if dep.contains(">=") {
            let parts: Vec<&str> = dep.split(">=").collect();
            if parts.len() == 2 {
                return Some((
                    parts[0].trim().to_string(),
                    format!(">={}", parts[1].trim()),
                ));
            }
        } else if dep.contains("<=") {
            let parts: Vec<&str> = dep.split("<=").collect();
            if parts.len() == 2 {
                return Some((
                    parts[0].trim().to_string(),
                    format!("<={}", parts[1].trim()),
                ));
            }
        } else if dep.contains('=') && !dep.contains(">=") && !dep.contains("<=") {
            let parts: Vec<&str> = dep.split('=').collect();
            if parts.len() == 2 {
                return Some((parts[0].trim().to_string(), format!("={}", parts[1].trim())));
            }
        }
        None
    }

    fn get_installed_version(&self, package: &str) -> Option<String> {
        let output = Command::new("pacman")
            .args(&["-Qi", package])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let info = String::from_utf8_lossy(&output.stdout);
        for line in info.lines() {
            if line.starts_with("Version") {
                return line.split(':').nth(1).map(|v| v.trim().to_string());
            }
        }
        None
    }

    fn version_satisfies(&self, installed: &str, requirement: &str) -> bool {
        // Simple version comparison - would need proper semver parsing for production
        if requirement.starts_with(">=") {
            let req_version = requirement.trim_start_matches(">=");
            installed >= req_version
        } else if requirement.starts_with("<=") {
            let req_version = requirement.trim_start_matches("<=");
            installed <= req_version
        } else if requirement.starts_with('=') {
            let req_version = requirement.trim_start_matches('=');
            installed == req_version
        } else {
            true // No specific requirement
        }
    }

    fn clean_dependency_name(&self, dep: &str) -> String {
        // Remove version constraints like >=1.0, <2.0, etc.
        dep.split_whitespace()
            .next()
            .unwrap_or(dep)
            .split(['>', '<', '='])
            .next()
            .unwrap_or(dep)
            .to_string()
    }
}

impl Default for EnhancedAurManager {
    fn default() -> Self {
        Self::new()
    }
}
