//! Flatpak Integration Module
//!
//! Provides complete Flatpak package management functionality including
//! search, install, remove, list, upgrade, and security auditing.

use crate::aur::SearchResult;
use crate::core::Source;
use anyhow::{Context, Result, anyhow};
use std::process::Command;

/// Information about an installed Flatpak application
#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    pub origin: String,
    pub installation: String, // "system" or "user"
    pub size: String,
}

/// Detailed information about a Flatpak application
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    pub origin: String,
    pub commit: String,
    pub installation: String,
    pub installed_size: String,
    pub runtime: String,
    pub sdk: String,
}

/// Security audit result for a Flatpak application
#[derive(Debug, Clone)]
pub struct SecurityAudit {
    pub app_id: String,
    pub permissions: Vec<String>,
    pub filesystem_access: Vec<String>,
    pub device_access: Vec<String>,
    pub socket_access: Vec<String>,
    pub is_sandboxed: bool,
    pub risk_level: RiskLevel,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "Low"),
            RiskLevel::Medium => write!(f, "Medium"),
            RiskLevel::High => write!(f, "High"),
        }
    }
}

/// Check if flatpak command is available
pub fn is_available() -> bool {
    Command::new("flatpak")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get flatpak version string
pub fn version() -> Option<String> {
    Command::new("flatpak")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Search for Flatpak applications
pub fn search(query: &str) -> Result<Vec<SearchResult>> {
    if !is_available() {
        return Err(anyhow!(
            "Flatpak is not installed. Install with: sudo pacman -S flatpak"
        ));
    }

    let output = Command::new("flatpak")
        .arg("search")
        .arg("--columns=name,application,version,branch,remotes,description")
        .arg(query)
        .output()
        .context("Failed to execute flatpak search")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No matches found") || stderr.trim().is_empty() {
            return Ok(vec![]);
        }
        return Err(anyhow!("Flatpak search failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    // Skip header line if present
    let lines: Vec<&str> = stdout.lines().collect();
    let start_idx = if lines.first().is_some_and(|l| l.contains("Application ID")) {
        1
    } else {
        0
    };

    for line in lines.iter().skip(start_idx) {
        if line.trim().is_empty() {
            continue;
        }

        // Parse tab-separated output
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 2 {
            let name = fields.first().unwrap_or(&"").trim().to_string();
            let app_id = fields.get(1).unwrap_or(&"").trim().to_string();
            let version = fields.get(2).unwrap_or(&"").trim().to_string();
            let description = fields.get(5).unwrap_or(&"").trim().to_string();

            if !app_id.is_empty() {
                results.push(SearchResult {
                    name: app_id.clone(),
                    version: if version.is_empty() {
                        "latest".to_string()
                    } else {
                        version
                    },
                    description: if description.is_empty() {
                        name
                    } else {
                        format!("{} - {}", name, description)
                    },
                    source: Source::Flatpak,
                });
            }
        }
    }

    Ok(results)
}

/// Install a Flatpak application
pub fn install(app_id: &str) -> Result<()> {
    if !is_available() {
        return Err(anyhow!(
            "Flatpak is not installed. Install with: sudo pacman -S flatpak"
        ));
    }

    println!("[flatpak] Installing {}...", app_id);

    // First try to install from flathub
    let status = Command::new("flatpak")
        .arg("install")
        .arg("--noninteractive")
        .arg("-y")
        .arg("flathub")
        .arg(app_id)
        .status()
        .context("Failed to execute flatpak install")?;

    if status.success() {
        println!("[flatpak] Successfully installed: {}", app_id);
        return Ok(());
    }

    // Retry without specifying remote
    let retry_status = Command::new("flatpak")
        .arg("install")
        .arg("--noninteractive")
        .arg("-y")
        .arg(app_id)
        .status()
        .context("Failed to execute flatpak install retry")?;

    if retry_status.success() {
        println!("[flatpak] Successfully installed: {}", app_id);
        Ok(())
    } else {
        Err(anyhow!(
            "Failed to install {}. Ensure the app ID is correct and Flathub is configured.\n\
             Try: flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo",
            app_id
        ))
    }
}

/// Remove a Flatpak application
pub fn remove(app_id: &str) -> Result<()> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    println!("[flatpak] Removing {}...", app_id);

    let status = Command::new("flatpak")
        .arg("uninstall")
        .arg("--noninteractive")
        .arg("-y")
        .arg(app_id)
        .status()
        .context("Failed to execute flatpak uninstall")?;

    if status.success() {
        println!("[flatpak] Successfully removed: {}", app_id);
        Ok(())
    } else {
        Err(anyhow!("Failed to remove {}. Is it installed?", app_id))
    }
}

/// List installed Flatpak applications
pub fn list() -> Result<Vec<InstalledApp>> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    let output = Command::new("flatpak")
        .arg("list")
        .arg("--app")
        .arg("--columns=application,name,version,branch,origin,installation,size")
        .output()
        .context("Failed to execute flatpak list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Flatpak list failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut apps = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 4 {
            apps.push(InstalledApp {
                app_id: fields.first().unwrap_or(&"").trim().to_string(),
                name: fields.get(1).unwrap_or(&"").trim().to_string(),
                version: fields.get(2).unwrap_or(&"").trim().to_string(),
                branch: fields.get(3).unwrap_or(&"").trim().to_string(),
                origin: fields.get(4).unwrap_or(&"").trim().to_string(),
                installation: fields.get(5).unwrap_or(&"system").trim().to_string(),
                size: fields.get(6).unwrap_or(&"").trim().to_string(),
            });
        }
    }

    Ok(apps)
}

