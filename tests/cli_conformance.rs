//! CLI conformance tests.
//!
//! These introspect the clap command graph (the single source of truth for the
//! CLI surface) and assert that the commands and flags referenced in the
//! documentation actually exist — and that flags we have explicitly documented
//! as *not* existing cannot silently reappear. This catches documentation drift
//! at test time instead of in a user's terminal.

use clap::CommandFactory;
use reap::cli::Cli;

fn long_flags(cmd: &clap::Command, sub: &str) -> Vec<String> {
    cmd.find_subcommand(sub)
        .unwrap_or_else(|| panic!("subcommand `{sub}` is missing from the CLI"))
        .get_arguments()
        .filter_map(|a| a.get_long().map(|s| s.to_string()))
        .collect()
}

fn subcommand_names(cmd: &clap::Command, sub: &str) -> Vec<String> {
    cmd.find_subcommand(sub)
        .unwrap_or_else(|| panic!("subcommand `{sub}` is missing from the CLI"))
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect()
}

#[test]
fn documented_top_level_commands_exist() {
    let cmd = Cli::command();
    // Commands referenced throughout README.md and docs/usage/commands.md.
    for name in [
        "install",
        "remove",
        "search",
        "update",
        "upgrade",
        "rollback",
        "tap",
        "gpg",
        "flatpak",
        "trust",
        "security",
        "doctor",
        "clean",
        "orphan",
        "pin",
        "backup",
        "config",
        "profile",
        "rate",
        "diff",
        "audit",
        "tui",
        "completion",
        "aur",
    ] {
        assert!(
            cmd.find_subcommand(name).is_some(),
            "documented command `reap {name}` is missing from the CLI"
        );
    }
}

#[test]
fn install_has_documented_flags_and_no_phantom_flags() {
    let cmd = Cli::command();
    let flags = long_flags(&cmd, "install");

    for expected in [
        "repo",
        "binary-only",
        "diff",
        "fast",
        "strict",
        "dry-run",
        "noconfirm",
        "skipreview",
        "insecure",
    ] {
        assert!(
            flags.iter().any(|f| f == expected),
            "documented `reap install --{expected}` flag is missing (have: {flags:?})"
        );
    }

    // These were removed/never existed; docs must not advertise them again.
    for phantom in ["force", "resolve-deps", "gpg-keyserver"] {
        assert!(
            !flags.iter().any(|f| f == phantom),
            "`reap install --{phantom}` does not exist but is present in the CLI"
        );
    }
}

#[test]
fn rollback_is_transaction_subcommand_based() {
    let cmd = Cli::command();
    let subs = subcommand_names(&cmd, "rollback");
    for expected in ["list", "show", "dry-run", "apply"] {
        assert!(
            subs.iter().any(|s| s == expected),
            "documented `reap rollback {expected}` subcommand is missing (have: {subs:?})"
        );
    }
}

#[test]
fn doctor_has_no_fix_flag() {
    let cmd = Cli::command();
    let flags = long_flags(&cmd, "doctor");
    assert!(
        !flags.iter().any(|f| f == "fix"),
        "`reap doctor --fix` does not exist but is present in the CLI"
    );
}

#[test]
fn gpg_has_no_set_keyserver_subcommand() {
    let cmd = Cli::command();
    let subs = subcommand_names(&cmd, "gpg");
    assert!(
        !subs.iter().any(|s| s == "set-keyserver"),
        "`reap gpg set-keyserver` was removed but is present in the CLI"
    );
}

#[test]
fn trust_has_documented_subcommands() {
    let cmd = Cli::command();
    let subs = subcommand_names(&cmd, "trust");
    for expected in ["score", "scan", "stats", "update"] {
        assert!(
            subs.iter().any(|s| s == expected),
            "documented `reap trust {expected}` subcommand is missing (have: {subs:?})"
        );
    }
}
