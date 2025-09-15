use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    pub package: String,
    pub signature_valid: bool,
    pub publisher_verified: bool,
    pub community_votes: u32,
    pub maintainer_reputation: f32,
    pub last_audit_date: Option<DateTime<Utc>>,
    pub security_flags: Vec<SecurityFlag>,
    pub overall_score: f32, // 0.0 - 10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityFlag {
    UnverifiedSignature,
    UnknownPublisher,
    RecentVulnerability,
    SuspiciousFiles,
    NetworkAccess,
    SystemAccess,
    OutdatedDependencies,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageVerification {
    pub package: String,
    pub source: crate::core::Source,
    pub pgp_signature: Option<PgpVerification>,
    pub publisher_info: Option<crate::tap::Publisher>,
    pub file_integrity: bool,
    pub dependency_scan: DependencyScan,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpVerification {
    pub key_id: String,
    pub key_fingerprint: String,
    pub signature_valid: bool,
    pub key_trusted: bool,
    pub key_expired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyScan {
    pub total_deps: u32,
    pub vulnerable_deps: u32,
    pub outdated_deps: u32,
    pub unknown_deps: u32,
}

pub struct TrustEngine {
    #[allow(dead_code)]
    cache_dir: PathBuf,
    reputation_db: HashMap<String, f32>,
}

impl TrustEngine {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("reap/trust");
        let _ = fs::create_dir_all(&cache_dir);

        Self {
            cache_dir,
            reputation_db: HashMap::new(),
        }
    }

    pub async fn compute_trust_score(&self, pkg: &str, source: &crate::core::Source) -> TrustScore {
        // Check cache first
        if let Some(cached_score) = self.get_cached_trust_score(pkg) {
            return cached_score;
        }

        let mut score = TrustScore {
            package: pkg.to_string(),
            signature_valid: false,
            publisher_verified: false,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: Some(Utc::now()),
            security_flags: Vec::new(),
            overall_score: 5.0,
        };

        // Verify PGP signature
        if let Some(pgp_result) = self.verify_pgp_signature(pkg, source).await {
            score.signature_valid = pgp_result.signature_valid;
            if !pgp_result.signature_valid {
                score.security_flags.push(SecurityFlag::UnverifiedSignature);
            }
        }

        // Check publisher verification
        if let Some(publisher) = self.get_publisher_info(pkg, source).await {
            score.publisher_verified = publisher.verified;
            if !publisher.verified {
                score.security_flags.push(SecurityFlag::UnknownPublisher);
            }
        }

        // Analyze PKGBUILD for security concerns
        if let Some(pkgbuild) = self.get_pkgbuild(pkg, source).await {
            let security_analysis = self.analyze_pkgbuild_security(&pkgbuild);
            score.security_flags.extend(security_analysis);
        }

        // Get community reputation
        score.community_votes = self.get_community_votes(pkg, source).await;
        score.maintainer_reputation = self.get_maintainer_reputation(pkg, source).await;

        // Calculate overall score
        score.overall_score = self.calculate_overall_score(&score);

        // Cache the result
        let _ = self.cache_trust_score(&score);

        score
    }

    async fn verify_pgp_signature(
        &self,
        pkg: &str,
        source: &crate::core::Source,
    ) -> Option<PgpVerification> {
        match source {
            crate::core::Source::Aur => {
                // Check AUR package signature
                let output = Command::new("curl")
                    .arg("-s")
                    .arg(format!(
                        "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD.sig?h={}",
                        pkg
                    ))
                    .output()
                    .ok()?;

                if output.status.success() && !output.stdout.is_empty() {
                    // Signature exists, verify it
                    Some(PgpVerification {
                        key_id: "unknown".to_string(),
                        key_fingerprint: "unknown".to_string(),
                        signature_valid: self.verify_signature_file(&output.stdout).await,
                        key_trusted: false,
                        key_expired: false,
                    })
                } else {
                    None
                }
            }
            crate::core::Source::Custom(tap_name) => {
                // Check tap signature using existing tap verification logic
                if let Some(tap) = self.find_tap_by_name(tap_name) {
                    self.verify_tap_signature(&tap, pkg).await
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    async fn verify_signature_file(&self, _signature_data: &[u8]) -> bool {
        // TODO: Implement actual GPG signature verification
        // For now, return true if signature exists
        true
    }

    async fn get_publisher_info(
        &self,
        _pkg: &str,
        source: &crate::core::Source,
    ) -> Option<crate::tap::Publisher> {
        match source {
            crate::core::Source::Custom(tap_name) => {
                if let Some(tap) = self.find_tap_by_name(tap_name) {
                    crate::tap::get_publisher_info(&tap)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn find_tap_by_name(&self, name: &str) -> Option<crate::tap::Tap> {
        let taps = crate::tap::discover_taps();
        taps.into_iter().find(|t| t.name == name)
    }

    async fn verify_tap_signature(
        &self,
        tap: &crate::tap::Tap,
        pkg: &str,
    ) -> Option<PgpVerification> {
        let tap_path = crate::tap::ensure_tap_cloned(tap);
        let sig_path = tap_path.join(pkg).join("PKGBUILD.sig");

        if sig_path.exists() {
            let verification = crate::gpg::verify_pkgbuild(&tap_path.join(pkg));
            Some(PgpVerification {
                key_id: "tap_key".to_string(),
                key_fingerprint: "unknown".to_string(),
                signature_valid: verification,
                key_trusted: true,
                key_expired: false,
            })
        } else {
            None
        }
    }

    async fn get_pkgbuild(&self, pkg: &str, source: &crate::core::Source) -> Option<String> {
        match source {
            crate::core::Source::Aur => Some(crate::aur::get_pkgbuild_preview(pkg)),
            crate::core::Source::Custom(tap_name) => {
                if let Some(tap) = self.find_tap_by_name(tap_name) {
                    let tap_path = crate::tap::ensure_tap_cloned(&tap);
                    let pkgbuild_path = tap_path.join(pkg).join("PKGBUILD");
                    fs::read_to_string(pkgbuild_path).ok()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn analyze_pkgbuild_security(&self, pkgbuild: &str) -> Vec<SecurityFlag> {
        let mut flags = Vec::new();

        // Check for suspicious patterns
        let suspicious_patterns = [
            "curl",
            "wget",
            "git clone",
            "sudo",
            "chmod +x",
            "rm -rf",
            "dd if=",
            "mktemp",
            "eval",
            "exec",
        ];

        for pattern in &suspicious_patterns {
            if pkgbuild.contains(pattern) {
                match *pattern {
                    "curl" | "wget" | "git clone" => flags.push(SecurityFlag::NetworkAccess),
                    "sudo" | "chmod +x" => flags.push(SecurityFlag::SystemAccess),
                    "rm -rf" | "dd if=" => flags.push(SecurityFlag::SuspiciousFiles),
                    _ => flags.push(SecurityFlag::SuspiciousFiles),
                }
            }
        }

        flags
    }

    async fn get_community_votes(&self, _pkg: &str, _source: &crate::core::Source) -> u32 {
        match _source {
            crate::core::Source::Aur => {
                // Query AUR API for vote count
                let url = format!(
                    "https://aur.archlinux.org/rpc/?v=5&type=info&arg[]={}",
                    _pkg
                );
                if let Ok(resp) = reqwest::get(&url).await {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        return json["results"][0]["NumVotes"].as_u64().unwrap_or(0) as u32;
                    }
                }
                0
            }
            _ => 0,
        }
    }

    async fn get_maintainer_reputation(&self, _pkg: &str, _source: &crate::core::Source) -> f32 {
        // Use cached reputation or default
        self.reputation_db.get(_pkg).copied().unwrap_or(5.0)
    }

    fn calculate_overall_score(&self, trust: &TrustScore) -> f32 {
        let mut score = 5.0;

        // Positive factors
        if trust.signature_valid {
            score += 2.0;
        }
        if trust.publisher_verified {
            score += 1.5;
        }
        score += (trust.community_votes as f32 * 0.01).min(1.0); // Max 1 point from votes
        score += (trust.maintainer_reputation - 5.0) * 0.5; // Maintainer rep adjustment

        // Negative factors
        score -= trust.security_flags.len() as f32 * 0.5;

        // Clamp between 0.0 and 10.0
        score.clamp(0.0, 10.0)
    }

    #[allow(dead_code)]
    pub fn get_cached_trust_score(&self, pkg: &str) -> Option<TrustScore> {
        let cache_file = self.cache_dir.join(format!("{}.json", pkg));
        if cache_file.exists() {
            if let Ok(content) = fs::read_to_string(cache_file) {
                return serde_json::from_str(&content).ok();
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn cache_trust_score(&self, trust_score: &TrustScore) -> Result<()> {
        let cache_file = self.cache_dir.join(format!("{}.json", trust_score.package));
        let content = serde_json::to_string_pretty(trust_score)?;
        fs::write(cache_file, content)?;
        Ok(())
    }

    pub fn display_trust_badge(&self, score: f32) -> String {
        use owo_colors::OwoColorize;

        match score {
            s if s >= 8.0 => "🛡️ TRUSTED".green().to_string(),
            s if s >= 6.0 => "✅ VERIFIED".cyan().to_string(),
            s if s >= 4.0 => "⚠️ CAUTION".yellow().to_string(),
            s if s >= 2.0 => "🚨 RISKY".red().to_string(),
            _ => "❌ UNSAFE".on_red().to_string(),
        }
    }
}

impl Default for TrustEngine {
    fn default() -> Self {
        Self::new()
    }
}
