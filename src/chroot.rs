use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use crate::config::ReapConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChrootConfig {
    pub root_dir: PathBuf,
    pub pacman_conf: PathBuf,
    pub makepkg_conf: PathBuf,
    pub copy_files: Vec<PathBuf>,
    pub bind_mounts: Vec<BindMount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    pub source: PathBuf,
    pub target: PathBuf,
    pub read_only: bool,
}

pub struct ChrootManager {
    config: ChrootConfig,
    chroot_id: String,
}

impl ChrootManager {
    pub fn new(config: &ReapConfig) -> Result<Self> {
        let chroot_id = format!("reap-{}", std::process::id());

        let chroot_config = ChrootConfig {
            root_dir: config.build.chroot_dir.join(&chroot_id),
            pacman_conf: PathBuf::from("/etc/pacman.conf"),
            makepkg_conf: PathBuf::from("/etc/makepkg.conf"),
            copy_files: vec![
                PathBuf::from("/etc/resolv.conf"),
                PathBuf::from("/etc/hosts"),
            ],
            bind_mounts: vec![
                BindMount {
                    source: PathBuf::from("/var/cache/pacman/pkg"),
                    target: PathBuf::from("var/cache/pacman/pkg"),
                    read_only: false,
                },
                BindMount {
                    source: PathBuf::from("/tmp"),
                    target: PathBuf::from("tmp"),
                    read_only: false,
                },
            ],
        };

        Ok(Self {
            config: chroot_config,
            chroot_id,
        })
    }

    pub fn create_chroot(&self) -> Result<()> {
        println!("[chroot] Creating chroot environment: {}", self.chroot_id);

        // Create chroot directory structure
        self.create_directory_structure()?;

        // Install base system
        self.install_base_system()?;

        // Setup bind mounts
        self.setup_bind_mounts()?;

        // Copy configuration files
        self.copy_configuration_files()?;

        println!("[chroot] ✅ Chroot environment ready");
        Ok(())
    }

    pub fn build_package(&self, pkgbuild_dir: &Path, package_name: &str) -> Result<Vec<PathBuf>> {
        println!("[chroot] 🔨 Building {} in isolated environment", package_name);

        // Copy PKGBUILD and sources into chroot
        let chroot_build_dir = self.config.root_dir.join("build").join(package_name);
        fs::create_dir_all(&chroot_build_dir)?;

        self.copy_sources(pkgbuild_dir, &chroot_build_dir)?;

        // Install build dependencies in chroot
        self.install_build_dependencies(&chroot_build_dir)?;

        // Execute makepkg in chroot
        let built_packages = self.execute_makepkg(&chroot_build_dir)?;

        // Copy built packages out of chroot
        let output_packages = self.extract_built_packages(&built_packages)?;

        println!("[chroot] ✅ Package {} built successfully", package_name);
        Ok(output_packages)
    }

    pub fn cleanup(&self) -> Result<()> {
        println!("[chroot] 🧹 Cleaning up chroot environment");

        // Unmount bind mounts
        self.cleanup_bind_mounts()?;

        // Remove chroot directory
        if self.config.root_dir.exists() {
            fs::remove_dir_all(&self.config.root_dir)
                .with_context(|| format!("Failed to remove chroot directory: {}",
                    self.config.root_dir.display()))?;
        }

        println!("[chroot] ✅ Cleanup complete");
        Ok(())
    }

    fn create_directory_structure(&self) -> Result<()> {
        let dirs = [
            "bin", "boot", "dev", "etc", "home", "lib", "lib64", "mnt",
            "opt", "proc", "root", "run", "sbin", "srv", "sys", "tmp",
            "usr", "var", "build",
        ];

        for dir in &dirs {
            let path = self.config.root_dir.join(dir);
            fs::create_dir_all(&path)
                .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        }

        // Create essential subdirectories
        let subdirs = [
            "usr/bin", "usr/lib", "usr/share", "usr/include",
            "var/cache/pacman/pkg", "var/lib/pacman",
            "etc/pacman.d", "home/builder",
        ];

        for subdir in &subdirs {
            let path = self.config.root_dir.join(subdir);
            fs::create_dir_all(&path)
                .with_context(|| format!("Failed to create subdirectory: {}", path.display()))?;
        }

        Ok(())
    }