/// Upgrade all installed Flatpak applications
pub fn upgrade() -> Result<()> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    println!("[flatpak] Upgrading all applications...");

    let status = Command::new("flatpak")
        .arg("update")
        .arg("--noninteractive")
        .arg("-y")
        .status()
        .context("Failed to execute flatpak update")?;

    if status.success() {
        println!("[flatpak] All applications upgraded!");
        Ok(())
    } else {
        Err(anyhow!("Flatpak upgrade failed"))
    }
}

/// Update Flatpak metadata (appstream data)
pub fn update_metadata() -> Result<()> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    println!("[flatpak] Updating metadata...");

    let status = Command::new("flatpak")
        .arg("update")
        .arg("--appstream")
        .status()
        .context("Failed to update flatpak metadata")?;

    if status.success() {
        println!("[flatpak] Metadata updated!");
        Ok(())
    } else {
        Err(anyhow!("Failed to update metadata"))
    }
}

/// Get detailed information about a Flatpak application
pub fn info(app_id: &str) -> Result<AppInfo> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    let output = Command::new("flatpak")
        .arg("info")
        .arg(app_id)
        .output()
        .context("Failed to execute flatpak info")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Failed to get info for {}: {}",
            app_id,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut info = AppInfo {
        app_id: app_id.to_string(),
        name: String::new(),
        version: String::new(),
        branch: String::new(),
        origin: String::new(),
        commit: String::new(),
        installation: String::new(),
        installed_size: String::new(),
        runtime: String::new(),
        sdk: String::new(),
    };

    for line in stdout.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();

            match key.as_str() {
                "name" => info.name = value,
                "version" => info.version = value,
                "branch" => info.branch = value,
                "origin" => info.origin = value,
                "commit" => info.commit = value,
                "installation" => info.installation = value,
                "installed size" => info.installed_size = value,
                "runtime" => info.runtime = value,
                "sdk" => info.sdk = value,
                _ => {}
            }
        }
    }

    Ok(info)
}

