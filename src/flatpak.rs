use crate::aur::SearchResult;
use crate::core::Source;
use anyhow::Result;
use std::process::Command;

// Flatpak integration with improved error handling
pub fn search(query: &str) -> Vec<SearchResult> {
    // Check if flatpak is installed
    if !is_flatpak_available() {
        eprintln!("[reap] Warning: Flatpak is not installed or not in PATH");
        return vec![];
    }

    let output = Command::new("flatpak")
        .arg("search")
        .arg("--columns=name,application,version,branch,remotes,description")
        .arg(query)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut results = Vec::new();

            // Skip header line if present
            let lines: Vec<&str> = stdout.lines().collect();
            let start_idx = if lines
                .first()
                .map_or(false, |l| l.contains("Application ID"))
            {
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
                    // Format: Name<tab>App ID<tab>Version<tab>Branch<tab>Remotes<tab>Description
                    let name = fields.get(0).unwrap_or(&"").trim().to_string();
                    let app_id = fields.get(1).unwrap_or(&"").trim().to_string();
                    let version = fields.get(2).unwrap_or(&"").trim().to_string();
                    let description = fields.get(5).unwrap_or(&"").trim().to_string();

                    // Use app_id as the primary identifier for installation
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
            results
        }
        Ok(out) => {
            // Command failed, check stderr for details
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("No matches found") {
                // This is not an error, just no results
                vec![]
            } else {
                eprintln!("[reap] Flatpak search error: {}", stderr.trim());
                vec![]
            }
        }
        Err(e) => {
            eprintln!("[reap] Failed to execute flatpak command: {}", e);
            vec![]
        }
    }
}

/// Check if flatpak command is available
pub fn is_flatpak_available() -> bool {
    Command::new("flatpak")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// Example usage: call flatpak::search from CLI or TUI for Flatpak search

/// Installs a Flatpak package with proper error handling.
///
/// # Arguments
///
/// * `pkg` - A string slice that holds the package name to be installed.
///
/// # Example
///
/// ```no_run
/// use crate::flatpak;
/// flatpak::install("com.example.App");
/// ```
pub fn install(pkg: &str) {
    if !is_flatpak_available() {
        eprintln!("[reap] Error: Flatpak is not installed. Install with: sudo pacman -S flatpak");
        return;
    }

    println!("[reap] Installing Flatpak package: {}", pkg);

    // First try to install from flathub
    let status = Command::new("flatpak")
        .arg("install")
        .arg("--noninteractive")
        .arg("-y")
        .arg("flathub")
        .arg(pkg)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("[reap] Successfully installed: {}", pkg);
        }
        Ok(_s) => {
            // Try without specifying remote
            let retry = Command::new("flatpak")
                .arg("install")
                .arg("--noninteractive")
                .arg("-y")
                .arg(pkg)
                .status();

            match retry {
                Ok(rs) if rs.success() => {
                    println!("[reap] Successfully installed: {}", pkg);
                }
                _ => {
                    eprintln!("[reap] Failed to install Flatpak package: {}", pkg);
                    eprintln!("[reap] Try running: flatpak install {}", pkg);
                }
            }
        }
        Err(e) => {
            eprintln!("[reap] Error executing flatpak command: {}", e);
        }
    }
}

