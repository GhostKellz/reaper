use std::path::Path;
use std::process::Command;
use tokio::process::Command as TokioCommand;

/// Result of GPG signature verification using --status-fd parsing
#[derive(Debug, Clone)]
pub enum VerificationResult {
    /// Signature is valid and key is trusted
    Valid { key_id: String, fingerprint: String },
    /// Signature is cryptographically valid but key is not trusted
    ValidUntrusted { key_id: String, fingerprint: String },
    /// Signature is invalid (bad signature)
    Invalid {
        #[allow(dead_code)]
        reason: String,
    },
    /// Key needed to verify is missing from keyring
    MissingKey { key_id: String },
    /// No signature file exists
    NoSignature,
    /// Error running GPG
    Error(#[allow(dead_code)] String),
}

impl VerificationResult {
    /// Returns true only if signature is cryptographically valid (trusted or not)
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        matches!(
            self,
            VerificationResult::Valid { .. } | VerificationResult::ValidUntrusted { .. }
        )
    }

    /// Returns true if signature is valid AND key is trusted
    #[allow(dead_code)]
    pub fn is_trusted(&self) -> bool {
        matches!(self, VerificationResult::Valid { .. })
    }
}

/// Verify a detached signature using GPG --status-fd for structured output
pub fn verify_signature(sig_path: &Path, file_path: &Path) -> VerificationResult {
    if !sig_path.exists() {
        return VerificationResult::NoSignature;
    }
    if !file_path.exists() {
        return VerificationResult::Error("File to verify does not exist".to_string());
    }

    // Use --status-fd 1 to get machine-readable status on stdout
    let output = Command::new("gpg")
        .args(["--status-fd", "1", "--verify"])
        .arg(sig_path)
        .arg(file_path)
        .output();

    match output {
        Ok(out) => parse_gpg_status(&out.stdout, &out.stderr),
        Err(e) => VerificationResult::Error(format!("Failed to run gpg: {}", e)),
    }
}

/// Parse GPG --status-fd output for verification result
fn parse_gpg_status(stdout: &[u8], stderr: &[u8]) -> VerificationResult {
    let status = String::from_utf8_lossy(stdout);
    let errors = String::from_utf8_lossy(stderr);

    let mut key_id = String::new();
    let mut fingerprint = String::new();
    let mut good_sig = false;
    let mut bad_sig = false;
    let mut no_pubkey = false;
    let mut trust_level = "UNDEFINED";

    // Parse status lines (format: [GNUPG:] KEYWORD args...)
    for line in status.lines() {
        if !line.starts_with("[GNUPG:]") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        match parts[1] {
            "GOODSIG" => {
                good_sig = true;
                if parts.len() > 2 {
                    key_id = parts[2].to_string();
                }
            }
            "BADSIG" => {
                bad_sig = true;
                if parts.len() > 2 {
                    key_id = parts[2].to_string();
                }
            }
            "ERRSIG" => {
                // ERRSIG keyid algo hashalgo class time rc
                // rc=9 means missing key
                if parts.len() > 2 {
                    key_id = parts[2].to_string();
                }
                if parts.len() > 7 && parts[7] == "9" {
                    no_pubkey = true;
                }
            }
            "NO_PUBKEY" => {
                no_pubkey = true;
                if parts.len() > 2 {
                    key_id = parts[2].to_string();
                }
            }
            "VALIDSIG" => {
                // VALIDSIG fingerprint creation_date sig_date ... trust_model trust_level
                if parts.len() > 2 {
                    fingerprint = parts[2].to_string();
                }
            }
            "TRUST_UNDEFINED" | "TRUST_NEVER" => trust_level = "UNDEFINED",
            "TRUST_MARGINAL" => trust_level = "MARGINAL",
            "TRUST_FULLY" => trust_level = "FULLY",
            "TRUST_ULTIMATE" => trust_level = "ULTIMATE",
            _ => {}
        }
    }

    // Determine result
    if bad_sig {
        return VerificationResult::Invalid {
            reason: format!("Bad signature from key {}", key_id),
        };
    }

    if no_pubkey {
        return VerificationResult::MissingKey { key_id };
    }

    if good_sig {
        if trust_level == "FULLY" || trust_level == "ULTIMATE" {
            return VerificationResult::Valid {
                key_id,
                fingerprint,
            };
        } else {
            return VerificationResult::ValidUntrusted {
                key_id,
                fingerprint,
            };
        }
    }

    // No clear result - check stderr for clues
    if errors.contains("No public key") {
        // Try to extract key ID from stderr
        for line in errors.lines() {
            if line.contains("key ID")
                && let Some(idx) = line.rfind(' ')
            {
                return VerificationResult::MissingKey {
                    key_id: line[idx + 1..].to_string(),
                };
            }
        }
        return VerificationResult::MissingKey {
            key_id: "unknown".to_string(),
        };
    }

    VerificationResult::Error(format!("Could not parse GPG output: {}", errors))
}

/// Show GPG key info (sync) - debugging/informational output
///
/// This function is for user-facing key inspection, not for trust decisions.
/// For programmatic key trust checking, use `get_trust_level()`.
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

/// GPG PKGBUILD signature check returning structured result
///
/// Use this for programmatic verification where you need to inspect the result.
#[allow(dead_code)] // API for callers who want structured results
pub fn gpg_check_structured(pkgdir: &Path) -> VerificationResult {
    let sig_path = pkgdir.join("PKGBUILD.sig");
    let pkgb_path = pkgdir.join("PKGBUILD");
    verify_signature(&sig_path, &pkgb_path)
}