/// Perform security audit on a Flatpak application
pub fn audit(app_id: &str) -> Result<SecurityAudit> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    // Get metadata permissions
    let output = Command::new("flatpak")
        .arg("info")
        .arg("--show-permissions")
        .arg(app_id)
        .output()
        .context("Failed to get permissions")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to audit {}: {}", app_id, stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut audit = SecurityAudit {
        app_id: app_id.to_string(),
        permissions: Vec::new(),
        filesystem_access: Vec::new(),
        device_access: Vec::new(),
        socket_access: Vec::new(),
        is_sandboxed: true,
        risk_level: RiskLevel::Low,
        warnings: Vec::new(),
    };

    let mut current_section = String::new();

    for line in stdout.lines() {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            continue;
        }

        if line.is_empty() {
            continue;
        }

        match current_section.as_str() {
            "Context" => {
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();

                    match key {
                        "shared" => {
                            for item in value.split(';').filter(|s| !s.is_empty()) {
                                audit.permissions.push(format!("shared:{}", item));
                                if item == "network" {
                                    audit.warnings.push("Has network access".to_string());
                                }
                            }
                        }
                        "sockets" => {
                            for item in value.split(';').filter(|s| !s.is_empty()) {
                                audit.socket_access.push(item.to_string());
                                if item == "x11" {
                                    audit
                                        .warnings
                                        .push("Uses X11 (less secure than Wayland)".to_string());
                                }
                                if item == "session-bus" || item == "system-bus" {
                                    audit.warnings.push(format!("Has {} D-Bus access", item));
                                }
                            }
                        }
                        "devices" => {
                            for item in value.split(';').filter(|s| !s.is_empty()) {
                                audit.device_access.push(item.to_string());
                                if item == "all" {
                                    audit.warnings.push("Has access to ALL devices".to_string());
                                    audit.risk_level = RiskLevel::High;
                                }
                            }
                        }
                        "filesystems" => {
                            for item in value.split(';').filter(|s| !s.is_empty()) {
                                audit.filesystem_access.push(item.to_string());
                                if item == "home" || item == "host" {
                                    audit
                                        .warnings
                                        .push(format!("Has {} filesystem access", item));
                                    if item == "host" {
                                        audit.risk_level = RiskLevel::High;
                                    } else if audit.risk_level == RiskLevel::Low {
                                        audit.risk_level = RiskLevel::Medium;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "Session Bus Policy" | "System Bus Policy" => {
                audit.permissions.push(format!("dbus:{}", line));
            }
            _ => {}
        }
    }

    // Check if sandboxed
    if audit.filesystem_access.contains(&"host".to_string())
        && audit.device_access.contains(&"all".to_string())
    {
        audit.is_sandboxed = false;
        audit
            .warnings
            .push("Effectively NOT sandboxed (host filesystem + all devices)".to_string());
        audit.risk_level = RiskLevel::High;
    }

    Ok(audit)
}

/// List configured remotes
pub fn list_remotes() -> Result<Vec<(String, String)>> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    let output = Command::new("flatpak")
        .arg("remotes")
        .arg("--columns=name,url")
        .output()
        .context("Failed to list remotes")?;

    if !output.status.success() {
        return Err(anyhow!("Failed to list remotes"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let remotes: Vec<(String, String)> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            } else {
                None
            }
        })
        .collect();

    Ok(remotes)
}

/// Check if Flathub remote is configured
pub fn has_flathub() -> bool {
    list_remotes()
        .map(|remotes| remotes.iter().any(|(name, _)| name == "flathub"))
        .unwrap_or(false)
}

/// Add Flathub remote if not present
pub fn ensure_flathub() -> Result<()> {
    if has_flathub() {
        return Ok(());
    }

    println!("[flatpak] Adding Flathub repository...");

    let status = Command::new("flatpak")
        .arg("remote-add")
        .arg("--if-not-exists")
        .arg("flathub")
        .arg("https://dl.flathub.org/repo/flathub.flatpakrepo")
        .status()
        .context("Failed to add Flathub")?;

    if status.success() {
        println!("[flatpak] Flathub repository added!");
        Ok(())
    } else {
        Err(anyhow!("Failed to add Flathub repository"))
    }
}

/// Get list of applications with available updates
pub fn check_updates() -> Result<Vec<String>> {
    if !is_available() {
        return Err(anyhow!("Flatpak is not installed"));
    }

    let output = Command::new("flatpak")
        .arg("remote-ls")
        .arg("--updates")
        .arg("--app")
        .arg("--columns=application")
        .output()
        .context("Failed to check updates")?;

    if !output.status.success() {
        return Ok(vec![]); // No updates or error
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let updates: Vec<String> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect();

    Ok(updates)
}

/// Display formatted output for list command
pub fn display_list(apps: &[InstalledApp]) {
    if apps.is_empty() {
        println!("No Flatpak applications installed.");
        return;
    }

    println!(
        "{:<40} {:<15} {:<10} {:<10} {:<8} {:<10}",
        "Application", "Name", "Version", "Branch", "Install", "Size"
    );
    println!("{}", "-".repeat(100));

    for app in apps {
        println!(
            "{:<40} {:<15} {:<10} {:<10} {:<8} {:<10}",
            truncate(&app.app_id, 40),
            truncate(&app.name, 15),
            truncate(&app.version, 10),
            truncate(&app.branch, 10),
            truncate(&app.installation, 8),
            truncate(&app.size, 10),
        );
    }

    println!(
        "\nTotal: {} applications (origin: {})",
        apps.len(),
        apps.first().map(|a| a.origin.as_str()).unwrap_or("unknown")
    );
}

/// Display formatted output for info command
pub fn display_info(info: &AppInfo) {
    println!("Application: {}", info.app_id);
    println!("Name:        {}", info.name);
    println!("Version:     {}", info.version);
    println!("Branch:      {}", info.branch);
    println!("Origin:      {}", info.origin);
    println!("Commit:      {}", truncate(&info.commit, 40));
    println!("Installation:{}", info.installation);
    println!("Size:        {}", info.installed_size);
    println!("Runtime:     {}", info.runtime);
    if !info.sdk.is_empty() {
        println!("SDK:         {}", info.sdk);
    }
}

/// Display formatted output for audit command
pub fn display_audit(audit: &SecurityAudit) {
    println!("Security Audit: {}", audit.app_id);
    println!("{}", "=".repeat(50));

    // Risk level with color indicator
    let risk_indicator = match audit.risk_level {
        RiskLevel::Low => "LOW",
        RiskLevel::Medium => "MEDIUM",
        RiskLevel::High => "HIGH",
    };
    println!("Risk Level: {}", risk_indicator);
    println!(
        "Sandboxed:  {}",
        if audit.is_sandboxed { "Yes" } else { "NO" }
    );
    println!();

    if !audit.filesystem_access.is_empty() {
        println!("Filesystem Access:");
        for fs in &audit.filesystem_access {
            println!("  - {}", fs);
        }
        println!();
    }

    if !audit.socket_access.is_empty() {
        println!("Socket Access:");
        for socket in &audit.socket_access {
            println!("  - {}", socket);
        }
        println!();
    }

    if !audit.device_access.is_empty() {
        println!("Device Access:");
        for device in &audit.device_access {
            println!("  - {}", device);
        }
        println!();
    }

    if !audit.warnings.is_empty() {
        println!("Warnings:");
        for warning in &audit.warnings {
            println!("  ! {}", warning);
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available() {
        // Just test that the function doesn't panic
        let _ = is_available();
    }

    #[test]
    fn test_version() {
        // Just test that the function doesn't panic
        let _ = version();
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(format!("{}", RiskLevel::Low), "Low");
        assert_eq!(format!("{}", RiskLevel::Medium), "Medium");
        assert_eq!(format!("{}", RiskLevel::High), "High");
    }
}
