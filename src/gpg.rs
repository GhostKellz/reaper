use std::path::Path;
use std::process::Command;
use tokio::process::Command as TokioCommand;

/// Show GPG key info (sync)
pub fn show_gpg_key_info(keyid: &str) {
    let output = Command::new("gpg")
        .args(["--list-keys", keyid, "--with-colons"])
        .output();
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("pub:") {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() > 6 {
                    println!(
                        "[reap] gpg :: Key trust: {} Expiry: {}",
                        fields[1], fields[6]
                    );
                }
            }
        }
    }
}

/// Helper to get GPG trust level for a keyid
pub fn get_trust_level(keyid: &str) -> Option<String> {
    let output = Command::new("gpg")
        .args(["--list-keys", "--with-colons", keyid])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("pub:") || line.starts_with("uid:") {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() > 1 {
                let trust = match fields[1] {
                    "f" => "Full",
                    "m" => "Marginal",
                    "n" => "None",
                    "u" => "Ultimate",
                    other => other,
                };
                return Some(trust.to_string());
            }
        }
    }
    None
}

// Use gpg_check(pkgdir) in the PKGBUILD verification step.

/// Verify PKGBUILD signature in a directory
pub fn verify_pkgbuild(pkgdir: &Path) -> bool {
    let sig_path = pkgdir.join("PKGBUILD.sig");
    let pkgb_path = pkgdir.join("PKGBUILD");
    if !sig_path.exists() || !pkgb_path.exists() {
        eprintln!("[reap] gpg :: PKGBUILD or signature missing");
        return false;
    }
    let status = Command::new("gpg")
        .arg("--verify")
        .arg(sig_path)
        .arg(pkgb_path)
        .status();
    if let Ok(s) = status {
        if s.success() {
            println!("[reap] gpg :: PKGBUILD signature verified");
            return true;
        }
    }
    eprintln!("[reap] gpg :: PKGBUILD signature verification failed");
    false
}

/// Enhanced GPG PKGBUILD signature check with auto key fetch
pub fn gpg_check(pkgdir: &Path) -> Result<(), String> {
    let sig_path = pkgdir.join("PKGBUILD.sig");
    let pkgb_path = pkgdir.join("PKGBUILD");
    if !sig_path.exists() || !pkgb_path.exists() {
        return Err("[reap] gpg :: PKGBUILD or signature missing".to_string());
    }
    let output = Command::new("gpg")
        .arg("--verify")
        .arg(&sig_path)
        .arg(&pkgb_path)
        .output();
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            println!("[reap] gpg :: PKGBUILD signature verified");
            // Show trust level if possible
            if let Some(keyid) = stdout.lines().find_map(|line| {
                if line.starts_with("pub:") {
                    let fields: Vec<&str> = line.split(':').collect();
                    if fields.len() > 4 {
                        return Some(fields[4].to_string());
                    }
                }
                None
            }) {
                if let Some(trust) = get_trust_level(&keyid) {
                    println!("[reap] gpg :: Key {} trust level: {}", keyid, trust);
                }
            }
            Ok(())
        } else {
            // Try to extract missing keyid from error output
            let mut keyid = None;
            for line in stderr.lines().chain(stdout.lines()) {
                if line.contains("NO_PUBKEY") {
                    // Example: gpg: Signature made ... using RSA key ABCDEF1234567890
                    if let Some(idx) = line.find("NO_PUBKEY ") {
                        keyid = line[idx + 10..]
                            .split_whitespace()
                            .next()
                            .map(|s| s.to_string());
                    }
                } else if line.contains("key ID") {
                    // Example: gpg: Can't check signature: No public key
                    //         gpg: Signature made ... using DSA key ID 12345678
                    if let Some(idx) = line.find("key ID ") {
                        keyid = line[idx + 7..]
                            .split_whitespace()
                            .next()
                            .map(|s| s.to_string());
                    }
                }
            }
            if let Some(keyid) = keyid {
                println!("[reap] gpg :: Missing public key: {}", keyid);
                let keyserver = "hkps://keys.openpgp.org";
                println!(
                    "[reap] gpg :: Attempting to fetch key {} from {}...",
                    keyid, keyserver
                );
                let fetch = Command::new("gpg")
                    .args(["--keyserver", keyserver, "--recv-keys", &keyid])
                    .status();
                match fetch {
                    Ok(s) if s.success() => {
                        println!(
                            "[reap] gpg :: Successfully imported key {} from {}",
                            keyid, keyserver
                        );
                        // Re-run verification
                        let retry = Command::new("gpg")
                            .arg("--verify")
                            .arg(&sig_path)
                            .arg(&pkgb_path)
                            .status();
                        if let Ok(s) = retry {
                            if s.success() {
                                println!(
                                    "[reap] gpg :: PKGBUILD signature verified after key import"
                                );
                                Ok(())
                            } else {
                                Err(format!(
                                    "[reap] gpg :: Verification still failed after importing key {}",
                                    keyid
                                ))
                            }
                        } else {
                            Err(
                                "[reap] gpg :: Error re-running verification after key import"
                                    .to_string(),
                            )
                        }
                    }
                    Ok(_) => Err(format!(
                        "[reap] gpg :: Failed to import key {} from {}",
                        keyid, keyserver
                    )),
                    Err(e) => Err(format!(
                        "[reap] gpg :: Error running gpg --recv-keys: {}",
                        e
                    )),
                }
            } else {
                // Could not extract keyid
                eprintln!("[reap] gpg :: Could not extract missing keyid from error output:");
                for line in stderr.lines() {
                    eprintln!("[gpg] {line}");
                }
                Err(
                    "[reap] gpg :: PKGBUILD signature verification failed and no keyid found"
                        .to_string(),
                )
            }
        }
    } else {
        Err("[reap] gpg :: Error running gpg --verify".to_string())
    }
}