/// Installs a Flatpak package asynchronously.
///
/// # Arguments
///
/// * `pkg` - A string slice that holds the package name to be installed.
///
/// # Errors
///
/// Returns an error if the installation fails.
///
/// # Example
///
/// ```no_run
/// use crate::flatpak;
///
/// #[tokio::main]
/// async fn main() {
///     if let Err(e) = flatpak::install_flatpak("com.example.App").await {
///         eprintln!("Error installing package: {}", e);
///     }
/// }
/// ```
pub async fn install_flatpak(pkg: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !is_flatpak_available() {
        return Err("Flatpak is not installed. Install with: sudo pacman -S flatpak".into());
    }

    println!("[reap][flatpak] Installing {}...", pkg);

    // Try with flathub remote first
    let status = Command::new("flatpak")
        .arg("install")
        .arg("--noninteractive")
        .arg("-y")
        .arg("flathub")
        .arg(pkg)
        .status()?;

    if status.success() {
        println!("[reap][flatpak] Installed {}!", pkg);
        Ok(())
    } else {
        // Retry without specifying remote
        let retry_status = Command::new("flatpak")
            .arg("install")
            .arg("--noninteractive")
            .arg("-y")
            .arg(pkg)
            .status()?;

        if retry_status.success() {
            println!("[reap][flatpak] Installed {}!", pkg);
            Ok(())
        } else {
            Err(format!("[reap][flatpak] install failed for {}. Ensure the package ID is correct and Flathub is configured.", pkg).into())
        }
    }
}

/// Upgrades all installed Flatpak packages.
///
/// # Example
///
/// ```no_run
/// use crate::flatpak;
/// flatpak::upgrade();
/// ```
#[allow(dead_code)]
pub fn upgrade() {
    if !is_flatpak_available() {
        eprintln!("[reap] Error: Flatpak is not installed. Install with: sudo pacman -S flatpak");
        return;
    }

    println!("[reap] flatpak :: Upgrading all flatpak packages...");
    let status = Command::new("flatpak")
        .arg("update")
        .arg("--noninteractive")
        .arg("-y")
        .status();

    match status {
        Ok(s) if s.success() => println!("[reap] flatpak :: All packages upgraded!"),
        Ok(_) => {
            eprintln!("[reap] flatpak :: upgrade failed. Check flatpak update output for details.")
        }
        Err(e) => eprintln!("[reap] flatpak :: upgrade error: {}", e),
    }
}

/// Upgrades all installed Flatpak packages asynchronously.
///
/// # Errors
///
/// Returns an error if the upgrade process fails.
///
/// # Example
///
/// ```no_run
/// use crate::flatpak;
///
/// #[tokio::main]
/// async fn main() {
///     if let Err(e) = flatpak::upgrade_flatpak().await {
///         eprintln!("Error upgrading packages: {}", e);
///     }
/// }
/// ```
pub async fn upgrade_flatpak() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !is_flatpak_available() {
        return Err("Flatpak is not installed. Install with: sudo pacman -S flatpak".into());
    }

    println!("[reap][flatpak] Upgrading all flatpak packages...");
    let status = Command::new("flatpak")
        .arg("update")
        .arg("--noninteractive")
        .arg("-y")
        .status()?;

    if status.success() {
        println!("[reap][flatpak] All packages upgraded!");
        Ok(())
    } else {
        Err("[reap][flatpak] upgrade failed. Check flatpak update output for details.".into())
    }
}
// Example usage: call flatpak::upgrade from CLI or TUI for Flatpak upgrade

/// Prints sandbox information for a Flatpak package.
///
/// # Arguments
///
/// * `pkg` - A string slice that holds the package name.
///
/// # Example
///
/// ```
/// flatpak::print_flatpak_sandbox_info("com.example.App");
/// ```
#[allow(dead_code)]
pub fn print_flatpak_sandbox_info(pkg: &str) {
    let output = Command::new("flatpak").arg("info").arg(pkg).output();
    if let Ok(out) = output {
        let info = String::from_utf8_lossy(&out.stdout);
        if info.contains("sandbox: none") {
            println!(
                "[reap] flatpak :: Warning: Flatpak {} is NOT sandboxed!",
                pkg
            );
        } else {
            println!("[reap] flatpak :: sandbox info for {}:\n{}", pkg, info);
        }
    }
}
// Example usage: call print_flatpak_sandbox_info in TUI/CLI details

// Ensure all async/parallel flows use owned values or Arc<T> in async blocks
// Use Arc::clone for shared state if needed
// Add explicit return types for async blocks using ?