/// Enhanced GPG PKGBUILD signature check with auto key fetch and console output
///
/// This wraps `verify_signature()` and adds:
/// - Console output for user feedback
/// - Auto key fetch on missing key
/// - Retry after key import
pub fn gpg_check(pkgdir: &Path) -> Result<(), String> {
    let sig_path = pkgdir.join("PKGBUILD.sig");
    let pkgb_path = pkgdir.join("PKGBUILD");

    // Use structured verification
    let result = verify_signature(&sig_path, &pkgb_path);

    match result {
        VerificationResult::Valid { key_id, .. } => {
            println!("[reap] gpg :: PKGBUILD signature verified (trusted)");
            if let Some(trust) = get_trust_level(&key_id) {
                println!("[reap] gpg :: Key {} trust level: {}", key_id, trust);
            }
            Ok(())
        }
        VerificationResult::ValidUntrusted { key_id, .. } => {
            println!(
                "[reap] gpg :: PKGBUILD signature valid but key not fully trusted"
            );
            if let Some(trust) = get_trust_level(&key_id) {
                println!("[reap] gpg :: Key {} trust level: {}", key_id, trust);
            }
            Ok(())
        }
        VerificationResult::MissingKey { key_id } => {
            println!("[reap] gpg :: Missing public key: {}", key_id);

            // Attempt auto key fetch
            let keyserver = "hkps://keys.openpgp.org";
            println!(
                "[reap] gpg :: Attempting to fetch key {} from {}...",
                key_id, keyserver
            );

            let fetch = Command::new("gpg")
                .args(["--keyserver", keyserver, "--recv-keys", &key_id])
                .status();

            match fetch {
                Ok(s) if s.success() => {
                    println!(
                        "[reap] gpg :: Successfully imported key {} from {}",
                        key_id, keyserver
                    );
                    // Re-run verification using verify_signature
                    let retry = verify_signature(&sig_path, &pkgb_path);
                    match retry {
                        VerificationResult::Valid { .. }
                        | VerificationResult::ValidUntrusted { .. } => {
                            println!(
                                "[reap] gpg :: PKGBUILD signature verified after key import"
                            );
                            Ok(())
                        }
                        _ => Err(format!(
                            "[reap] gpg :: Verification still failed after importing key {}",
                            key_id
                        )),
                    }
                }
                Ok(_) => Err(format!(
                    "[reap] gpg :: Failed to import key {} from {}",
                    key_id, keyserver
                )),
                Err(e) => Err(format!(
                    "[reap] gpg :: Error running gpg --recv-keys: {}",
                    e
                )),
            }
        }
        VerificationResult::Invalid { reason } => {
            Err(format!("[reap] gpg :: Invalid signature: {}", reason))
        }
        VerificationResult::NoSignature => {
            Err("[reap] gpg :: PKGBUILD or signature missing".to_string())
        }
        VerificationResult::Error(e) => {
            Err(format!("[reap] gpg :: Error: {}", e))
        }
    }
}

/// Refresh all GPG keys
pub fn refresh_keys() {
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

/// Show GPG key details - user-facing informational output
///
/// Wrapper for `show_gpg_key_info()`. Use for debugging or user inspection,
/// not for programmatic trust decisions.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_result_is_valid() {
        // Test Valid is recognized as valid
        let valid = VerificationResult::Valid {
            key_id: "ABC123".to_string(),
            fingerprint: "0123456789ABCDEF".to_string(),
        };
        assert!(valid.is_valid());
        assert!(valid.is_trusted());

        // Test ValidUntrusted is valid but not trusted
        let valid_untrusted = VerificationResult::ValidUntrusted {
            key_id: "ABC123".to_string(),
            fingerprint: "0123456789ABCDEF".to_string(),
        };
        assert!(valid_untrusted.is_valid());
        assert!(!valid_untrusted.is_trusted());

        // Test Invalid is not valid
        let invalid = VerificationResult::Invalid {
            reason: "test".to_string(),
        };
        assert!(!invalid.is_valid());
        assert!(!invalid.is_trusted());

        // Test MissingKey is not valid
        let missing = VerificationResult::MissingKey {
            key_id: "ABC123".to_string(),
        };
        assert!(!missing.is_valid());
        assert!(!missing.is_trusted());

        // Test NoSignature is not valid
        let no_sig = VerificationResult::NoSignature;
        assert!(!no_sig.is_valid());
        assert!(!no_sig.is_trusted());

        // Test Error is not valid
        let error = VerificationResult::Error("test".to_string());
        assert!(!error.is_valid());
        assert!(!error.is_trusted());
    }

    #[test]
    fn test_verification_result_variants_distinguishable() {
        // Ensure we can pattern match all variants
        let results = vec![
            VerificationResult::Valid {
                key_id: "A".to_string(),
                fingerprint: "B".to_string(),
            },
            VerificationResult::ValidUntrusted {
                key_id: "A".to_string(),
                fingerprint: "B".to_string(),
            },
            VerificationResult::Invalid {
                reason: "bad".to_string(),
            },
            VerificationResult::MissingKey {
                key_id: "A".to_string(),
            },
            VerificationResult::NoSignature,
            VerificationResult::Error("err".to_string()),
        ];

        let mut valid_count = 0;
        let mut trusted_count = 0;

        for r in &results {
            if r.is_valid() {
                valid_count += 1;
            }
            if r.is_trusted() {
                trusted_count += 1;
            }
        }

        assert_eq!(valid_count, 2); // Valid and ValidUntrusted
        assert_eq!(trusted_count, 1); // Only Valid
    }
}
