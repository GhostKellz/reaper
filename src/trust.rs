//! Trust and Security Model for Reaper
//!
//! # Trust Model: Advisory Only
//!
//! Reaper's trust system is **advisory only** - it provides information and warnings
//! but never blocks installations. This is a deliberate design decision:
//!
//! - Trust scores are heuristic indicators, not cryptographic guarantees
//! - Users make final decisions about what to install
//! - Warnings are displayed but installations proceed
//!
//! # Security Verification by Source
//!
//! ## Tap Packages (Custom Repositories)
//! - GPG signature verification is **required** by default
//! - Missing or invalid signatures abort installation
//! - Use `--insecure` flag to override (with explicit warning)
//! - Verified publishers show trust badges
//!
//! ## AUR Packages
//! - No signature verification (AUR does not provide package signatures)
//! - Trust scores based on heuristics: maintainer reputation, age, votes
//! - PKGBUILD content is analyzed for suspicious patterns
//! - Advisory warnings displayed, but installation proceeds
//!
//! ## Pacman Packages (Official Repos)
//! - Uses pacman's built-in signature verification
//! - Reaper does not add additional verification layer
//!
//! # Secure Defaults
//!
//! - `insecure: false` - require verification where available
//! - Missing signatures on taps: abort (override with --insecure)
//! - Invalid signatures: abort (override with --insecure)
//! - Unknown publishers: warn but proceed (advisory)
//! - Network failures during verification: warn and proceed
//!
//! # Trust Score Components
//!
//! - `signature_valid`: GPG signature cryptographically verified
//! - `publisher_verified`: Key is in trusted keyring with full/ultimate trust
//! - `community_votes`: AUR vote count (popularity indicator)
//! - `maintainer_reputation`: Historical maintainer track record
//! - `security_flags`: Detected risk indicators from PKGBUILD analysis

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    pub package: String,
    pub signature_valid: bool,
    pub publisher_status: crate::tap::PublisherStatus,
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

#[allow(dead_code)] // Infrastructure for future verification features
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