/// Refresh all GPG keys
pub fn refresh_keys() {
    // TODO: Wire this into CLI flow in core::handle_cli()
    let status = Command::new("gpg").arg("--refresh-keys").status();
    if let Ok(s) = status {
        if s.success() {
            println!("[reap] gpg :: Refreshed all keys");
        } else {
            eprintln!("[reap] gpg :: Failed to refresh keys");
        }
    }
}

/// Async GPG key import from multiple keyservers
pub async fn import_gpg_key_async(keyid: &str) -> Result<(), String> {
    let keyservers = [
        "hkps://keyserver.ubuntu.com",
        "hkps://keys.openpgp.org",
        "hkps://pgp.mit.edu",
    ];
    let mut last_err = None;
    for server in &keyservers {
        match TokioCommand::new("gpg")
            .args(["--keyserver", server, "--recv-keys", keyid])
            .status()
            .await
        {
            Ok(s) if s.success() => {
                println!("[reap] gpg :: Imported key {} from {}", keyid, server);
                return Ok(());
            }
            Ok(_) => {
                last_err = Some(format!("Failed to import key from {}", server));
            }
            Err(e) => {
                last_err = Some(format!("TokioCommand error for {}: {}", server, e));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "All keyserver attempts failed".to_string()))
}

/// Async GPG key presence check
pub async fn check_key(keyid: &str) {
    let output = TokioCommand::new("gpg")
        .args(["--list-keys", keyid])
        .output()
        .await;
    if let Ok(out) = output {
        if out.status.success() {
            println!("[reap] gpg :: GPG key {} is present.", keyid);
        } else {
            println!("[reap] gpg :: GPG key {} is NOT present.", keyid);
        }
    } else {
        println!("[reap] gpg :: Failed to check GPG key {}.", keyid);
    }
}

/// Show GPG key details
pub fn show_key(keyid: &str) {
    show_gpg_key_info(keyid);
}

/// Check if GPG key exists in keyring
pub fn key_exists(keyid: &str) -> bool {
    let output = Command::new("gpg").args(["--list-keys", keyid]).output();
    if let Ok(out) = output {
        out.status.success()
    } else {
        false
    }
}