    fn install_base_system(&self) -> Result<()> {
        println!("[chroot] Installing base system packages");

        let base_packages = [
            "base", "base-devel", "git", "sudo",
        ];

        let output = Command::new("pacstrap")
            .arg(&self.config.root_dir)
            .args(&base_packages)
            .output()
            .context("Failed to execute pacstrap")?;

        if !output.status.success() {
            return Err(anyhow!("Failed to install base system: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        // Create builder user
        self.create_builder_user()?;

        Ok(())
    }

    fn create_builder_user(&self) -> Result<()> {
        println!("[chroot] Creating builder user");

        let commands = [
            "useradd -m -G wheel builder",
            "echo 'builder ALL=(ALL) NOPASSWD: ALL' >> /etc/sudoers",
        ];

        for cmd in &commands {
            let output = Command::new("arch-chroot")
                .arg(&self.config.root_dir)
                .arg("bash")
                .arg("-c")
                .arg(cmd)
                .output()
                .context("Failed to execute arch-chroot")?;

            if !output.status.success() {
                return Err(anyhow!("Failed to create builder user: {}",
                    String::from_utf8_lossy(&output.stderr)));
            }
        }

        Ok(())
    }

    fn setup_bind_mounts(&self) -> Result<()> {
        println!("[chroot] Setting up bind mounts");

        for mount in &self.config.bind_mounts {
            let target_path = self.config.root_dir.join(&mount.target);

            // Ensure target directory exists
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Create the mount point if it doesn't exist
            if !target_path.exists() {
                if mount.source.is_dir() {
                    fs::create_dir_all(&target_path)?;
                } else {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::File::create(&target_path)?;
                }
            }

            // Perform bind mount
            let mut cmd = Command::new("mount");
            cmd.arg("--bind")
                .arg(&mount.source)
                .arg(&target_path);

            if mount.read_only {
                cmd.arg("-o").arg("ro");
            }

            let output = cmd.output()
                .context("Failed to execute mount")?;

            if !output.status.success() {
                return Err(anyhow!("Failed to bind mount {} to {}: {}",
                    mount.source.display(),
                    target_path.display(),
                    String::from_utf8_lossy(&output.stderr)));
            }
        }

        Ok(())
    }

    fn copy_configuration_files(&self) -> Result<()> {
        println!("[chroot] Copying configuration files");

        // Copy pacman.conf
        let target_pacman_conf = self.config.root_dir.join("etc/pacman.conf");
        fs::copy(&self.config.pacman_conf, &target_pacman_conf)
            .with_context(|| "Failed to copy pacman.conf")?;

        // Copy makepkg.conf
        let target_makepkg_conf = self.config.root_dir.join("etc/makepkg.conf");
        fs::copy(&self.config.makepkg_conf, &target_makepkg_conf)
            .with_context(|| "Failed to copy makepkg.conf")?;

        // Copy other configuration files
        for file in &self.config.copy_files {
            if file.exists() {
                let file_name = file.file_name()
                    .ok_or_else(|| anyhow!("Invalid file name: {}", file.display()))?;
                let target_file = self.config.root_dir.join("etc").join(file_name);

                fs::copy(file, &target_file)
                    .with_context(|| format!("Failed to copy {}", file.display()))?;
            }
        }

        Ok(())
    }

    fn copy_sources(&self, source_dir: &Path, target_dir: &Path) -> Result<()> {
        println!("[chroot] Copying sources to chroot");

        let output = Command::new("cp")
            .arg("-r")
            .arg(source_dir.to_str().unwrap())
            .arg(target_dir.parent().unwrap().to_str().unwrap())
            .output()
            .context("Failed to copy sources")?;

        if !output.status.success() {
            return Err(anyhow!("Failed to copy sources: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        // Set ownership to builder user
        let output = Command::new("arch-chroot")
            .arg(&self.config.root_dir)
            .arg("chown")
            .arg("-R")
            .arg("builder:builder")
            .arg(target_dir.strip_prefix(&self.config.root_dir)?)
            .output()
            .context("Failed to change ownership")?;

        if !output.status.success() {
            eprintln!("[chroot] Warning: Failed to change ownership: {}",
                String::from_utf8_lossy(&output.stderr));
        }

        Ok(())
    }

    fn install_build_dependencies(&self, build_dir: &Path) -> Result<()> {
        println!("[chroot] Installing build dependencies");

        // Parse PKGBUILD to extract dependencies
        let pkgbuild_path = build_dir.join("PKGBUILD");
        let dependencies = self.parse_pkgbuild_dependencies(&pkgbuild_path)?;

        if dependencies.is_empty() {
            println!("[chroot] No build dependencies required");
            return Ok(());
        }

        println!("[chroot] Installing dependencies: {}", dependencies.join(", "));

        let output = Command::new("arch-chroot")
            .arg(&self.config.root_dir)
            .arg("pacman")
            .arg("-S")
            .arg("--noconfirm")
            .args(&dependencies)
            .output()
            .context("Failed to install build dependencies")?;

        if !output.status.success() {
            return Err(anyhow!("Failed to install build dependencies: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    fn parse_pkgbuild_dependencies(&self, pkgbuild_path: &Path) -> Result<Vec<String>> {
        let content = fs::read_to_string(pkgbuild_path)
            .with_context(|| "Failed to read PKGBUILD")?;

        let mut dependencies = Vec::new();

        // Parse makedepends and depends arrays
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("makedepends=") || line.starts_with("depends=") {
                // Extract dependencies from array notation
                if let Some(deps_part) = line.split('=').nth(1) {
                    let deps_clean = deps_part
                        .trim_matches(&['(', ')', '"', '\''])
                        .trim();

                    for dep in deps_clean.split_whitespace() {
                        let dep_clean = dep.trim_matches(&['"', '\'']);
                        if !dep_clean.is_empty() && !dependencies.contains(&dep_clean.to_string()) {
                            dependencies.push(dep_clean.to_string());
                        }
                    }
                }
            }
        }

        Ok(dependencies)
    }

    fn execute_makepkg(&self, build_dir: &Path) -> Result<Vec<PathBuf>> {
        println!("[chroot] Executing makepkg");

        let build_path = build_dir.strip_prefix(&self.config.root_dir)?;

        let output = Command::new("arch-chroot")
            .arg(&self.config.root_dir)
            .arg("sudo")
            .arg("-u")
            .arg("builder")
            .arg("bash")
            .arg("-c")
            .arg(format!("cd {} && makepkg -s --noconfirm", build_path.display()))
            .output()
            .context("Failed to execute makepkg")?;

        if !output.status.success() {
            return Err(anyhow!("makepkg failed: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        // Find built packages
        let mut built_packages = Vec::new();
        for entry in fs::read_dir(build_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("zst") ||
               path.extension().and_then(|ext| ext.to_str()) == Some("xz") {
                built_packages.push(path);
            }
        }

        if built_packages.is_empty() {
            return Err(anyhow!("No packages were built"));
        }

        println!("[chroot] Built {} package(s)", built_packages.len());
        Ok(built_packages)
    }

    fn extract_built_packages(&self, built_packages: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let output_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap")
            .join("built_packages");

        fs::create_dir_all(&output_dir)?;

        let mut output_packages = Vec::new();

        for package in built_packages {
            let file_name = package.file_name()
                .ok_or_else(|| anyhow!("Invalid package filename"))?;
            let output_path = output_dir.join(file_name);

            fs::copy(package, &output_path)
                .with_context(|| format!("Failed to copy package to output directory"))?;

            output_packages.push(output_path);
        }

        Ok(output_packages)
    }

    fn cleanup_bind_mounts(&self) -> Result<()> {
        for mount in &self.config.bind_mounts {
            let target_path = self.config.root_dir.join(&mount.target);

            let output = Command::new("umount")
                .arg(&target_path)
                .output();

            if let Ok(output) = output {
                if !output.status.success() {
                    eprintln!("[chroot] Warning: Failed to unmount {}: {}",
                        target_path.display(),
                        String::from_utf8_lossy(&output.stderr));
                }
            }
        }

        Ok(())
    }

    pub fn is_available() -> bool {
        // Check if necessary tools are available
        let tools = ["pacstrap", "arch-chroot", "mount", "umount"];

        for tool in &tools {
            if Command::new("which").arg(tool).output().is_err() {
                return false;
            }
        }

        true
    }
}

impl Drop for ChrootManager {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            eprintln!("[chroot] Warning: Cleanup failed: {}", e);
        }
    }
}

// Helper function to build a package in a chroot environment
pub fn build_package_chroot(
    config: &ReapConfig,
    pkgbuild_dir: &Path,
    package_name: &str,
) -> Result<Vec<PathBuf>> {
    if !config.build.use_chroot {
        return Err(anyhow!("Chroot builds are disabled in configuration"));
    }

    if !ChrootManager::is_available() {
        return Err(anyhow!("Chroot tools are not available on this system"));
    }

    let chroot_manager = ChrootManager::new(config)?;
    chroot_manager.create_chroot()?;
    chroot_manager.build_package(pkgbuild_dir, package_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_chroot_config_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = ReapConfig::default();
        config.build.chroot_dir = temp_dir.path().to_path_buf();
        config.build.use_chroot = true;

        let chroot_manager = ChrootManager::new(&config);
        assert!(chroot_manager.is_ok());
    }

    #[test]
    fn test_dependency_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = ReapConfig::default();
        config.build.chroot_dir = temp_dir.path().to_path_buf();

        let chroot_manager = ChrootManager::new(&config).unwrap();

        // Create a mock PKGBUILD
        let pkgbuild_content = r#"
            pkgname=test-package
            makedepends=('gcc' 'make')
            depends=('glibc' 'bash')
        "#;

        let pkgbuild_path = temp_dir.path().join("PKGBUILD");
        std::fs::write(&pkgbuild_path, pkgbuild_content).unwrap();

        let deps = chroot_manager.parse_pkgbuild_dependencies(&pkgbuild_path).unwrap();
        assert!(deps.contains(&"gcc".to_string()));
        assert!(deps.contains(&"glibc".to_string()));
    }
}