#[allow(dead_code)] // Infrastructure for future dependency scanning
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
        let cache_dir = crate::paths::trust_dir();
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
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
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

        // Check publisher verification status
        score.publisher_status = self.get_publisher_status(source);
        match score.publisher_status {
            crate::tap::PublisherStatus::Unknown | crate::tap::PublisherStatus::SelfDeclared => {
                score.security_flags.push(SecurityFlag::UnknownPublisher);
            }
            _ => {}
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
            // AUR packages: signature verification not available
            // AUR does not have a standard signature infrastructure we can verify against.
            // Trust for AUR relies on PKGBUILD analysis and community signals instead.
            crate::core::Source::Aur => None,

            // Tap packages: use real GPG verification
            crate::core::Source::Custom(tap_name) => {
                if let Some(tap) = self.find_tap_by_name(tap_name) {
                    self.verify_tap_signature(&tap, pkg).await
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn get_publisher_status(&self, source: &crate::core::Source) -> crate::tap::PublisherStatus {
        match source {
            crate::core::Source::Custom(tap_name) => {
                if let Some(tap) = self.find_tap_by_name(tap_name) {
                    crate::tap::get_publisher_status(&tap)
                } else {
                    crate::tap::PublisherStatus::Unknown
                }
            }
            // Non-tap sources don't have publisher verification
            _ => crate::tap::PublisherStatus::NotApplicable,
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
        let pkg_dir = tap_path.join(pkg);
        let sig_path = pkg_dir.join("PKGBUILD.sig");
        let pkgbuild_path = pkg_dir.join("PKGBUILD");

        if !sig_path.exists() || !pkgbuild_path.exists() {
            return None;
        }

        // Use new structured GPG verification
        let result = crate::gpg::verify_signature(&sig_path, &pkgbuild_path);

        match result {
            crate::gpg::VerificationResult::Valid {
                key_id,
                fingerprint,
            } => Some(PgpVerification {
                key_id,
                key_fingerprint: fingerprint,
                signature_valid: true,
                key_trusted: true,
                key_expired: false,
            }),
            crate::gpg::VerificationResult::ValidUntrusted {
                key_id,
                fingerprint,
            } => {
                Some(PgpVerification {
                    key_id,
                    key_fingerprint: fingerprint,
                    signature_valid: true,
                    key_trusted: false, // Valid signature but key not fully trusted
                    key_expired: false,
                })
            }
            crate::gpg::VerificationResult::Invalid { reason: _ } => Some(PgpVerification {
                key_id: "unknown".to_string(),
                key_fingerprint: "unknown".to_string(),
                signature_valid: false,
                key_trusted: false,
                key_expired: false,
            }),
            crate::gpg::VerificationResult::MissingKey { key_id } => Some(PgpVerification {
                key_id,
                key_fingerprint: "unknown".to_string(),
                signature_valid: false,
                key_trusted: false,
                key_expired: false,
            }),
            crate::gpg::VerificationResult::NoSignature
            | crate::gpg::VerificationResult::Error(_) => None,
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
                if let Ok(resp) = reqwest::get(&url).await
                    && let Ok(json) = resp.json::<serde_json::Value>().await
                {
                    return json["results"][0]["NumVotes"].as_u64().unwrap_or(0) as u32;
                }
                0
            }
            _ => 0,
        }
    }

    async fn get_maintainer_reputation(&self, _pkg: &str, _source: &crate::core::Source) -> f32 {
        // NOTE: Maintainer reputation lookup is not yet implemented.
        // Returns cached value if available, otherwise neutral score (5.0).
        // A real implementation would query a reputation database.
        self.reputation_db.get(_pkg).copied().unwrap_or(5.0)
    }

    fn calculate_overall_score(&self, trust: &TrustScore) -> f32 {
        let mut score = 5.0;

        // Positive factors
        if trust.signature_valid {
            score += 2.0;
        }

        // Publisher status scoring
        match trust.publisher_status {
            crate::tap::PublisherStatus::KeyMatches => score += 1.5, // Cryptographic verification
            crate::tap::PublisherStatus::SelfDeclared => score += 0.5, // Some metadata, but unverified
            crate::tap::PublisherStatus::NotApplicable => {} // No bonus or penalty
            crate::tap::PublisherStatus::Unknown => {} // Already penalized via security flag
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
        if cache_file.exists()
            && let Ok(content) = fs::read_to_string(&cache_file)
            && let Ok(cached) = serde_json::from_str::<TrustScore>(&content)
        {
            // Check if cache is fresh
            if self.is_cache_fresh(&cached) {
                return Some(cached);
            }
            // Cache is stale, will be recomputed
            // Optionally delete stale cache file
            let _ = fs::remove_file(&cache_file);
        }
        None
    }

    /// Check if a cached trust score is still fresh based on TTL
    pub fn is_cache_fresh(&self, cached: &TrustScore) -> bool {
        let config = crate::config::Config::load();
        let ttl_hours = config.security.trust_cache_ttl_hours;

        if let Some(cached_at) = cached.last_audit_date {
            let age = Utc::now() - cached_at;
            age < chrono::Duration::hours(ttl_hours)
        } else {
            // No timestamp - treat as stale
            false
        }
    }

    #[allow(dead_code)]
    pub fn cache_trust_score(&self, trust_score: &TrustScore) -> Result<()> {
        let cache_file = self.cache_dir.join(format!("{}.json", trust_score.package));
        let content = serde_json::to_string_pretty(trust_score)?;
        fs::write(cache_file, content)?;
        Ok(())
    }

    /// Invalidate cached trust score for a specific package
    pub fn invalidate_cache(&self, pkg: &str) {
        let cache_file = self.cache_dir.join(format!("{}.json", pkg));
        if cache_file.exists() {
            let _ = fs::remove_file(&cache_file);
        }
    }

    /// Invalidate all cached trust scores for packages in a tap
    ///
    /// Call this after tap sync to ensure trust data is recomputed
    pub fn invalidate_tap_cache(&self, tap_name: &str) {
        // Find the tap and list packages
        if let Some(tap) = crate::tap::discover_taps()
            .into_iter()
            .find(|t| t.name == tap_name)
        {
            let tap_path = crate::tap::ensure_tap_cloned(&tap);
            // List all package directories (each dir with PKGBUILD is a package)
            if let Ok(entries) = fs::read_dir(&tap_path) {
                for entry in entries.flatten() {
                    let pkg_dir = entry.path();
                    if pkg_dir.is_dir()
                        && pkg_dir.join("PKGBUILD").exists()
                        && let Some(pkg_name) = pkg_dir.file_name()
                    {
                        self.invalidate_cache(&pkg_name.to_string_lossy());
                    }
                }
            }
        }
    }

    pub fn display_trust_badge(&self, score: f32) -> String {
        use owo_colors::OwoColorize;
        // NOTE: These labels reflect heuristic trust scores, not cryptographic verification.
        // "HIGH TRUST" indicates positive community signals, not verified signatures.
        match score {
            s if s >= 8.0 => "🛡️ HIGH TRUST".green().to_string(),
            s if s >= 6.0 => "✓ MODERATE".cyan().to_string(),
            s if s >= 4.0 => "⚠️ LOW TRUST".yellow().to_string(),
            s if s >= 2.0 => "🚨 RISKY".red().to_string(),
            _ => "❌ UNTRUSTED".on_red().to_string(),
        }
    }

    /// Display signature verification status as a badge
    ///
    /// This shows the cryptographic verification status separately from heuristic scores.
    pub fn display_signature_badge(&self, score: &TrustScore) -> String {
        use owo_colors::OwoColorize;

        if score.signature_valid {
            // Check if the publisher key matches (cryptographic trust)
            match score.publisher_status {
                crate::tap::PublisherStatus::KeyMatches => {
                    "🔒 VERIFIED".green().to_string()
                }
                _ => "✓ SIGNED".cyan().to_string(),
            }
        } else {
            // Check if there's an UnverifiedSignature flag (explicit verification failure)
            let has_invalid = score
                .security_flags
                .iter()
                .any(|f| matches!(f, SecurityFlag::UnverifiedSignature));
            if has_invalid {
                "❌ INVALID".red().to_string()
            } else {
                "⚠️ UNSIGNED".yellow().to_string()
            }
        }
    }

    /// Display publisher status as human-readable text
    pub fn display_publisher_status(&self, status: &crate::tap::PublisherStatus) -> String {
        match status {
            crate::tap::PublisherStatus::KeyMatches => "Verified (key matches signing key)".to_string(),
            crate::tap::PublisherStatus::SelfDeclared => "Self-declared (not verified)".to_string(),
            crate::tap::PublisherStatus::NotApplicable => "N/A (non-tap source)".to_string(),
            crate::tap::PublisherStatus::Unknown => "Unknown".to_string(),
        }
    }
}

impl Default for TrustEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_badge_high_score() {
        let engine = TrustEngine::new();
        let badge = engine.display_trust_badge(9.0);
        assert!(
            badge.contains("HIGH TRUST"),
            "Score 9.0 should show HIGH TRUST"
        );
    }

    #[test]
    fn test_trust_badge_moderate_score() {
        let engine = TrustEngine::new();
        let badge = engine.display_trust_badge(6.5);
        assert!(badge.contains("MODERATE"), "Score 6.5 should show MODERATE");
    }

    #[test]
    fn test_trust_badge_low_score() {
        let engine = TrustEngine::new();
        let badge = engine.display_trust_badge(4.5);
        assert!(
            badge.contains("LOW TRUST"),
            "Score 4.5 should show LOW TRUST"
        );
    }

    #[test]
    fn test_trust_badge_risky_score() {
        let engine = TrustEngine::new();
        let badge = engine.display_trust_badge(2.5);
        assert!(badge.contains("RISKY"), "Score 2.5 should show RISKY");
    }

    #[test]
    fn test_trust_badge_untrusted_score() {
        let engine = TrustEngine::new();
        let badge = engine.display_trust_badge(1.0);
        assert!(
            badge.contains("UNTRUSTED"),
            "Score 1.0 should show UNTRUSTED"
        );
    }

    #[test]
    fn test_trust_score_calculation_unverified() {
        let engine = TrustEngine::new();
        // An unverified package should not get full trust score
        let score = TrustScore {
            package: "test-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::Unknown,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![SecurityFlag::UnverifiedSignature],
            overall_score: 0.0, // Will be calculated
        };
        let calculated = engine.calculate_overall_score(&score);
        // Base 5.0, no signature bonus (+0), no publisher bonus (+0),
        // security flag penalty (-0.5) = 4.5
        assert!(
            calculated < 5.0,
            "Unverified package should have below-average score"
        );
    }

    #[test]
    fn test_insecure_flag_default() {
        let opts = crate::core::InstallOptions::default();
        assert!(!opts.insecure, "insecure should be false by default");
    }

    #[test]
    fn test_insecure_flag_set() {
        let opts = crate::core::InstallOptions {
            insecure: true,
            ..Default::default()
        };
        assert!(opts.insecure, "insecure should be settable to true");
    }

    #[test]
    fn test_trust_is_advisory_only() {
        // Per user decision: trust model is "Advisory Only" - show warnings but never block
        // Even with the lowest possible trust score, install should be allowed
        let engine = TrustEngine::new();

        let score = TrustScore {
            package: "suspicious-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::Unknown,
            community_votes: 0,
            maintainer_reputation: 0.0,
            last_audit_date: None,
            security_flags: vec![
                SecurityFlag::UnverifiedSignature,
                SecurityFlag::NetworkAccess,
                SecurityFlag::SystemAccess,
                SecurityFlag::SuspiciousFiles,
            ],
            overall_score: 0.0,
        };

        let calculated = engine.calculate_overall_score(&score);

        // Even with score 0, trust is advisory only - we display warnings but don't block
        // This test documents the design decision
        let badge = engine.display_trust_badge(calculated);
        assert!(
            badge.contains("UNTRUSTED") || badge.contains("RISKY"),
            "Low trust should show warning badge"
        );

        // The key assertion: there is no "block" function in TrustEngine
        // Trust warnings are displayed, but installation proceeds
        // This is enforced by the absence of any blocking mechanism in the API
    }

    #[test]
    fn test_security_flags_are_additive() {
        let engine = TrustEngine::new();

        // Use moderate values so the score doesn't hit the 10.0 cap
        let score_no_flags = TrustScore {
            package: "clean-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 10,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };

        let score_with_flags = TrustScore {
            package: "flagged-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 10,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![SecurityFlag::NetworkAccess, SecurityFlag::SystemAccess],
            overall_score: 0.0,
        };

        let clean_score = engine.calculate_overall_score(&score_no_flags);
        let flagged_score = engine.calculate_overall_score(&score_with_flags);

        // Each security flag should reduce score by 0.5
        // 2 flags = 1.0 reduction
        assert!(
            flagged_score < clean_score,
            "Security flags should reduce trust score: clean={}, flagged={}",
            clean_score,
            flagged_score
        );
        assert!(
            (clean_score - flagged_score - 1.0).abs() < 0.01,
            "Two security flags should reduce score by 1.0"
        );
    }

    #[test]
    fn test_publisher_status_scoring() {
        let engine = TrustEngine::new();

        // KeyMatches gets highest publisher bonus (+1.5)
        let score_key_matches = TrustScore {
            package: "verified-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::KeyMatches,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };

        // SelfDeclared gets partial bonus (+0.5)
        let score_self_declared = TrustScore {
            package: "declared-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::SelfDeclared,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };

        // NotApplicable gets no bonus
        let score_na = TrustScore {
            package: "aur-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };

        let key_matches_score = engine.calculate_overall_score(&score_key_matches);
        let self_declared_score = engine.calculate_overall_score(&score_self_declared);
        let na_score = engine.calculate_overall_score(&score_na);

        // KeyMatches > SelfDeclared > NotApplicable
        assert!(
            key_matches_score > self_declared_score,
            "KeyMatches should score higher than SelfDeclared"
        );
        assert!(
            self_declared_score > na_score,
            "SelfDeclared should score higher than NotApplicable"
        );

        // Verify the actual bonus values
        assert!(
            (key_matches_score - na_score - 1.5).abs() < 0.01,
            "KeyMatches should add 1.5 points"
        );
        assert!(
            (self_declared_score - na_score - 0.5).abs() < 0.01,
            "SelfDeclared should add 0.5 points"
        );
    }

    #[test]
    fn test_signature_badge_verified() {
        let engine = TrustEngine::new();

        // Verified: valid signature + key matches publisher
        let score = TrustScore {
            package: "verified-pkg".to_string(),
            signature_valid: true,
            publisher_status: crate::tap::PublisherStatus::KeyMatches,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };
        let badge = engine.display_signature_badge(&score);
        assert!(badge.contains("VERIFIED"), "Should show VERIFIED for valid+KeyMatches");
    }

    #[test]
    fn test_signature_badge_signed() {
        let engine = TrustEngine::new();

        // Signed: valid signature but publisher not verified
        let score = TrustScore {
            package: "signed-pkg".to_string(),
            signature_valid: true,
            publisher_status: crate::tap::PublisherStatus::SelfDeclared,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };
        let badge = engine.display_signature_badge(&score);
        assert!(badge.contains("SIGNED"), "Should show SIGNED for valid signature without KeyMatches");
    }

    #[test]
    fn test_signature_badge_unsigned() {
        let engine = TrustEngine::new();

        // Unsigned: no signature, no verification failure flag
        let score = TrustScore {
            package: "unsigned-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![],
            overall_score: 0.0,
        };
        let badge = engine.display_signature_badge(&score);
        assert!(badge.contains("UNSIGNED"), "Should show UNSIGNED for no signature");
    }

    #[test]
    fn test_signature_badge_invalid() {
        let engine = TrustEngine::new();

        // Invalid: verification failure flag present
        let score = TrustScore {
            package: "invalid-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None,
            security_flags: vec![SecurityFlag::UnverifiedSignature],
            overall_score: 0.0,
        };
        let badge = engine.display_signature_badge(&score);
        assert!(badge.contains("INVALID"), "Should show INVALID when UnverifiedSignature flag present");
    }

    #[test]
    fn test_cache_freshness_with_recent_timestamp() {
        let engine = TrustEngine::new();

        // Cache with recent timestamp should be fresh
        let recent_score = TrustScore {
            package: "fresh-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: Some(Utc::now()), // Just now
            security_flags: vec![],
            overall_score: 5.0,
        };

        assert!(
            engine.is_cache_fresh(&recent_score),
            "Cache with recent timestamp should be fresh"
        );
    }

    #[test]
    fn test_cache_freshness_with_old_timestamp() {
        let engine = TrustEngine::new();

        // Cache with old timestamp should be stale (default TTL is 24 hours)
        let old_score = TrustScore {
            package: "stale-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: Some(Utc::now() - chrono::Duration::hours(48)), // 48 hours ago
            security_flags: vec![],
            overall_score: 5.0,
        };

        assert!(
            !engine.is_cache_fresh(&old_score),
            "Cache older than TTL should be stale"
        );
    }

    #[test]
    fn test_cache_freshness_with_no_timestamp() {
        let engine = TrustEngine::new();

        // Cache without timestamp should be stale
        let no_date_score = TrustScore {
            package: "no-date-pkg".to_string(),
            signature_valid: false,
            publisher_status: crate::tap::PublisherStatus::NotApplicable,
            community_votes: 0,
            maintainer_reputation: 5.0,
            last_audit_date: None, // No timestamp
            security_flags: vec![],
            overall_score: 5.0,
        };

        assert!(
            !engine.is_cache_fresh(&no_date_score),
            "Cache without timestamp should be stale"
        );
    }
}
