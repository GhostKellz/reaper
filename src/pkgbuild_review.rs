use crate::audit::{self, Confidence};
use crate::aur;
use crate::core::Source;
use crate::install_plan::{self, InstallPlan};
use crate::pacman;
use crate::paths;
use chrono::Utc;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl ReviewRisk {
    pub fn from_score(score: i32) -> Self {
        match score {
            0..=5 => ReviewRisk::Low,
            6..=15 => ReviewRisk::Medium,
            16..=30 => ReviewRisk::High,
            _ => ReviewRisk::Critical,
        }
    }

    pub fn is_high_risk(&self) -> bool {
        matches!(self, ReviewRisk::High | ReviewRisk::Critical)
    }

    pub fn label(&self) -> &'static str {
        match self {
            ReviewRisk::Low => "LOW",
            ReviewRisk::Medium => "MEDIUM",
            ReviewRisk::High => "HIGH",
            ReviewRisk::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PkgbuildReview {
    pub package: String,
    pub risk: ReviewRisk,
    pub risk_score: i32,
    pub infostealer_confidence: Confidence,
    pub warnings: Vec<String>,
    pub pkgbuild: String,
    pub pkgbuild_sha256: String,
    pub previous_sha256: Option<String>,
    pub changed_since_last_review: bool,
    pub diff_preview: Vec<String>,
    pub install_hook: String,
    pub install_hook_present: bool,
    pub install_hook_sha256: Option<String>,
    pub install_hook_changed: bool,
    pub install_hook_diff_preview: Vec<String>,
    pub dependencies: usize,
    pub make_dependencies: usize,
    pub check_dependencies: usize,
    pub conflicts: usize,
    pub provides: usize,
    pub source_downloads: usize,
}

impl PkgbuildReview {
    /// True when the unified engine has high-confidence infostealer evidence.
    pub fn is_infostealer_block(&self) -> bool {
        self.infostealer_confidence == Confidence::High
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewBaseline {
    package: String,
    pkgbuild_sha256: String,
    reviewed_at: String,
    pkgbuild: String,
    /// Persisted `.install` hook contents. Optional so baselines written by
    /// older versions (PKGBUILD-only) still deserialize cleanly.
    #[serde(default)]
    install_hook: Option<String>,
    #[serde(default)]
    install_hook_sha256: Option<String>,
}

/// Whether the `.install` hook changed between the stored baseline and the
/// current fetch. A hook that newly appears or disappears counts as a change,
/// since it runs as root and is a high-value target for a malicious update.
fn hook_changed(previous_sha: &Option<String>, current_sha: &Option<String>) -> bool {
    match (previous_sha, current_sha) {
        (Some(prev), Some(curr)) => prev != curr,
        (None, Some(_)) | (Some(_), None) => true,
        (None, None) => false,
    }
}

pub fn review_plan(plan: &InstallPlan) -> Vec<PkgbuildReview> {
    let mut reviews = Vec::new();
    for step in &plan.steps {
        if !step.needs_install() || !matches!(step.source, Source::Aur) {
            continue;
        }

        let pkgbuild = aur::get_pkgbuild_preview(&step.name);
        if pkgbuild.starts_with("[reap] PKGBUILD not found") {
            let baseline = load_baseline(&step.name);
            reviews.push(PkgbuildReview {
                package: step.name.clone(),
                risk: ReviewRisk::High,
                risk_score: 20,
                infostealer_confidence: Confidence::None,
                warnings: vec!["PKGBUILD could not be fetched for review".to_string()],
                pkgbuild: String::new(),
                pkgbuild_sha256: String::new(),
                previous_sha256: baseline.as_ref().map(|b| b.pkgbuild_sha256.clone()),
                changed_since_last_review: true,
                diff_preview: Vec::new(),
                install_hook: String::new(),
                install_hook_present: false,
                install_hook_sha256: None,
                install_hook_changed: false,
                install_hook_diff_preview: Vec::new(),
                dependencies: 0,
                make_dependencies: 0,
                check_dependencies: 0,
                conflicts: 0,
                provides: 0,
                source_downloads: 0,
            });
            continue;
        }

        let install_hook = aur::get_install_file_preview(&step.name, &pkgbuild);
        let report = audit::audit(&pkgbuild, &install_hook);
        let warnings = report.warnings();
        let risk_score = report.risk_score;
        let infostealer_confidence = report.infostealer_confidence;

        let audit_input = format!("{}\n{}", pkgbuild, install_hook);
        let deps = install_plan::parse_pkgbuild_deps(&audit_input);
        let pkgbuild_sha256 = sha256_hex(&pkgbuild);
        let baseline = load_baseline(&step.name);
        let previous_sha256 = baseline
            .as_ref()
            .map(|baseline| baseline.pkgbuild_sha256.clone());
        let changed_since_last_review = previous_sha256
            .as_ref()
            .is_none_or(|previous| previous != &pkgbuild_sha256);
        let diff_preview = baseline
            .as_ref()
            .filter(|_| changed_since_last_review)
            .map(|baseline| compact_diff_preview(&baseline.pkgbuild, &pkgbuild, 18))
            .unwrap_or_default();

        // Track the `.install` hook separately: it runs as root and is the
        // highest-value target for a malicious update, so any change to it
        // must be surfaced even when the PKGBUILD itself is unchanged.
        let install_hook_present = !install_hook.is_empty();
        let install_hook_sha256 = if install_hook_present {
            Some(sha256_hex(&install_hook))
        } else {
            None
        };
        let previous_hook = baseline.as_ref().and_then(|b| b.install_hook.clone());
        let previous_hook_sha = baseline
            .as_ref()
            .and_then(|b| b.install_hook_sha256.clone());
        let install_hook_changed = hook_changed(&previous_hook_sha, &install_hook_sha256);
        let install_hook_diff_preview = match (&previous_hook, install_hook_present) {
            (Some(prev), true) if install_hook_changed => {
                compact_diff_preview(prev, &install_hook, 18)
            }
            _ => Vec::new(),
        };
        let source_downloads = count_source_entries(&pkgbuild);

        reviews.push(PkgbuildReview {
            package: step.name.clone(),
            risk: ReviewRisk::from_score(risk_score),
            risk_score,
            infostealer_confidence,
            warnings,
            pkgbuild,
            pkgbuild_sha256,
            previous_sha256,
            changed_since_last_review,
            diff_preview,
            install_hook,
            install_hook_present,
            install_hook_sha256,
            install_hook_changed,
            install_hook_diff_preview,
            dependencies: deps.depends.len(),
            make_dependencies: deps.make_depends.len(),
            check_dependencies: deps.check_depends.len(),
            conflicts: deps.conflicts.len(),
            provides: deps.provides.len(),
            source_downloads,
        });
    }
    reviews
}

pub fn print_reviews(reviews: &[PkgbuildReview]) {
    if reviews.is_empty() {
        return;
    }

    println!();
    println!("{}", "PKGBUILD review".bold());
    println!("{}", "---------------".dimmed());

    for review in reviews {
        let risk = match review.risk {
            ReviewRisk::Low => review.risk.label().green().to_string(),
            ReviewRisk::Medium => review.risk.label().yellow().to_string(),
            ReviewRisk::High | ReviewRisk::Critical => review.risk.label().red().bold().to_string(),
        };

        println!(
            "{} {} risk={} score={} state={} deps={}/make={}/check={} sources={} conflicts={} provides={}",
            "review".cyan(),
            review.package.bright_white(),
            risk,
            review.risk_score,
            review_state_label(review),
            review.dependencies,
            review.make_dependencies,
            review.check_dependencies,
            review.source_downloads,
            review.conflicts,
            review.provides
        );

        if review.is_infostealer_block() {
            println!(
                "  {} high-confidence infostealer behavior (sensitive read correlated with exfiltration)",
                "BLOCK".red().bold()
            );
        } else if review.infostealer_confidence == Confidence::Medium {
            println!(
                "  {} possible infostealer indicators (medium confidence)",
                "warn".yellow()
            );
        }

        if review.install_hook_present {
            let state = if review.install_hook_changed {
                ".install hook changed since last accepted review"
                    .red()
                    .bold()
                    .to_string()
            } else {
                ".install hook present; reviewed with PKGBUILD".to_string()
            };
            println!("  {} {}", "warn".yellow(), state);
        }

        if !review.diff_preview.is_empty() {
            println!(
                "  {} PKGBUILD changed since last accepted review",
                "diff".blue()
            );
            for line in &review.diff_preview {
                println!("    {}", line);
            }
        }

        if !review.install_hook_diff_preview.is_empty() {
            println!("  {} .install hook diff", "diff".blue());
            for line in &review.install_hook_diff_preview {
                println!("    {}", line);
            }
        }

        for warning in review.warnings.iter().take(6) {
            println!("  - {}", warning);
        }
        if review.warnings.len() > 6 {
            println!("  - ... {} more finding(s)", review.warnings.len() - 6);
        }
    }
    println!();
}

pub fn has_high_risk_review(reviews: &[PkgbuildReview]) -> bool {
    reviews.iter().any(|review| review.risk.is_high_risk())
}

/// True when any review carries high-confidence infostealer evidence. Callers
/// use this to enforce the default install block (override only via `--insecure`).
pub fn has_infostealer_block(reviews: &[PkgbuildReview]) -> bool {
    reviews.iter().any(PkgbuildReview::is_infostealer_block)
}

/// Names of packages flagged as high-confidence infostealers, for messaging.
pub fn infostealer_blocked_packages(reviews: &[PkgbuildReview]) -> Vec<String> {
    reviews
        .iter()
        .filter(|review| review.is_infostealer_block())
        .map(|review| review.package.clone())
        .collect()
}

pub fn record_review_baselines(reviews: &[PkgbuildReview]) -> io::Result<()> {
    if reviews.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(paths::review_dir())?;
    for review in reviews {
        if review.pkgbuild.is_empty() || pacman::get_version(&review.package).is_none() {
            continue;
        }

        let baseline = ReviewBaseline {
            package: review.package.clone(),
            pkgbuild_sha256: review.pkgbuild_sha256.clone(),
            reviewed_at: Utc::now().to_rfc3339(),
            pkgbuild: review.pkgbuild.clone(),
            install_hook: if review.install_hook_present {
                Some(review.install_hook.clone())
            } else {
                None
            },
            install_hook_sha256: review.install_hook_sha256.clone(),
        };
        let json = serde_json::to_string_pretty(&baseline)?;
        fs::write(review_path(&review.package), json)?;
    }

    Ok(())
}

/// Render a colored diff of a package's current PKGBUILD and `.install` hook
/// against the last accepted review baseline, followed by a short audit summary.
///
/// Used by `reap diff <pkg>` and the `install --diff` preflight. When no
/// baseline exists yet, the current contents are audited and reported as new.
pub fn render_diff(package: &str) {
    let remote_pkgbuild = aur::get_pkgbuild_preview(package);
    if remote_pkgbuild.starts_with("[reap] PKGBUILD not found") {
        eprintln!("[diff] PKGBUILD for {} could not be fetched", package);
        return;
    }
    let remote_hook = aur::get_install_file_preview(package, &remote_pkgbuild);
    let baseline = load_baseline(package);

    println!("{} {}", "diff".bold(), package.bright_white());
    match baseline.as_ref() {
        Some(base) => {
            if base.pkgbuild == remote_pkgbuild {
                println!("  PKGBUILD: {}", "unchanged since last review".green());
            } else {
                println!(
                    "  PKGBUILD: {}",
                    "changed since last review".yellow().bold()
                );
                print_colored_diff(&base.pkgbuild, &remote_pkgbuild);
            }

            let prev_hook = base.install_hook.clone().unwrap_or_default();
            if prev_hook.is_empty() && remote_hook.is_empty() {
                // No hook on either side; nothing to report.
            } else if prev_hook == remote_hook {
                println!("  .install hook: {}", "unchanged".green());
            } else {
                println!(
                    "  .install hook: {}",
                    "changed since last review".red().bold()
                );
                print_colored_diff(&prev_hook, &remote_hook);
            }
        }
        None => {
            println!(
                "  {}",
                "no prior reviewed baseline; showing current contents".dimmed()
            );
            print_colored_diff("", &remote_pkgbuild);
            if !remote_hook.is_empty() {
                println!("  .install hook ({}):", "new".bright_white());
                print_colored_diff("", &remote_hook);
            }
        }
    }

    let report = audit::audit(&remote_pkgbuild, &remote_hook);
    println!(
        "  audit: risk={} infostealer-confidence={}",
        report.risk_score,
        report.infostealer_confidence.label()
    );
    if report.is_infostealer_block() {
        println!(
            "  {} install of this package is blocked by default (override: --insecure)",
            "BLOCK".red().bold()
        );
    }
}

fn print_colored_diff(previous: &str, current: &str) {
    for change in diff::lines(previous, current) {
        match change {
            diff::Result::Left(l) => println!("    {}", format!("- {}", l).red()),
            diff::Result::Right(r) => println!("    {}", format!("+ {}", r).green()),
            diff::Result::Both(l, _) => println!("      {}", l.dimmed()),
        }
    }
}

fn review_state_label(review: &PkgbuildReview) -> String {
    match (
        review.previous_sha256.is_some(),
        review.changed_since_last_review,
    ) {
        (false, _) => "new".bright_white().to_string(),
        (true, false) => "unchanged".green().to_string(),
        (true, true) => "changed".yellow().bold().to_string(),
    }
}

fn load_baseline(package: &str) -> Option<ReviewBaseline> {
    let content = fs::read_to_string(review_path(package)).ok()?;
    serde_json::from_str(&content).ok()
}

fn review_path(package: &str) -> PathBuf {
    paths::review_dir().join(format!("{}.json", safe_package_name(package)))
}

fn safe_package_name(package: &str) -> String {
    package
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '+' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn compact_diff_preview(previous: &str, current: &str, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for change in diff::lines(previous, current) {
        match change {
            diff::Result::Left(line) => lines.push(format!("- {}", line)),
            diff::Result::Right(line) => lines.push(format!("+ {}", line)),
            diff::Result::Both(_, _) => continue,
        }

        if lines.len() == max_lines {
            lines.push(format!(
                "... diff truncated to {} changed line(s)",
                max_lines
            ));
            break;
        }
    }
    lines
}

fn count_source_entries(pkgbuild: &str) -> usize {
    let parsed = parse_array(pkgbuild, "source");
    if parsed.is_empty() {
        parse_array(pkgbuild, "source_x86_64").len()
    } else {
        parsed.len()
    }
}

fn parse_array(content: &str, name: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut active = false;
    let mut buffer = String::new();
    let prefix = format!("{}=(", name);

    for line in content.lines().map(str::trim) {
        if active {
            buffer.push(' ');
            buffer.push_str(line.trim_end_matches(')'));
            if line.ends_with(')') {
                active = false;
                values.extend(split_array_items(&buffer));
                buffer.clear();
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix(&prefix) {
            if line.ends_with(')') {
                values.extend(split_array_items(rest.trim_end_matches(')')));
            } else {
                active = true;
                buffer = rest.to_string();
            }
        }
    }

    values
}

fn split_array_items(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|item| item.trim_matches(',').trim_matches('"').trim_matches('\''))
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_labels_track_scores() {
        assert_eq!(ReviewRisk::from_score(0), ReviewRisk::Low);
        assert_eq!(ReviewRisk::from_score(8), ReviewRisk::Medium);
        assert_eq!(ReviewRisk::from_score(20), ReviewRisk::High);
        assert_eq!(ReviewRisk::from_score(40), ReviewRisk::Critical);
    }

    #[test]
    fn counts_multiline_source_entries() {
        let pkgbuild = r#"
source=(
  "https://example.test/a.tar.gz"
  'git+https://example.test/repo'
)
"#;
        assert_eq!(count_source_entries(pkgbuild), 2);
    }

    #[test]
    fn diff_preview_shows_added_and_removed_lines() {
        let old = "pkgname=test\npkgver=1\nsource=(old.tar.gz)\n";
        let new = "pkgname=test\npkgver=2\nsource=(new.tar.gz)\n";
        let preview = compact_diff_preview(old, new, 8);

        assert!(preview.iter().any(|line| line == "- pkgver=1"));
        assert!(preview.iter().any(|line| line == "+ pkgver=2"));
        assert!(preview.iter().any(|line| line == "- source=(old.tar.gz)"));
        assert!(preview.iter().any(|line| line == "+ source=(new.tar.gz)"));
    }

    #[test]
    fn package_names_are_safe_for_review_paths() {
        assert_eq!(safe_package_name("foo/bar baz"), "foo_bar_baz");
        assert_eq!(safe_package_name("libfoo-git+v2"), "libfoo-git+v2");
    }

    #[test]
    fn hook_change_detection_covers_presence_transitions() {
        let a = Some("aaa".to_string());
        let b = Some("bbb".to_string());
        // Unchanged hook.
        assert!(!hook_changed(&a, &a.clone()));
        // Content changed.
        assert!(hook_changed(&a, &b));
        // Hook newly added (none -> some) and removed (some -> none) both count.
        assert!(hook_changed(&None, &a));
        assert!(hook_changed(&a, &None));
        // No hook in either baseline or current.
        assert!(!hook_changed(&None, &None));
    }

    #[test]
    fn legacy_baseline_without_install_hook_deserializes() {
        // Baselines written before hook tracking existed have no install_hook
        // fields; #[serde(default)] must let them load as None rather than error.
        let legacy = r#"{
            "package": "foo",
            "pkgbuild_sha256": "deadbeef",
            "reviewed_at": "2026-01-01T00:00:00Z",
            "pkgbuild": "pkgname=foo\n"
        }"#;
        let baseline: ReviewBaseline =
            serde_json::from_str(legacy).expect("legacy baseline should deserialize");
        assert_eq!(baseline.package, "foo");
        assert!(baseline.install_hook.is_none());
        assert!(baseline.install_hook_sha256.is_none());
    }
}
