//! Debian Package (.deb) Support
//!
//! Provides local .deb file handling for Arch Linux users.
//! This is a convenience feature for installing software that only ships as .deb.
//!
//! # Commands
//!
//! - `info` - Inspect package metadata before installing
//! - `install` - Convert to Arch package and install (uses debtap)

use anyhow::{Context, Result, anyhow};
use std::path::Path;
use std::process::Command;

/// Package metadata extracted from a .deb file
#[derive(Debug, Clone, Default)]
pub struct DebPackageInfo {
    pub name: String,
    pub version: String,
    pub architecture: String,
    pub maintainer: String,
    pub description: String,
    pub homepage: String,
    pub section: String,
    pub installed_size: String,
    pub depends: Vec<String>,
}

/// Check if dpkg-deb is available
pub fn has_dpkg_deb() -> bool {
    Command::new("dpkg-deb")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if debtap is available
pub fn has_debtap() -> bool {
    Command::new("which")
        .arg("debtap")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get package info from a .deb file
pub fn info(deb_path: &Path) -> Result<DebPackageInfo> {
    // Validate file exists
    if !deb_path.exists() {
        return Err(anyhow!("File not found: {}", deb_path.display()));
    }

    // Validate extension
    let ext = deb_path.extension().and_then(|e| e.to_str());
    if ext != Some("deb") {
        return Err(anyhow!("Not a .deb file: {}", deb_path.display()));
    }

    // Check for dpkg-deb
    if !has_dpkg_deb() {
        return Err(anyhow!(
            "dpkg-deb not found.\nInstall with: sudo pacman -S dpkg"
        ));
    }

    // Run dpkg-deb --info
    let output = Command::new("dpkg-deb")
        .arg("--info")
        .arg(deb_path)
        .output()
        .context("Failed to run dpkg-deb")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("dpkg-deb failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_deb_info(&stdout)
}

fn parse_deb_info(output: &str) -> Result<DebPackageInfo> {
    let mut info = DebPackageInfo::default();

    for line in output.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim();

            match key.as_str() {
                "package" => info.name = value.to_string(),
                "version" => info.version = value.to_string(),
                "architecture" => info.architecture = value.to_string(),
                "maintainer" => info.maintainer = value.to_string(),
                "description" => info.description = value.to_string(),
                "homepage" => info.homepage = value.to_string(),
                "section" => info.section = value.to_string(),
                "installed-size" => info.installed_size = format!("{} KB", value),
                "depends" => {
                    info.depends = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    if info.name.is_empty() {
        return Err(anyhow!("Could not parse package name from .deb file"));
    }

    Ok(info)
}

/// Install a .deb file using debtap for conversion
pub fn install(deb_path: &Path) -> Result<()> {
    // Validate file exists
    if !deb_path.exists() {
        return Err(anyhow!("File not found: {}", deb_path.display()));
    }

    // Validate extension
    let ext = deb_path.extension().and_then(|e| e.to_str());
    if ext != Some("deb") {
        return Err(anyhow!("Not a .deb file: {}", deb_path.display()));
    }

    // Check for debtap
    if !has_debtap() {
        return Err(anyhow!(
            "debtap not found.\n\
             Install with: reap install debtap\n\
             Then initialize: sudo debtap -u"
        ));
    }

    // Show package info first
    println!("[dpkg] Inspecting package...\n");
    if let Ok(pkg_info) = info(deb_path) {
        println!("  Name:    {}", pkg_info.name);
        println!("  Version: {}", pkg_info.version);
        println!("  Arch:    {}", pkg_info.architecture);
        if !pkg_info.depends.is_empty() {
            println!("  Deps:    {} dependencies", pkg_info.depends.len());
        }
        println!();
    }

    println!("[dpkg] Converting to Arch package...");

    // Run debtap
    let debtap_status = Command::new("debtap")
        .arg("-q") // Quiet mode - skip prompts
        .arg(deb_path)
        .status()
        .context("Failed to run debtap")?;

    if !debtap_status.success() {
        return Err(anyhow!(
            "debtap conversion failed.\n\
             Make sure you've run 'sudo debtap -u' to initialize the database."
        ));
    }

    // Find the generated .pkg.tar.* file
    let pkg_file = find_generated_package(deb_path)?;

    println!("[dpkg] Installing with pacman...");

    // Install with pacman
    let pacman_status = Command::new("sudo")
        .arg("pacman")
        .arg("-U")
        .arg("--noconfirm")
        .arg(&pkg_file)
        .status()
        .context("Failed to run pacman")?;

    // Clean up generated package file
    if pkg_file.exists() {
        let _ = std::fs::remove_file(&pkg_file);
    }

    if pacman_status.success() {
        println!("[dpkg] Installation complete!");
        Ok(())
    } else {
        Err(anyhow!("pacman installation failed"))
    }
}

/// Find the .pkg.tar.* file generated by debtap
fn find_generated_package(deb_path: &Path) -> Result<std::path::PathBuf> {
    let dir = deb_path.parent().unwrap_or(Path::new("."));
    let stem = deb_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("package");

    // debtap generates files like: packagename-version-1-x86_64.pkg.tar.zst
    // Look for any .pkg.tar.* file that matches
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name.contains(".pkg.tar.") && name.starts_with(stem) {
                return Ok(path);
            }
        }
    }

    // Also check current directory
    if let Ok(entries) = std::fs::read_dir(".") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name.contains(".pkg.tar.") && name.starts_with(stem) {
                return Ok(path);
            }
        }
    }

    Err(anyhow!(
        "Could not find generated Arch package.\n\
         debtap may have failed silently or output to an unexpected location."
    ))
}

/// Display package info in formatted output
pub fn display_info(info: &DebPackageInfo) {
    println!("Debian Package Info");
    println!("{}", "=".repeat(50));
    println!("Name:         {}", info.name);
    println!("Version:      {}", info.version);
    println!("Architecture: {}", info.architecture);

    if !info.section.is_empty() {
        println!("Section:      {}", info.section);
    }
    if !info.installed_size.is_empty() {
        println!("Size:         {}", info.installed_size);
    }
    if !info.homepage.is_empty() {
        println!("Homepage:     {}", info.homepage);
    }
    if !info.maintainer.is_empty() {
        println!("Maintainer:   {}", info.maintainer);
    }

    println!("\nDescription:");
    println!("  {}", info.description);

    if !info.depends.is_empty() {
        println!("\nDebian Dependencies ({}):", info.depends.len());
        for dep in &info.depends {
            println!("  - {}", dep);
        }
        println!("\nNote: Dependencies are Debian package names.");
        println!("      debtap will attempt to map them to Arch packages.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deb_info() {
        let sample = r#"
 Package: example-app
 Version: 1.2.3
 Architecture: amd64
 Maintainer: Test User <test@example.com>
 Installed-Size: 5432
 Depends: libc6 (>= 2.17), libssl1.1, zlib1g
 Section: utils
 Description: An example application
 Homepage: https://example.com
"#;
        let info = parse_deb_info(sample).unwrap();
        assert_eq!(info.name, "example-app");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.architecture, "amd64");
        assert_eq!(info.depends.len(), 3);
        assert!(info.depends.contains(&"libc6 (>= 2.17)".to_string()));
    }

    #[test]
    fn test_has_dpkg_deb() {
        // Just verify it doesn't panic
        let _ = has_dpkg_deb();
    }

    #[test]
    fn test_has_debtap() {
        // Just verify it doesn't panic
        let _ = has_debtap();
    }
}
