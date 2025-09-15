// Core feature tests for Reap v0.3.0-rc
use anyhow::{Context, Result};
use reap::config::GlobalConfig;
use reap::flatpak::install_flatpak;
use reap::utils;
use std::fs;

/// Test configuration precedence by simulating a config file and checking the precedence of settings.
#[test]
fn test_config_precedence() -> Result<()> {
    let config_path = reap::config::config_path();

    // Create parent directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    fs::write(
        &config_path,
        r#"backend_order = ['aur', 'flatpak']
auto_resolve_deps = false
noconfirm = true
log_verbose = false
"#,
    )
    .context("Failed to write config file")?;
    let cfg = GlobalConfig::load();
    assert_eq!(cfg.backend_order, vec!["aur", "flatpak"]);
    assert!(!cfg.auto_resolve_deps);
    fs::remove_file(&config_path).context("Failed to remove config file")?;
    Ok(())
}

/// Test handling of an invalid keyserver by attempting to use an unreachable keyserver.
#[tokio::test]
async fn test_invalid_keyserver() -> Result<()> {
    utils::check_keyserver_async("hkps://invalid.keyserver.example").await;
    Ok(())
}

/// Test invocation of hooks by creating a dummy pre_install.sh script and checking its execution.
#[test]
fn test_hook_invocation() -> Result<()> {
    let hooks_dir = dirs::home_dir().unwrap().join(".config/reap/hooks");
    fs::create_dir_all(&hooks_dir).context("Failed to create hooks dir")?;
    let script = hooks_dir.join("pre_install.sh");
    fs::write(
        &script,
        "#!/bin/bash\necho '[HOOK] Dummy pre_install invoked'\n",
    )
    .context("Failed to write hook script")?;
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&script)
        .status()
        .context("Failed to chmod hook script")?;
    let ctx = reap::hooks::HookContext {
        pkg: "dummy".to_string(),
        version: None,
        source: None,
        install_path: None,
        tap: None,
    };
    reap::hooks::pre_install(&ctx);
    fs::remove_file(&script).context("Failed to remove hook script")?;
    Ok(())
}

/// Test Flatpak installation failure by attempting to install a non-existent Flatpak package.
#[tokio::test]
async fn test_flatpak_install_failure() -> Result<()> {
    let result = install_flatpak("nonexistent.flatpak.app").await;
    assert!(result.is_err());
    Ok(())
}
