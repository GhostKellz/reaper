// Pacman repo logic
use std::process::Command;

/// Install a package from the official repositories using pacman.
/// Uses --noconfirm for scripted/batch installs.
pub fn install(package: &str) {
    install_with_options(package, true);
}

/// Install a package with explicit confirmation control.
/// When `noconfirm` is false, pacman will prompt the user for confirmation.
pub fn install_with_options(package: &str, noconfirm: bool) {
    println!("[pacman] Installing package: {}", package);

    let mut cmd = Command::new("sudo");
    cmd.arg("pacman").arg("-S");

    if noconfirm {
        cmd.arg("--noconfirm");
    }

    cmd.arg(package);

    let status = cmd.status();
    if let Ok(s) = status {
        if s.success() {
            println!("[pacman] {} installed successfully!", package);
        } else {
            eprintln!("[pacman] pacman failed for {}", package);
        }
    } else {
        eprintln!("[pacman] failed to run pacman for {}", package);
    }
}

/// Install a package interactively (prompts for confirmation).
/// Use this for user-initiated single package installs.
#[allow(dead_code)]
pub fn install_interactive(package: &str) {
    install_with_options(package, false);
}

/// Remove a package with explicit confirmation control.
#[allow(dead_code)]
pub fn remove_with_options(packages: &[String], noconfirm: bool) {
    println!("[pacman] Removing packages: {:?}", packages);

    let mut cmd = Command::new("sudo");
    cmd.arg("pacman").arg("-Rs");

    if noconfirm {
        cmd.arg("--noconfirm");
    }

    for pkg in packages {
        cmd.arg(pkg);
    }

    let status = cmd.status();
    if let Ok(s) = status {
        if s.success() {
            println!("[pacman] Packages removed successfully");
        } else {
            eprintln!("[pacman] Failed to remove packages");
        }
    } else {
        eprintln!("[pacman] Failed to run pacman -Rs");
    }
}

#[allow(dead_code)]
pub fn is_installed(pkg: &str) -> bool {
    Command::new("pacman")
        .arg("-Q")
        .arg(pkg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn get_version(pkg: &str) -> Option<String> {
    let output = Command::new("pacman").arg("-Qi").arg(pkg).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("Version") {
            // Format: "Version         : 1.2.3-1"
            return line.split(':').nth(1).map(|s| s.trim().to_string());
        }
    }
    None
}

pub fn list_installed_aur() -> Vec<String> {
    let output = std::process::Command::new("pacman").arg("-Qm").output();
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    }
}

/// Get packages that depend on the given package (reverse dependencies)
pub fn get_reverse_depends(pkg: &str) -> Vec<String> {
    let output = Command::new("pacman")
        .args(["-Qi", pkg])
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("Required By") {
                let deps = line.split(':').nth(1).unwrap_or("").trim();
                if deps == "None" {
                    return Vec::new();
                }
                return deps
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Get package dependencies
pub fn get_depends(pkg: &str) -> Vec<String> {
    let output = Command::new("pacman")
        .args(["-Qi", pkg])
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("Depends On") {
                let deps = line.split(':').nth(1).unwrap_or("").trim();
                if deps == "None" {
                    return Vec::new();
                }
                return deps
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Get what a package provides
pub fn get_provides(pkg: &str) -> Vec<String> {
    let output = Command::new("pacman")
        .args(["-Qi", pkg])
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("Provides") {
                let provides = line.split(':').nth(1).unwrap_or("").trim();
                if provides == "None" {
                    return Vec::new();
                }
                return provides
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Get package conflicts
pub fn get_conflicts(pkg: &str) -> Vec<String> {
    let output = Command::new("pacman")
        .args(["-Qi", pkg])
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("Conflicts With") {
                let conflicts = line.split(':').nth(1).unwrap_or("").trim();
                if conflicts == "None" {
                    return Vec::new();
                }
                return conflicts
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Parse a dependency string to extract name and version constraint
/// e.g., "glibc>=2.17" -> ("glibc", Some(">=2.17"))
pub fn parse_dependency(dep: &str) -> (String, Option<String>) {
    // Common version operators: >=, <=, =, >, <
    for op in &[">=", "<=", "=", ">", "<"] {
        if let Some(pos) = dep.find(op) {
            let name = dep[..pos].to_string();
            let constraint = dep[pos..].to_string();
            return (name, Some(constraint));
        }
    }
    (dep.to_string(), None)
}

/// Check if a version satisfies a constraint
/// Constraint format: ">=2.17", "<=3.0", "=1.0", etc.
pub fn version_satisfies(version: &str, constraint: &str) -> bool {
    use std::cmp::Ordering;

    let (op, required) = if let Some(v) = constraint.strip_prefix(">=") {
        (">=", v)
    } else if let Some(v) = constraint.strip_prefix("<=") {
        ("<=", v)
    } else if let Some(v) = constraint.strip_prefix("=") {
        ("=", v)
    } else if let Some(v) = constraint.strip_prefix(">") {
        (">", v)
    } else if let Some(v) = constraint.strip_prefix("<") {
        ("<", v)
    } else {
        return true; // No operator means any version is fine
    };

    let cmp = crate::version::vercmp(version, required);
    match op {
        ">=" => matches!(cmp, Ordering::Greater | Ordering::Equal),
        "<=" => matches!(cmp, Ordering::Less | Ordering::Equal),
        "=" => cmp == Ordering::Equal,
        ">" => cmp == Ordering::Greater,
        "<" => cmp == Ordering::Less,
        _ => true,
    }
}

/// Get all installed packages with their versions
#[allow(dead_code)]
pub fn list_installed_with_versions() -> Vec<(String, String)> {
    let output = Command::new("pacman")
        .args(["-Q"])
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        return stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let name = parts.next()?;
                let version = parts.next()?;
                Some((name.to_string(), version.to_string()))
            })
            .collect();
    }
    Vec::new()
}

/// Get names of all installed packages as a set for efficient comparison
pub fn list_installed_names() -> std::collections::HashSet<String> {
    let output = Command::new("pacman").args(["-Qq"]).output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        return stdout.lines().map(|s| s.to_string()).collect();
    }
    std::collections::HashSet::new()
}
