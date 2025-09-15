mod aur;
mod backend;
mod cli;
mod config;
mod core;
mod enhanced_aur;
mod flatpak;
mod gpg;
mod hooks;
mod interactive;
mod pacman;
mod profiles;
mod tap;
mod trust;
mod tui;
mod utils;

use crate::backend::Backend;
use crate::cli::Commands;
use clap::Parser;
use cli::Cli;

#[cfg(debug_assertions)]
async fn test_parallel_runners() {
    use crate::config::ReapConfig;
    use crate::core::{install_with_priority, parallel_install, parallel_upgrade};
    use crate::tui::LogPane;
    let config = std::sync::Arc::new(ReapConfig::load());
    let log = std::sync::Arc::new(LogPane::default());
    parallel_install(
        &["yay".to_string(), "zsh".to_string()],
        config.clone(),
        log.clone(),
    )
    .await;
    parallel_upgrade(
        &["firefox".to_string(), "ripgrep".to_string()],
        config.clone(),
        log.clone(),
    )
    .await;
    install_with_priority(
        "htop",
        config,
        true,
        log,
        &crate::core::InstallOptions::default(),
    )
    .await;
}

#[tokio::main]
async fn main() {
    // Auto-sync enabled taps before any command
    if let Err(e) = tap::sync_enabled_taps() {
        eprintln!("Warning: Failed to sync taps: {}", e);
    }
    #[cfg(debug_assertions)]
    tokio::spawn(test_parallel_runners());
    let cli = Cli::parse();

    // Handle pacman-style flags first (-Sy, -Syu, -S <pkg>, etc.)
    if let Some(packages) = &cli.sync {
        if cli.refresh && cli.upgrade {
            // -Syu: refresh database and upgrade all
            println!("🔄 Refreshing package database and upgrading all packages...");
            core::handle_upgrade_all();
            return;
        } else if cli.refresh {
            // -Sy: refresh database only
            println!("🔄 Refreshing package database...");
            core::handle_sync_db();
            if !packages.is_empty() {
                // -Sy <pkg>: refresh then install packages
                for pkg in packages {
                    let config = std::sync::Arc::new(config::ReapConfig::load());
                    let log = std::sync::Arc::new(tui::LogPane::default());
                    let options = core::InstallOptions::default();
                    core::install_with_priority(pkg, config, true, log, &options).await;
                }
            }
            return;
        } else if cli.upgrade {
            // -Su: upgrade packages
            core::handle_upgrade_all();
            return;
        } else if !packages.is_empty() {
            // -S <pkg>: install packages
            for pkg in packages {
                let config = std::sync::Arc::new(config::ReapConfig::load());
                let log = std::sync::Arc::new(tui::LogPane::default());
                let options = core::InstallOptions::default();
                core::install_with_priority(pkg, config, true, log, &options).await;
            }
            return;
        }
    }

    if let Some(packages) = &cli.remove {
        // -R <pkg>: remove packages
        let interactive = crate::interactive::InteractiveManager::new();
        if interactive.confirm_removal(packages) {
            core::handle_removal(packages);
        }
        return;
    }

    if let Some(terms) = &cli.search {
        // -Q <term>: search packages
        core::handle_search(terms);
        return;
    }

    if let Some(paths) = &cli.local {
        // -U <path>: install local packages
        core::handle_local_install(paths);
        return;
    }

    // All install/upgrade flows use Reap's own async/parallel logic (no yay/paru fallback)
    if let Err(e) = core::handle_cli(&cli).await {
        eprintln!("[reap] CLI error: {e}");
        std::process::exit(1);
    }
    let config = config::ReapConfig::load();
    println!(
        "[main] Loaded config with parallel level: {}",
        config.parallel
    );
    match cli.command {
        Commands::Audit { pkg } => {
            // Use the backend trait's audit method
            let backend = backend::AurBackend::new();
            tokio::spawn(async move {
                backend.audit(&pkg).await;
            });
        }
        Commands::Rollback { pkg } => core::handle_rollback(&pkg),
        Commands::SyncDb => println!("Syncing pacman database..."),
        Commands::Pin { pkg } => {
            if let Err(e) = crate::utils::pin_package(&pkg) {
                eprintln!("[reap] Pin failed: {}", e);
            } else {
                println!("[reap] Pinned {}", pkg);
            }
        }
        Commands::Tui => {
            let _config = config::ReapConfig::load();
            tokio::spawn(crate::tui::launch_tui()).await.unwrap();
        }
        Commands::Profile { cmd } => {
            let mut profile_manager = profiles::ProfileManager::new();
            match cmd {
                cli::ProfileCmd::Create { name, template } => {
                    let profile = match template.as_deref() {
                        Some("developer") => profiles::create_developer_profile(),
                        Some("gaming") => profiles::create_gaming_profile(),
                        Some("minimal") => profiles::create_minimal_profile(),
                        _ => profiles::ProfileConfig {
                            name: name.clone(),
                            ..Default::default()
                        },
                    };
                    if let Err(e) = profile_manager.create_profile(&profile) {
                        eprintln!("[profiles] Failed to create profile: {}", e);
                    }
                }
                cli::ProfileCmd::Switch { name } => {
                    if let Err(e) = profile_manager.switch_profile(&name) {
                        eprintln!("[profiles] Failed to switch profile: {}", e);
                    }
                }
                cli::ProfileCmd::List => {
                    if let Ok(profiles) = profile_manager.list_profiles() {
                        println!("[profiles] Available profiles:");
                        for profile in profiles {
                            println!("  - {}", profile);
                        }
                    }
                }
                cli::ProfileCmd::Show { name } => {
                    if let Ok(profile) = profile_manager.load_profile(&name) {
                        println!("[profiles] Profile '{}': {:?}", name, profile);
                    }
                }
                cli::ProfileCmd::Delete { name } => {
                    if let Err(e) = profile_manager.delete_profile(&name) {
                        eprintln!("[profiles] Failed to delete profile: {}", e);
                    }
                }
                cli::ProfileCmd::Edit { .. } => {
                    println!("[profiles] Edit functionality coming soon");
                }
            }
        }
        Commands::Trust { cmd } => {
            let trust_engine = trust::TrustEngine::new();
            match cmd {
                cli::TrustCmd::Score { pkg } => {
                    let source =
                        core::detect_source(&pkg, None, false).unwrap_or(core::Source::Aur);
                    let trust_score = trust_engine.compute_trust_score(&pkg, &source).await;
                    let badge = trust_engine.display_trust_badge(trust_score.overall_score);
                    println!(
                        "[trust] {} {} (Score: {:.1}/10)",
                        pkg, badge, trust_score.overall_score
                    );
                    for flag in trust_score.security_flags {
                        println!("[trust] ⚠️ {:?}", flag);
                    }
                }
                cli::TrustCmd::Scan => {
                    println!("[trust] Scanning all installed packages...");
                    // TODO: Implement full system scan
                }
                cli::TrustCmd::Stats => {
                    println!("[trust] Trust statistics coming soon");
                }
                cli::TrustCmd::Update => {
                    println!("[trust] Updating trust database...");
                }
            }
        }
        Commands::Install {
            pkg,
            repo: _,
            binary_only: _,
            diff,
        } => {
            let config = std::sync::Arc::new(config::ReapConfig::load());
            let log = std::sync::Arc::new(tui::LogPane::default());

            if diff {
                // Show PKGBUILD diff before install
                core::show_pkgbuild_diff(&pkg);

                if !interactive::InteractiveManager::confirm_action(
                    "Continue with installation?",
                    true,
                ) {
                    return;
                }
            }

            // Backup package state before install
            if let Err(e) = core::backup_package_state(&pkg) {
                eprintln!("[backup] Warning: Failed to backup package state: {}", e);
            }

            // Use priority-based install
            let options = core::InstallOptions::default();
            core::install_with_priority(&pkg, config, true, log, &options).await;
        }

        Commands::Rate {
            pkg,
            rating,
            comment,
        } => {
            let mut interactive = interactive::InteractiveManager::new();

            // Get and display current rating
            if let Ok(pkg_rating) = interactive.get_package_rating(&pkg).await {
                println!(
                    "Current rating: {}",
                    interactive.display_rating(&pkg_rating)
                );
            }

            if let Err(e) = interactive.submit_user_rating(&pkg, rating, comment.clone()) {
                eprintln!("[rating] Failed to submit rating: {}", e);
            }
        }
        Commands::Aur { cmd } => {
            let mut aur_manager = enhanced_aur::EnhancedAurManager::new();
            match cmd {
                cli::AurCmd::Fetch { pkg } => match aur_manager.fetch_pkgbuild(&pkg).await {
                    Ok(pkgbuild) => println!("[aur] PKGBUILD fetched: {:?}", pkgbuild),
                    Err(e) => eprintln!("[aur] Failed to fetch PKGBUILD: {}", e),
                },
                cli::AurCmd::Edit { pkg } => {
                    let interactive = interactive::InteractiveManager::new();
                    if interactive.confirm_pkgbuild_edit(&pkg) {
                        if let Err(e) = aur_manager.edit_pkgbuild(&pkg) {
                            eprintln!("[aur] Failed to edit PKGBUILD: {}", e);
                        }
                    }
                }
                cli::AurCmd::Deps { pkg, conflicts: _ } => {
                    match aur_manager
                        .resolve_dependencies_advanced(&[pkg.clone()])
                        .await
                    {
                        Ok(conflicts_found) => {
                            if conflicts_found.is_empty() {
                                println!("[aur] ✅ No conflicts detected for {}", pkg);
                            } else {
                                println!("[aur] ⚠️ {} conflicts detected:", conflicts_found.len());
                                for conflict in conflicts_found {
                                    println!("  • {:?}", conflict);
                                }
                            }
                        }
                        Err(e) => eprintln!("[aur] Failed to resolve dependencies: {}", e),
                    }
                }
            }
        }
        Commands::BatchInstall { pkgs, parallel } => {
            let config = std::sync::Arc::new(config::ReapConfig::load());
            let log = std::sync::Arc::new(tui::LogPane::default());

            if parallel {
                log.push(&format!(
                    "[batch] Installing {} packages in parallel",
                    pkgs.len()
                ));
                core::parallel_install(&pkgs, config, log).await;
            } else {
                for pkg in pkgs {
                    log.push(&format!("[batch] Installing {}", pkg));
                    let options = core::InstallOptions::default();
                    core::install_with_priority(&pkg, config.clone(), true, log.clone(), &options)
                        .await;
                }
            }
        }
        Commands::Remove { pkgs } => {
            let interactive = interactive::InteractiveManager::new();
            if interactive.confirm_removal(&pkgs) {
                core::handle_removal(&pkgs);
            }
        }
        Commands::Local { pkgs } => {
            core::handle_local_install(&pkgs);
        }
        Commands::Search { terms } => {
            core::handle_search(&terms);
        }
        Commands::Update => {
            core::handle_update();
        }
        Commands::Upgrade { parallel } => {
            core::handle_upgrade(parallel);
        }
        Commands::ParallelUpgrade { pkgs } => {
            let config = std::sync::Arc::new(config::ReapConfig::load());
            let log = std::sync::Arc::new(tui::LogPane::default());

            log.push(&format!(
                "[parallel] Upgrading {} packages in parallel",
                pkgs.len()
            ));
            core::parallel_upgrade(&pkgs, config, log).await;
        }
        Commands::UpgradeAll => {
            core::handle_upgrade_all();
        }
        Commands::FlatpakUpgrade => {
            println!("Upgrading Flatpak packages...");
        }
        Commands::Clean => {
            core::handle_clean();
            // Also clean cache using utils
            match utils::clean_cache() {
                Ok(msg) => println!("[clean] {}", msg),
                Err(e) => eprintln!("[clean] Error: {}", e),
            }
        }
        Commands::Doctor => {
            core::handle_doctor();
        }
        Commands::Perf { cmd } => match cmd {
            cli::PerfCmd::WarmCache => {
                println!("[perf] Warming cache with popular packages...");
                tokio::spawn(async {
                    if let Err(e) = aur::warm_cache().await {
                        eprintln!("[perf] Cache warming failed: {}", e);
                    }
                });
            }
            cli::PerfCmd::ParallelSearch { queries } => {
                println!(
                    "[perf] Running parallel search for {} queries",
                    queries.len()
                );
                tokio::spawn(async move {
                    match aur::parallel_search(&queries).await {
                        Ok(results) => println!("[perf] Found {} total results", results.len()),
                        Err(e) => eprintln!("[perf] Parallel search failed: {}", e),
                    }
                });
            }
            cli::PerfCmd::ParallelFetch { packages } => {
                println!(
                    "[perf] Running parallel PKGBUILD fetch for {} packages",
                    packages.len()
                );
                tokio::spawn(async move {
                    match aur::parallel_pkgbuild_fetch(&packages).await {
                        Ok(downloads) => println!(
                            "[perf] Successfully downloaded {} PKGBUILDs",
                            downloads.len()
                        ),
                        Err(e) => eprintln!("[perf] Parallel fetch failed: {}", e),
                    }
                });
            }
            cli::PerfCmd::CacheStats => {
                println!("[perf] Cache statistics:");
                #[cfg(feature = "cache")]
                {
                    println!("  Cache directory: {:?}", *utils::cache::PKGBUILD_CACHE_DIR);
                    if let Ok(entries) = std::fs::read_dir(&*utils::cache::PKGBUILD_CACHE_DIR) {
                        let count = entries.count();
                        println!("  Cached PKGBUILDs: {}", count);
                    }
                }
                #[cfg(not(feature = "cache"))]
                println!("  Caching disabled (compile with --features cache)");
            }
            cli::PerfCmd::ClearCache => match utils::clean_cache() {
                Ok(msg) => println!("[perf] {}", msg),
                Err(e) => eprintln!("[perf] Cache clear error: {}", e),
            },
        },
        Commands::Security { cmd } => match cmd {
            cli::SecurityCmd::Audit { pkg } => {
                println!("[security] Auditing package: {}", pkg);
                let pkgbuild = aur::get_pkgbuild_preview(&pkg);
                let (warnings, risk_score) = utils::audit_pkgbuild(&pkgbuild);

                if warnings.is_empty() {
                    println!("✅ Package {} passed security audit", pkg);
                } else {
                    println!("⚠️ Package {} security audit findings:", pkg);
                    for warning in warnings {
                        println!("  {}", warning);
                    }
                }
                println!("🛡️ Security risk score: {}", risk_score);
            }
            cli::SecurityCmd::ScanAll => {
                println!("[security] Scanning all installed packages...");
                let installed = core::get_installed_packages();
                let mut total_risk = 0;
                let mut risky_packages = Vec::new();

                for (pkg, source) in installed {
                    if matches!(source, core::Source::Aur) {
                        let pkgbuild = aur::get_pkgbuild_preview(&pkg);
                        let (warnings, risk_score) = utils::audit_pkgbuild(&pkgbuild);

                        if risk_score > 15 {
                            risky_packages.push((pkg, risk_score, warnings.len()));
                        }
                        total_risk += risk_score;
                    }
                }

                println!("🛡️ Security scan complete:");
                println!("  Total risk score: {}", total_risk);
                println!("  High-risk packages: {}", risky_packages.len());

                for (pkg, score, warning_count) in risky_packages {
                    println!("    {} (score: {}, {} warnings)", pkg, score, warning_count);
                }
            }
            cli::SecurityCmd::Stats => {
                println!("[security] Security statistics:");
                println!("  Security rules: 38 patterns");
                println!("  Domain blacklist: 7 entries");
                println!("  Credential patterns: 10 patterns");
            }
            cli::SecurityCmd::UpdateRules => {
                println!("[security] Security rules are built-in and updated with releases");
            }
        },
        Commands::Gpg { cmd } => match cmd {
            cli::GpgCmd::Refresh => {
                println!("Refreshing GPG keys...");
                gpg::refresh_keys();
            }
            cli::GpgCmd::Import { keyid } => {
                println!("Importing GPG key: {}", keyid);
                tokio::spawn(async move {
                    if let Err(e) = gpg::import_gpg_key_async(&keyid).await {
                        eprintln!("[reap] Failed to import GPG key: {}", e);
                    }
                });
            }
            cli::GpgCmd::Show { keyid } => {
                println!("Showing GPG key: {}", keyid);
                gpg::show_key(&keyid);
            }
            cli::GpgCmd::Check { keyid } => {
                println!("Checking GPG key: {}", keyid);
                if gpg::key_exists(&keyid) {
                    println!("[reap] GPG key {} exists in keyring", keyid);
                } else {
                    println!("[reap] GPG key {} not found in keyring", keyid);
                }
            }
            cli::GpgCmd::VerifyPkgbuild { path } => {
                println!("Verifying PKGBUILD: {}", path);
                match gpg::gpg_check(std::path::Path::new(&path)) {
                    Ok(()) => println!("[reap] PKGBUILD signature verified"),
                    Err(e) => eprintln!("[reap] PKGBUILD verification failed: {}", e),
                }
            }
            cli::GpgCmd::SetKeyserver { url } => {
                println!("Setting GPG keyserver: {}", url);
                utils::cli_set_keyserver(&url);
            }
            cli::GpgCmd::CheckKeyserver { url } => {
                println!("Checking GPG keyserver: {}", url);
                tokio::spawn(async move {
                    utils::check_keyserver_async(&url).await;
                });
            }
        },
        Commands::Flatpak { cmd } => match cmd {
            cli::FlatpakCmd::Install { pkg } => {
                println!("Installing Flatpak package: {}", pkg);
            }
            cli::FlatpakCmd::Remove { pkg } => {
                println!("Removing Flatpak package: {}", pkg);
            }
            cli::FlatpakCmd::Search { query } => {
                println!("Searching Flatpak packages: {}", query);
            }
            cli::FlatpakCmd::Update => {
                println!("Updating Flatpak packages...");
            }
            cli::FlatpakCmd::List => {
                println!("Listing Flatpak packages...");
            }
            cli::FlatpakCmd::Upgrade => {
                println!("Upgrading Flatpak packages...");
            }
            cli::FlatpakCmd::Audit { pkg } => {
                println!("Auditing Flatpak package: {}", pkg);
            }
        },
        Commands::Tap { cmd } => match cmd {
            cli::TapCmd::Add {
                name,
                url,
                priority,
            } => {
                crate::tap::add_or_update_tap(&name, &url, Some(priority as u8), true);
            }
            cli::TapCmd::Remove { name } => {
                crate::tap::remove_tap(&name);
            }
            cli::TapCmd::Enable { name } => {
                crate::tap::set_tap_enabled(&name, true);
            }
            cli::TapCmd::Disable { name } => {
                crate::tap::set_tap_enabled(&name, false);
            }
            cli::TapCmd::Update => {
                crate::tap::sync_taps();
            }
            cli::TapCmd::Sync => {
                crate::tap::sync_taps();
            }
            cli::TapCmd::List => {
                crate::tap::list_taps();
            }
        },
        Commands::Completion { shell } => {
            println!("Generating completion for shell: {}", shell);
        }
        Commands::Backup => {
            println!("Backing up configuration...");
        }
        Commands::Orphan { remove, all } => {
            core::handle_orphan(remove, all);
        }
        Commands::Config { cmd } => match cmd {
            cli::ConfigCmd::Show => {
                let config = config::ReapConfig::load();
                println!("Current configuration: {:#?}", config);
            }
            cli::ConfigCmd::Set { key, value } => {
                crate::config::set_config_key(&key, &value);
            }
            cli::ConfigCmd::Get { key } => {
                if let Some(val) = crate::config::get_config_key(&key) {
                    println!("{} = {}", key, val);
                } else {
                    println!("Key '{}' not found in config.", key);
                }
            }
            cli::ConfigCmd::Reset => {
                crate::config::reset_config();
            }
        },
    }
}
