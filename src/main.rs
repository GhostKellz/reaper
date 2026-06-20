use clap::Parser;
use owo_colors::OwoColorize;
use reap::backend::{self, Backend};
use reap::cli::{self, Cli, Commands, PacmanAction};
use reap::{
    audit, aur, config, core, dpkg, enhanced_aur, flatpak, gpg, install_plan, interactive, pacman,
    paths, pkgbuild_review, profiles, tap, transaction, trust, tui, utils,
};

#[tokio::main]
async fn main() {
    // Migrate legacy paths if needed (non-destructive, safe)
    let migrated = paths::migrate_legacy_paths();
    for m in migrated {
        println!("[reap] Migrated: {}", m);
    }

    // NOTE: Tap sync removed from startup - use explicit `reap tap sync` command
    // This prevents side effects on read-only operations like search/info

    let cli = Cli::parse();

    // Handle pacman-style flags first (e.g., -Syu, -Ss, -R)
    if let Some(action) = cli.resolve_pacman_flags() {
        if !handle_pacman_action(action).await {
            std::process::exit(1);
        }
        return;
    }

    // Fall back to subcommand dispatch
    let Some(command) = cli.command else {
        // No command and no pacman flags - show help
        eprintln!("No command specified. Use --help for usage information.");
        std::process::exit(1);
    };

    // Dispatch based on subcommand
    match command {
        Commands::Audit { pkg } => {
            // Use the backend trait's audit method
            let backend = backend::AurBackend::new();
            backend.audit(&pkg).await;
        }
        Commands::Diff { pkg } => {
            pkgbuild_review::render_diff(&pkg);
        }
        Commands::Rollback { cmd } => {
            let journal = transaction::TransactionJournal::new();
            match cmd {
                cli::RollbackCmd::List { limit, package } => {
                    let records = if let Some(pkg) = package {
                        journal.transactions_for_package(&pkg).unwrap_or_default()
                    } else {
                        journal.list_transactions(Some(limit)).unwrap_or_default()
                    };
                    display_transaction_list(&records);
                }
                cli::RollbackCmd::Show { txid } => match journal.load_transaction(&txid) {
                    Ok(record) => display_transaction_details(&record),
                    Err(e) => {
                        eprintln!("[rollback] {}", e);
                        eprintln!(
                            "[rollback] Run 'reap rollback list' to see available transactions."
                        );
                        std::process::exit(1);
                    }
                },
                cli::RollbackCmd::DryRun { txid } => match journal.load_transaction(&txid) {
                    Ok(record) => display_rollback_preview(&record),
                    Err(e) => {
                        eprintln!("[rollback] {}", e);
                        eprintln!(
                            "[rollback] Run 'reap rollback list' to see available transactions."
                        );
                        std::process::exit(1);
                    }
                },
                cli::RollbackCmd::Apply { txid, yes } => match journal.load_transaction(&txid) {
                    Ok(mut record) => {
                        let plan = transaction::create_rollback_plan(
                            &record,
                            &transaction::artifact_search_dirs(),
                        );
                        if plan.downgrades.is_empty()
                            && plan.reinstalls.is_empty()
                            && plan.removals.is_empty()
                        {
                            eprintln!("[rollback] Nothing to rollback for transaction {}", txid);
                            std::process::exit(1);
                        }

                        // Show preview
                        println!(
                            "{}",
                            format!("Rollback plan for transaction {}", txid).bold()
                        );
                        if !plan.downgrades.is_empty() {
                            println!("\n  {} Downgrades:", "→".cyan());
                            for (pkg, ver, path) in &plan.downgrades {
                                println!("    {} → {} ({})", pkg.yellow(), ver, path.display());
                            }
                        }
                        if !plan.reinstalls.is_empty() {
                            println!("\n  {} Reinstalls:", "→".cyan());
                            for (pkg, ver, path) in &plan.reinstalls {
                                println!("    {} → {} ({})", pkg.yellow(), ver, path.display());
                            }
                        }
                        if !plan.removals.is_empty() {
                            println!("\n  {} Removals:", "→".cyan());
                            for pkg in &plan.removals {
                                println!("    {}", pkg.red());
                            }
                        }
                        if !plan.unavailable.is_empty() {
                            println!("\n  {} Unavailable (will be skipped):", "⚠".yellow());
                            for (pkg, reason) in &plan.unavailable {
                                println!("    {} - {}", pkg.dimmed(), reason);
                            }
                        }

                        // Show analysis warnings
                        if plan.has_warnings() {
                            let critical: Vec<_> = plan
                                .analysis
                                .warnings
                                .iter()
                                .filter(|w| w.is_critical())
                                .collect();
                            let advisories: Vec<_> = plan
                                .analysis
                                .warnings
                                .iter()
                                .filter(|w| !w.is_critical())
                                .collect();

                            if !critical.is_empty() {
                                println!(
                                    "\n  {} Critical Issues (high risk of system breakage):",
                                    "✗".red()
                                );
                                for warning in critical {
                                    println!("    {} {}", "•".red(), warning.description().red());
                                }
                            }

                            if !advisories.is_empty() {
                                println!("\n  {} Advisory Warnings:", "⚠".yellow());
                                for warning in advisories {
                                    println!("    {} {}", "•".yellow(), warning.description());
                                }
                            }

                            // Show provider changes if any
                            if !plan.analysis.provider_changes.is_empty() {
                                println!("\n  {} Provider Changes:", "ℹ".cyan());
                                for change in &plan.analysis.provider_changes {
                                    println!(
                                        "    {} provides {} (may affect dependents)",
                                        change.old_provider.cyan(),
                                        change.capability
                                    );
                                }
                            }
                        }

                        // Warn if there are critical issues
                        if plan.has_critical_warnings() {
                            println!(
                                "\n{}",
                                "Warning: This rollback has critical issues that may break your system."
                                    .red()
                                    .bold()
                            );
                        }

                        // Confirm unless -y flag
                        if !yes {
                            print!("\nProceed with rollback? [y/N] ");
                            use std::io::{self, Write};
                            let _ = io::stdout().flush();
                            let mut input = String::new();
                            // Treat an unreadable stdin (closed pipe, non-interactive
                            // context) as a decline rather than crashing.
                            if io::stdin().read_line(&mut input).is_err()
                                || !input.trim().eq_ignore_ascii_case("y")
                            {
                                println!("[rollback] Aborted.");
                                return;
                            }
                        }

                        // Execute rollback
                        println!("\n[rollback] Executing rollback...");
                        let result = transaction::execute_rollback(&plan);

                        // Report results
                        if !result.packages_restored.is_empty() {
                            println!("\n  {} Successfully restored:", "✓".green());
                            for pkg in &result.packages_restored {
                                println!("    {}", pkg.green());
                            }
                        }
                        if !result.packages_removed.is_empty() {
                            println!("\n  {} Successfully removed:", "✓".green());
                            for pkg in &result.packages_removed {
                                println!("    {}", pkg.green());
                            }
                        }
                        if !result.packages_failed.is_empty() {
                            println!("\n  {} Failed:", "✗".red());
                            for (pkg, reason) in &result.packages_failed {
                                println!("    {} - {}", pkg.red(), reason);
                            }
                        }
                        if !result.verification_passed {
                            println!(
                                "\n  {} {}",
                                "⚠".yellow(),
                                "Verification check did not pass".yellow()
                            );
                        }

                        // Record the rollback attempt (success, partial, or failure)
                        if let Err(e) =
                            transaction::record_rollback_attempt(&journal, &mut record, &result)
                        {
                            eprintln!(
                                "[rollback] Warning: failed to update transaction status: {}",
                                e
                            );
                        }

                        // Report final status
                        if result.packages_failed.is_empty() && result.verification_passed {
                            println!(
                                "\n{}",
                                "[rollback] Rollback completed successfully.".green()
                            );
                        } else {
                            println!(
                                "\n{}",
                                "[rollback] Rollback completed with issues.".yellow()
                            );
                            if !result.packages_failed.is_empty() {
                                println!(
                                    "{}",
                                    "Note: System may be in a partially rolled back state."
                                        .yellow()
                                );
                                println!(
                                    "{}",
                                    "Some packages may need manual intervention.".yellow()
                                );
                            }
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("[rollback] {}", e);
                        eprintln!(
                            "[rollback] Run 'reap rollback list' to see available transactions."
                        );
                        std::process::exit(1);
                    }
                },
            }
        }
        Commands::SyncDb => {
            if !core::handle_sync_db() {
                std::process::exit(1);
            }
        }
        Commands::Pin { pkg } => {
            if let Err(e) = reap::utils::pin_package(&pkg) {
                eprintln!("[reap] Pin failed: {}", e);
            } else {
                println!("[reap] Pinned {}", pkg);
            }
        }
        Commands::Tui => {
            let _config = config::Config::load();
            if let Err(e) = reap::tui::launch_tui().await {
                eprintln!("[tui] Terminal error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Profile { cmd } => {
            let profile_manager = profiles::ProfileManager::new();
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
            }
        }
        Commands::Trust { cmd } => {
            let trust_engine = trust::TrustEngine::new();
            match cmd {
                cli::TrustCmd::Score { pkg } => {
                    let source =
                        core::detect_source(&pkg, None, false).unwrap_or(core::Source::Aur);
                    let trust_score = trust_engine.compute_trust_score(&pkg, &source).await;
                    let score_badge = trust_engine.display_trust_badge(trust_score.overall_score);
                    let sig_badge = trust_engine.display_signature_badge(&trust_score);
                    let pub_status =
                        trust_engine.display_publisher_status(&trust_score.publisher_status);

                    println!("[trust] Package: {}", pkg);
                    println!("[trust] Source: {:?}", source);
                    println!(
                        "[trust] Overall: {} (Score: {:.1}/10)",
                        score_badge, trust_score.overall_score
                    );
                    println!("[trust] Signature: {}", sig_badge);
                    println!("[trust] Publisher: {}", pub_status);

                    if !trust_score.security_flags.is_empty() {
                        println!("[trust] Security flags:");
                        for flag in &trust_score.security_flags {
                            println!("[trust]   ⚠️ {:?}", flag);
                        }
                    }

                    // Show cache info
                    if let Some(audit_date) = trust_score.last_audit_date {
                        println!(
                            "[trust] Last checked: {}",
                            audit_date.format("%Y-%m-%d %H:%M UTC")
                        );
                    }
                }
                cli::TrustCmd::Scan => {
                    println!("[trust] Scanning all installed AUR packages...\n");
                    let installed = core::get_installed_packages();

                    let aur_packages: Vec<_> = installed
                        .iter()
                        .filter(|(_, source)| matches!(source, core::Source::Aur))
                        .collect();

                    if aur_packages.is_empty() {
                        println!("[trust] No AUR packages installed.");
                        return;
                    }

                    println!(
                        "[trust] Found {} AUR packages to scan\n",
                        aur_packages.len()
                    );

                    let mut scores: Vec<(String, trust::TrustScore)> = Vec::new();
                    for (pkg, source) in &aur_packages {
                        let score = trust_engine.compute_trust_score(pkg, source).await;
                        scores.push((pkg.to_string(), score));
                    }

                    // Sort by score (lowest first - most concerning). Use total
                    // ordering so a NaN score can never panic the sort.
                    scores.sort_by(|a, b| a.1.overall_score.total_cmp(&b.1.overall_score));

                    println!("{:<30} {:>8} {:>8} Flags", "Package", "Score", "Votes");
                    println!("{}", "-".repeat(70));

                    for (pkg, score) in &scores {
                        let badge = trust_engine.display_trust_badge(score.overall_score);
                        let flags = if score.security_flags.is_empty() {
                            "✓".to_string()
                        } else {
                            format!("{} flags", score.security_flags.len())
                        };
                        println!(
                            "{:<30} {:>6.1} {} {:>6} {}",
                            pkg, score.overall_score, badge, score.community_votes, flags
                        );
                    }

                    // Summary
                    let avg_score: f32 = scores.iter().map(|(_, s)| s.overall_score).sum::<f32>()
                        / scores.len() as f32;
                    let low_trust = scores.iter().filter(|(_, s)| s.overall_score < 5.0).count();

                    println!("\n{}", "=".repeat(70));
                    println!("Average trust score: {:.1}/10", avg_score);
                    println!("Low trust packages (<5.0): {}", low_trust);

                    // Advisory notice
                    println!();
                    println!("⚠️  Note: Trust scores are advisory only.");
                    println!(
                        "   Scores reflect heuristic signals (votes, maintainer reputation, PKGBUILD analysis)"
                    );
                    println!(
                        "   not cryptographic verification. Review PKGBUILDs before installing."
                    );
                }
                cli::TrustCmd::Stats => {
                    println!("[trust] Computing trust statistics...\n");
                    let installed = core::get_installed_packages();

                    let aur_packages: Vec<_> = installed
                        .iter()
                        .filter(|(_, source)| matches!(source, core::Source::Aur))
                        .collect();

                    if aur_packages.is_empty() {
                        println!("[trust] No AUR packages installed.");
                        return;
                    }

                    let mut total_score = 0.0f32;
                    let mut total_votes = 0u32;
                    let mut flag_counts: std::collections::HashMap<String, usize> =
                        std::collections::HashMap::new();
                    let mut score_buckets = [0usize; 5]; // 0-2, 2-4, 4-6, 6-8, 8-10

                    for (pkg, source) in &aur_packages {
                        let score = trust_engine.compute_trust_score(pkg, source).await;
                        total_score += score.overall_score;
                        total_votes += score.community_votes;

                        // Bucket scores
                        let bucket = ((score.overall_score / 2.0).floor() as usize).min(4);
                        score_buckets[bucket] += 1;

                        // Count flags
                        for flag in &score.security_flags {
                            *flag_counts.entry(format!("{:?}", flag)).or_insert(0) += 1;
                        }
                    }

                    let count = aur_packages.len();
                    let avg_score = total_score / count as f32;

                    println!("📊 Trust Statistics");
                    println!("{}", "=".repeat(50));
                    println!("  Total AUR packages: {}", count);
                    println!("  Average trust score: {:.1}/10", avg_score);
                    println!("  Total community votes: {}", total_votes);

                    println!("\n📈 Score Distribution:");
                    println!(
                        "  0-2 (Critical):  {} {}",
                        score_buckets[0],
                        "█".repeat(score_buckets[0])
                    );
                    println!(
                        "  2-4 (Low):       {} {}",
                        score_buckets[1],
                        "█".repeat(score_buckets[1])
                    );
                    println!(
                        "  4-6 (Medium):    {} {}",
                        score_buckets[2],
                        "█".repeat(score_buckets[2])
                    );
                    println!(
                        "  6-8 (Good):      {} {}",
                        score_buckets[3],
                        "█".repeat(score_buckets[3])
                    );
                    println!(
                        "  8-10 (Excellent):{} {}",
                        score_buckets[4],
                        "█".repeat(score_buckets[4])
                    );

                    if !flag_counts.is_empty() {
                        println!("\n⚠️  Security Flags:");
                        let mut flags: Vec<_> = flag_counts.iter().collect();
                        flags.sort_by(|a, b| b.1.cmp(a.1));
                        for (flag, count) in flags {
                            println!("  {}: {}", flag, count);
                        }
                    }

                    // Cache statistics
                    let cache_dir = reap::paths::trust_dir();
                    let mut cached_count = 0usize;
                    let mut stale_count = 0usize;
                    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                        for entry in entries.flatten() {
                            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                                cached_count += 1;
                                // Check if stale
                                if let Ok(content) = std::fs::read_to_string(entry.path())
                                    && let Ok(cached) =
                                        serde_json::from_str::<trust::TrustScore>(&content)
                                    && !trust_engine.is_cache_fresh(&cached)
                                {
                                    stale_count += 1;
                                }
                            }
                        }
                    }
                    println!("\n📦 Cache Statistics:");
                    println!("  Cached scores: {}", cached_count);
                    println!("  Stale entries: {}", stale_count);
                    println!(
                        "  Fresh entries: {}",
                        cached_count.saturating_sub(stale_count)
                    );
                }
                cli::TrustCmd::Update => {
                    println!("[trust] Refreshing trust scores for installed AUR packages...\n");
                    let installed = core::get_installed_packages();
                    let aur_packages: Vec<_> = installed
                        .iter()
                        .filter(|(_, source)| matches!(source, core::Source::Aur))
                        .collect();

                    if aur_packages.is_empty() {
                        println!("[trust] No AUR packages installed.");
                        return;
                    }

                    let mut refreshed = 0usize;
                    let mut low_trust = Vec::new();
                    for (pkg, source) in &aur_packages {
                        // Invalidate first: compute_trust_score returns the cached
                        // score early when present, so a stale entry would not refresh.
                        trust_engine.invalidate_cache(pkg);
                        let score = trust_engine.compute_trust_score(pkg, source).await;
                        refreshed += 1;
                        if score.overall_score < 5.0 {
                            low_trust.push((pkg.to_string(), score.overall_score));
                        }
                        println!("  ✓ {} ({:.1}/10)", pkg, score.overall_score);
                    }

                    println!("\n[trust] Refreshed {} package(s).", refreshed);
                    if !low_trust.is_empty() {
                        println!(
                            "[trust] ⚠️  {} package(s) below trust threshold:",
                            low_trust.len()
                        );
                        for (pkg, score) in &low_trust {
                            println!("  {} ({:.1}/10)", pkg, score);
                        }
                    }
                }
            }
        }
        Commands::Install {
            pkg,
            repo: _,
            binary_only: _,
            diff,
            fast,
            strict,
            dry_run,
            noconfirm,
            skipreview,
            insecure,
        } => {
            // Auto-sync enabled taps before resolving packages (interval-gated;
            // respects the auto_sync setting, verifies commits in advisory mode).
            let _ = tap::sync_enabled_taps();

            let config = std::sync::Arc::new(config::Config::load());
            let log = std::sync::Arc::new(tui::LogPane::default());
            let skip_confirm = noconfirm || config.noconfirm;
            let mut reviews = Vec::new();

            if fast {
                eprintln!("[perf] Fast mode enabled: optional preflight checks may be skipped");
            }

            if strict {
                eprintln!(
                    "[security] Strict mode enabled: trusted signatures are required where available"
                );
            }

            if insecure {
                eprintln!(
                    "[security] ⚠️  Running with --insecure: signature verification will be skipped"
                );
            }

            if diff {
                // Show PKGBUILD diff before install
                core::show_pkgbuild_diff(&pkg);

                if !dry_run
                    && !skip_confirm
                    && !interactive::InteractiveManager::confirm_action(
                        "Continue with installation?",
                        true,
                    )
                {
                    return;
                }
            }

            // Use priority-based install with insecure flag
            let options = core::InstallOptions {
                insecure,
                fast_mode: fast,
                strict_signatures: strict || config.security.strict_mode,
                ..Default::default()
            };

            if config.auto_resolve_deps || dry_run {
                match install_plan::create_install_plan(std::slice::from_ref(&pkg), &config).await {
                    Ok(plan) => {
                        plan.print_summary();
                        if plan.is_blocked() {
                            eprintln!(
                                "[plan] Install plan has unresolved dependencies or conflicts."
                            );
                            std::process::exit(1);
                        }
                        // Always run the structured review so the infostealer
                        // hard-block is enforced even under --skipreview; only
                        // the verbose findings and the high-risk prompt are
                        // gated by --skipreview.
                        reviews = pkgbuild_review::review_plan(&plan);
                        if !skipreview {
                            pkgbuild_review::print_reviews(&reviews);
                        }
                        if enforce_infostealer_block(&reviews, insecure) {
                            std::process::exit(2);
                        }
                        if !skipreview
                            && !dry_run
                            && pkgbuild_review::has_high_risk_review(&reviews)
                            && !skip_confirm
                            && !interactive::InteractiveManager::confirm_action(
                                "High-risk PKGBUILD findings detected. Continue?",
                                false,
                            )
                        {
                            return;
                        }
                        if dry_run {
                            return;
                        }
                        if !skip_confirm
                            && !interactive::InteractiveManager::confirm_action(
                                "Proceed with planned installation?",
                                true,
                            )
                        {
                            return;
                        }
                        for dep in plan.aur_dependency_steps() {
                            let mut dep_options = options.clone();
                            dep_options.asdeps = true;
                            println!("[plan] Installing AUR dependency {}...", dep.name);
                            if let Err(e) = core::install_with_priority(
                                &dep.name,
                                config.clone(),
                                true,
                                log.clone(),
                                &dep_options,
                            )
                            .await
                            {
                                eprintln!(
                                    "[plan] Failed to install AUR dependency {}: {}",
                                    dep.name, e
                                );
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[plan] Failed to create install plan: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            if dry_run {
                return;
            }

            if fast {
                eprintln!("[backup] Skipping pre-install backup due to --fast");
            } else {
                // Backup package state before install
                if let Err(e) = core::backup_package_state(&pkg) {
                    eprintln!("[backup] Warning: Failed to backup package state: {}", e);
                }
            }

            match core::install_with_priority(&pkg, config, true, log, &options).await {
                Ok(()) => {
                    if let Err(e) = pkgbuild_review::record_review_baselines(&reviews) {
                        eprintln!(
                            "[review] Warning: failed to record PKGBUILD review state: {}",
                            e
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[install] Failed to install {}: {}", pkg, e);
                    std::process::exit(1);
                }
            }
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
                    if interactive.confirm_pkgbuild_edit(&pkg)
                        && let Err(e) = aur_manager.edit_pkgbuild(&pkg)
                    {
                        eprintln!("[aur] Failed to edit PKGBUILD: {}", e);
                    }
                }
                cli::AurCmd::Deps { pkg, conflicts: _ } => {
                    match aur_manager
                        .resolve_dependencies_advanced(std::slice::from_ref(&pkg))
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
        Commands::BatchInstall {
            pkgs,
            parallel,
            dry_run,
            noconfirm,
            skipreview,
        } => {
            let config = std::sync::Arc::new(config::Config::load());
            let log = std::sync::Arc::new(tui::LogPane::default());
            let skip_confirm = noconfirm || config.noconfirm;
            let mut reviews = Vec::new();

            if config.auto_resolve_deps || dry_run {
                match install_plan::create_install_plan(&pkgs, &config).await {
                    Ok(plan) => {
                        plan.print_summary();
                        if plan.is_blocked() {
                            eprintln!(
                                "[plan] Batch install plan has unresolved dependencies or conflicts."
                            );
                            std::process::exit(1);
                        }
                        // Always review so the infostealer hard-block applies
                        // even under --skipreview. Batch install has no
                        // --insecure flag, so the block cannot be overridden.
                        reviews = pkgbuild_review::review_plan(&plan);
                        if !skipreview {
                            pkgbuild_review::print_reviews(&reviews);
                        }
                        if enforce_infostealer_block(&reviews, false) {
                            std::process::exit(2);
                        }
                        if !skipreview
                            && !dry_run
                            && pkgbuild_review::has_high_risk_review(&reviews)
                            && !skip_confirm
                            && !interactive::InteractiveManager::confirm_action(
                                "High-risk PKGBUILD findings detected. Continue?",
                                false,
                            )
                        {
                            return;
                        }
                        if dry_run {
                            return;
                        }
                        if !skip_confirm
                            && !interactive::InteractiveManager::confirm_action(
                                "Proceed with planned batch installation?",
                                true,
                            )
                        {
                            return;
                        }
                        for dep in plan.aur_dependency_steps() {
                            let dep_options = core::InstallOptions {
                                asdeps: true,
                                ..Default::default()
                            };
                            println!("[plan] Installing AUR dependency {}...", dep.name);
                            if let Err(e) = core::install_with_priority(
                                &dep.name,
                                config.clone(),
                                true,
                                log.clone(),
                                &dep_options,
                            )
                            .await
                            {
                                eprintln!(
                                    "[plan] Failed to install AUR dependency {}: {}",
                                    dep.name, e
                                );
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[plan] Failed to create batch install plan: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            let failures = if parallel {
                log.push(&format!(
                    "[batch] Installing {} packages in parallel",
                    pkgs.len()
                ));
                core::parallel_install(&pkgs, config, log).await
            } else {
                let mut failures = Vec::new();
                for pkg in pkgs {
                    log.push(&format!("[batch] Installing {}", pkg));
                    let options = core::InstallOptions::default();
                    if let Err(e) = core::install_with_priority(
                        &pkg,
                        config.clone(),
                        true,
                        log.clone(),
                        &options,
                    )
                    .await
                    {
                        eprintln!("[batch] Failed to install {}: {}", pkg, e);
                        failures.push(pkg);
                    }
                }
                failures
            };
            if failures.is_empty() {
                if let Err(e) = pkgbuild_review::record_review_baselines(&reviews) {
                    eprintln!(
                        "[review] Warning: failed to record PKGBUILD review state: {}",
                        e
                    );
                }
            } else {
                eprintln!(
                    "[batch] {} package(s) failed: {:?}",
                    failures.len(),
                    failures
                );
                std::process::exit(1);
            }
        }
        Commands::Remove { pkgs } => {
            let interactive = interactive::InteractiveManager::new();
            if interactive.confirm_removal(&pkgs) && !core::handle_removal(&pkgs) {
                std::process::exit(1);
            }
        }
        Commands::Local { pkgs } => {
            if !core::handle_local_install(&pkgs) {
                std::process::exit(1);
            }
        }
        Commands::Search { terms } => {
            core::handle_search(&terms);
        }
        Commands::Update => {
            core::handle_update();
        }
        Commands::Upgrade { parallel } => {
            let _ = tap::sync_enabled_taps();
            if !core::handle_upgrade(parallel) {
                std::process::exit(1);
            }
        }
        Commands::ParallelUpgrade { pkgs } => {
            let _ = tap::sync_enabled_taps();
            let config = std::sync::Arc::new(config::Config::load());
            let log = std::sync::Arc::new(tui::LogPane::default());

            log.push(&format!(
                "[parallel] Upgrading {} packages in parallel",
                pkgs.len()
            ));
            let failures = core::parallel_upgrade(&pkgs, config, log).await;
            if !failures.is_empty() {
                eprintln!(
                    "[parallel] {} upgrade(s) failed: {:?}",
                    failures.len(),
                    failures
                );
                std::process::exit(1);
            }
        }
        Commands::UpgradeAll => {
            let _ = tap::sync_enabled_taps();
            if !core::handle_upgrade_all() {
                std::process::exit(1);
            }
        }
        Commands::FlatpakUpgrade => {
            if !flatpak::is_available() {
                eprintln!("[flatpak] Flatpak is not installed on this system");
                std::process::exit(1);
            }
            if let Err(e) = flatpak::upgrade() {
                eprintln!("[flatpak] Upgrade failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Clean => {
            let mut ok = core::handle_clean();
            // Also clean cache using utils
            match utils::clean_cache() {
                Ok(msg) => println!("[clean] {}", msg),
                Err(e) => {
                    eprintln!("[clean] Error: {}", e);
                    ok = false;
                }
            }
            if !ok {
                std::process::exit(1);
            }
        }
        Commands::Doctor => {
            core::handle_doctor();
        }
        Commands::Perf { cmd } => match cmd {
            cli::PerfCmd::WarmCache => {
                println!("[perf] Warming cache with popular packages...");
                if let Err(e) = aur::warm_cache().await {
                    eprintln!("[perf] Cache warming failed: {}", e);
                }
            }
            cli::PerfCmd::ParallelSearch { queries } => {
                println!(
                    "[perf] Running parallel search for {} queries",
                    queries.len()
                );
                match aur::parallel_search(&queries).await {
                    Ok(results) => println!("[perf] Found {} total results", results.len()),
                    Err(e) => eprintln!("[perf] Parallel search failed: {}", e),
                }
            }
            cli::PerfCmd::ParallelFetch { packages } => {
                println!(
                    "[perf] Running parallel PKGBUILD fetch for {} packages",
                    packages.len()
                );
                match aur::parallel_pkgbuild_fetch(&packages).await {
                    Ok(downloads) => println!(
                        "[perf] Successfully downloaded {} PKGBUILDs",
                        downloads.len()
                    ),
                    Err(e) => eprintln!("[perf] Parallel fetch failed: {}", e),
                }
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
                // Accept either an AUR package name or a local PKGBUILD path.
                let (label, pkgbuild, install_hook) = if std::path::Path::new(&pkg).is_file() {
                    let contents = std::fs::read_to_string(&pkg).unwrap_or_default();
                    (pkg.clone(), contents, String::new())
                } else {
                    let pkgbuild = aur::get_pkgbuild_preview(&pkg);
                    let hook = aur::get_install_file_preview(&pkg, &pkgbuild);
                    (pkg.clone(), pkgbuild, hook)
                };
                println!("[security] Auditing: {}", label);
                if !install_hook.is_empty() {
                    println!("[security] Including .install hook in scan");
                }

                let report = audit::audit(&pkgbuild, &install_hook);
                if report.findings.is_empty() {
                    println!("✅ {} passed security audit", label);
                } else {
                    println!("⚠️ {} security audit findings:", label);
                    for finding in &report.findings {
                        println!("  {}", finding.describe());
                    }
                }
                println!("🛡️ Security risk score: {}", report.risk_score);
                println!(
                    "🕵️ Infostealer confidence: {}",
                    report.infostealer_confidence.label()
                );
                if report.is_infostealer_block() {
                    println!(
                        "⛔ This package would be BLOCKED on install by default (override: --insecure)"
                    );
                }
            }
            cli::SecurityCmd::ScanAll => {
                println!("[security] Scanning all installed AUR packages...\n");
                let installed = core::get_installed_packages();
                let mut total_risk = 0;
                let mut scanned_count = 0;
                let mut risky_packages = Vec::new();

                let aur_packages: Vec<_> = installed
                    .iter()
                    .filter(|(_, source)| matches!(source, core::Source::Aur))
                    .collect();

                if aur_packages.is_empty() {
                    println!("[security] No AUR packages installed.");
                    return;
                }

                println!(
                    "[security] Found {} AUR packages to scan",
                    aur_packages.len()
                );

                let mut infostealer_packages = Vec::new();
                for (pkg, _source) in &aur_packages {
                    let pkgbuild = aur::get_pkgbuild_preview(pkg);
                    let install_hook = aur::get_install_file_preview(pkg, &pkgbuild);
                    let report = audit::audit(&pkgbuild, &install_hook);
                    scanned_count += 1;

                    if report.is_infostealer_block() {
                        infostealer_packages.push(pkg.to_string());
                    }
                    if report.risk_score > 15 {
                        risky_packages.push((
                            pkg.to_string(),
                            report.risk_score,
                            report.warnings(),
                        ));
                    }
                    total_risk += report.risk_score;
                }

                println!("\n🛡️ Security Scan Results");
                println!("{}", "=".repeat(50));
                println!("  Packages scanned: {}", scanned_count);
                println!("  Total risk score: {}", total_risk);
                println!(
                    "  Average risk: {:.1}",
                    if scanned_count > 0 {
                        total_risk as f64 / scanned_count as f64
                    } else {
                        0.0
                    }
                );
                println!("  High-risk packages: {}", risky_packages.len());
                println!(
                    "  Infostealer-flagged packages: {}",
                    infostealer_packages.len()
                );

                if !infostealer_packages.is_empty() {
                    println!("\n⛔ High-confidence infostealer behavior detected:");
                    println!("{}", "-".repeat(50));
                    for pkg in &infostealer_packages {
                        println!("  • {}", pkg);
                    }
                }

                if !risky_packages.is_empty() {
                    println!("\n⚠️  High-Risk Packages (score > 15):");
                    println!("{}", "-".repeat(50));
                    for (pkg, score, warnings) in &risky_packages {
                        println!("\n  {} (risk score: {})", pkg, score);
                        for warning in warnings.iter().take(3) {
                            println!("    • {}", warning);
                        }
                        if warnings.len() > 3 {
                            println!("    • ... and {} more warnings", warnings.len() - 3);
                        }
                    }
                } else {
                    println!("\n✅ No high-risk packages detected");
                }
            }
            cli::SecurityCmd::Stats => {
                let counts = audit::pattern_counts();
                println!("[security] Detection rules (built-in):");
                println!("  Risky-command patterns: {}", counts.risky);
                println!("  Supply-chain patterns: {}", counts.supply_chain);
                println!("  Suspicious domains: {}", counts.domains);
                println!("  Credential patterns: {}", counts.credentials);
                println!(
                    "  Infostealer sensitive-path indicators: {}",
                    counts.infostealer_sensitive
                );
                println!("  Exfiltration indicators: {}", counts.exfil);
                println!("  Source-integrity rules: {}", counts.source_rules);
                println!("  Total: {}", counts.total());
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
                if let Err(e) = gpg::import_gpg_key_async(&keyid).await {
                    eprintln!("[reap] Failed to import GPG key: {}", e);
                }
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
            cli::GpgCmd::CheckKeyserver { url } => {
                println!("Checking GPG keyserver: {}", url);
                utils::check_keyserver_async(&url).await;
            }
        },
        Commands::Flatpak { cmd } => {
            // Check if Flatpak is available first
            if !flatpak::is_available() {
                eprintln!("[flatpak] Flatpak is not installed on this system");
                eprintln!("[flatpak] Install with: sudo pacman -S flatpak");
                std::process::exit(1);
            }

            // Show Flatpak version for verbose operations
            if let Some(ver) = flatpak::version()
                && matches!(
                    cmd,
                    cli::FlatpakCmd::List | cli::FlatpakCmd::Remotes | cli::FlatpakCmd::Upgrade
                )
            {
                println!("[flatpak] Using {}", ver);
            }

            match cmd {
                cli::FlatpakCmd::Search { query } => match flatpak::search(&query) {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("[flatpak] No results found for: {}", query);
                        } else {
                            println!("[flatpak] Found {} results:\n", results.len());
                            for r in results {
                                println!("flatpak/{} {}\n    {}", r.name, r.version, r.description);
                            }
                        }
                    }
                    Err(e) => eprintln!("[flatpak] Search failed: {}", e),
                },
                cli::FlatpakCmd::Install {
                    app_id,
                    setup_flathub,
                } => {
                    if setup_flathub && let Err(e) = flatpak::ensure_flathub() {
                        eprintln!("[flatpak] Warning: Could not setup Flathub: {}", e);
                    }
                    if let Err(e) = flatpak::install(&app_id) {
                        eprintln!("[flatpak] Install failed: {}", e);
                        std::process::exit(1);
                    }
                }
                cli::FlatpakCmd::Remove { app_id } => {
                    if let Err(e) = flatpak::remove(&app_id) {
                        eprintln!("[flatpak] Remove failed: {}", e);
                        std::process::exit(1);
                    }
                }
                cli::FlatpakCmd::Update => {
                    if let Err(e) = flatpak::update_metadata() {
                        eprintln!("[flatpak] Metadata update failed: {}", e);
                        std::process::exit(1);
                    }
                }
                cli::FlatpakCmd::List => match flatpak::list() {
                    Ok(apps) => flatpak::display_list(&apps),
                    Err(e) => {
                        eprintln!("[flatpak] List failed: {}", e);
                        std::process::exit(1);
                    }
                },
                cli::FlatpakCmd::Upgrade => {
                    if let Err(e) = flatpak::upgrade() {
                        eprintln!("[flatpak] Upgrade failed: {}", e);
                        std::process::exit(1);
                    }
                }
                cli::FlatpakCmd::Info { app_id } => match flatpak::info(&app_id) {
                    Ok(info) => flatpak::display_info(&info),
                    Err(e) => {
                        eprintln!("[flatpak] Info failed: {}", e);
                        std::process::exit(1);
                    }
                },
                cli::FlatpakCmd::Audit { app_id } => match flatpak::audit(&app_id) {
                    Ok(audit) => flatpak::display_audit(&audit),
                    Err(e) => {
                        eprintln!("[flatpak] Audit failed: {}", e);
                        std::process::exit(1);
                    }
                },
                cli::FlatpakCmd::Remotes => match flatpak::list_remotes() {
                    Ok(remotes) => {
                        if remotes.is_empty() {
                            println!("[flatpak] No remotes configured");
                            println!(
                                "[flatpak] Add Flathub with: reap flatpak install --setup-flathub <app>"
                            );
                        } else {
                            println!("Configured Flatpak remotes:\n");
                            println!("{:<20} URL", "Name");
                            println!("{}", "-".repeat(60));
                            for (name, url) in remotes {
                                println!("{:<20} {}", name, url);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[flatpak] Failed to list remotes: {}", e);
                        std::process::exit(1);
                    }
                },
                cli::FlatpakCmd::CheckUpdates => match flatpak::check_updates() {
                    Ok(updates) => {
                        if updates.is_empty() {
                            println!("[flatpak] All applications are up to date");
                        } else {
                            println!("[flatpak] {} updates available:\n", updates.len());
                            for app in updates {
                                println!("  {}", app);
                            }
                            println!("\nRun 'reap flatpak upgrade' to install updates");
                        }
                    }
                    Err(e) => {
                        eprintln!("[flatpak] Failed to check updates: {}", e);
                        std::process::exit(1);
                    }
                },
            }
        }
        Commands::Tap { cmd } => match cmd {
            cli::TapCmd::Add {
                name,
                url,
                priority,
            } => {
                reap::tap::add_or_update_tap(&name, &url, Some(priority as u8), true);
            }
            cli::TapCmd::Remove { name } => {
                reap::tap::remove_tap(&name);
            }
            cli::TapCmd::Enable { name } => {
                reap::tap::set_tap_enabled(&name, true);
            }
            cli::TapCmd::Disable { name } => {
                reap::tap::set_tap_enabled(&name, false);
            }
            cli::TapCmd::Update => {
                reap::tap::sync_taps();
                // Invalidate trust cache for all synced taps
                let trust_engine = trust::TrustEngine::new();
                for tap in reap::tap::discover_taps() {
                    if tap.enabled {
                        trust_engine.invalidate_tap_cache(&tap.name);
                    }
                }
            }
            cli::TapCmd::Sync => {
                reap::tap::sync_taps();
                // Invalidate trust cache for all synced taps
                let trust_engine = trust::TrustEngine::new();
                for tap in reap::tap::discover_taps() {
                    if tap.enabled {
                        trust_engine.invalidate_tap_cache(&tap.name);
                    }
                }
            }
            cli::TapCmd::List => {
                reap::tap::list_taps();
            }
        },
        Commands::Completion { shell } => {
            utils::completion(&shell);
        }
        Commands::Backup => match utils::backup_config() {
            Ok(_) => println!("[reap] Config backup complete."),
            Err(e) => {
                eprintln!("[reap] Config backup failed: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Orphan { remove, all } => {
            core::handle_orphan(remove, all);
        }
        Commands::Config { cmd } => match cmd {
            cli::ConfigCmd::Show => {
                config::show_config();
            }
            cli::ConfigCmd::Set { key, value } => {
                reap::config::set_config_key(&key, &value);
            }
            cli::ConfigCmd::Get { key } => {
                if let Some(val) = reap::config::get_config_key(&key) {
                    println!("{} = {}", key, val);
                } else {
                    println!("Key '{}' not found in config.", key);
                }
            }
            cli::ConfigCmd::Reset => {
                reap::config::reset_config();
            }
        },
        Commands::Dpkg { cmd } => match cmd {
            cli::DpkgCmd::Info { path } => {
                let deb_path = std::path::Path::new(&path);
                match dpkg::info(deb_path) {
                    Ok(info) => dpkg::display_info(&info),
                    Err(e) => {
                        eprintln!("[dpkg] Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            cli::DpkgCmd::Install { path } => {
                let deb_path = std::path::Path::new(&path);
                if let Err(e) = dpkg::install(deb_path) {
                    eprintln!("[dpkg] Error: {}", e);
                    std::process::exit(1);
                }
            }
        },
    }
}

/// Handle pacman-style flag combinations (e.g., -Syu, -Ss, -R)
/// Dispatch a pacman-style action. Returns `true` on success, `false` if any
/// underlying operation failed (so the caller can set a nonzero exit code).
async fn handle_pacman_action(action: PacmanAction) -> bool {
    match action {
        PacmanAction::SyncUpgrade => {
            // -Syu: sync database and upgrade all
            println!("[reap] Syncing databases and upgrading...");
            if !core::handle_sync_db() {
                return false;
            }
            if let Err(e) = aur::upgrade_all().await {
                eprintln!("[reap] Upgrade failed: {}", e);
                return false;
            }
            true
        }
        PacmanAction::RefreshDb => {
            // -Sy: sync database
            println!("[reap] Syncing package databases...");
            core::handle_sync_db()
        }
        PacmanAction::Upgrade => {
            // -Su: upgrade all
            println!("[reap] Upgrading all packages...");
            if let Err(e) = aur::upgrade_all().await {
                eprintln!("[reap] Upgrade failed: {}", e);
                return false;
            }
            true
        }
        PacmanAction::Install(pkgs) => {
            // -S <pkgs>: install packages
            let config = std::sync::Arc::new(config::Config::load());
            let log = std::sync::Arc::new(tui::LogPane::default());

            let mut all_ok = true;
            for pkg in pkgs {
                println!("[reap] Installing {}...", pkg);
                let opts = core::InstallOptions::default();
                if let Err(e) =
                    core::install_with_priority(&pkg, config.clone(), false, log.clone(), &opts)
                        .await
                {
                    eprintln!("[reap] Install failed for {}: {}", pkg, e);
                    all_ok = false;
                }
            }
            all_ok
        }
        PacmanAction::Search(terms) => {
            // -Ss: search packages
            let query = terms.join(" ");
            println!("[reap] Searching for: {}", query);
            let results = core::unified_search(&query).await;
            for r in results {
                println!(
                    "{}/{} {}\n    {}",
                    r.source.label(),
                    r.name,
                    r.version,
                    r.description
                );
            }
            true
        }
        PacmanAction::Remove(pkgs) => {
            // -R: remove packages
            let pkg_refs: Vec<String> = pkgs.iter().map(|s| s.to_string()).collect();
            pacman::remove_with_options(&pkg_refs, false)
        }
        PacmanAction::QueryUpgradable => {
            // -Qu: show packages that can be upgraded
            println!("[reap] Checking for upgradable packages...");
            let outdated = aur::get_outdated();
            if outdated.is_empty() {
                println!("All packages are up to date.");
            } else {
                println!("Packages with updates available:");
                for pkg in outdated {
                    if let Some(ver) = pacman::get_version(&pkg)
                        && let Ok(remote) = aur::fetch_package_info(&pkg)
                    {
                        println!("  {} {} -> {}", pkg, ver, remote.version);
                    }
                }
            }
            true
        }
        PacmanAction::QueryInstalled(pkgs) => {
            // -Q <pkg>: check if installed
            for pkg in pkgs {
                if let Some(ver) = pacman::get_version(&pkg) {
                    println!("{} {}", pkg, ver);
                } else {
                    println!("Package '{}' is not installed", pkg);
                }
            }
            true
        }
        PacmanAction::CleanCache => {
            // -Sc: clean cache
            core::handle_clean()
        }
    }
}

/// Enforce the default infostealer hard-block.
///
/// High-confidence infostealer evidence (a sensitive-credential read correlated
/// with a network-exfiltration mechanism) stops the install by default. This is
/// the one place reaper overrides the user automatically; the override is
/// intentionally limited to `--insecure` and is NOT bypassed by `--skipreview`
/// or `--noconfirm`. Returns `true` when the caller must abort.
fn enforce_infostealer_block(reviews: &[pkgbuild_review::PkgbuildReview], insecure: bool) -> bool {
    if !pkgbuild_review::has_infostealer_block(reviews) {
        return false;
    }
    let blocked = pkgbuild_review::infostealer_blocked_packages(reviews).join(", ");
    if insecure {
        eprintln!(
            "[security] ⚠️  --insecure set: overriding infostealer block for {}",
            blocked
        );
        false
    } else {
        eprintln!(
            "[security] ⛔ BLOCKED: high-confidence infostealer behavior detected in {}",
            blocked
        );
        eprintln!(
            "[security] These PKGBUILDs read sensitive credentials and exfiltrate them over the network."
        );
        eprintln!(
            "[security] Installation refused. If you have manually verified this is a false positive, re-run with --insecure."
        );
        true
    }
}

// =============================================================================
// Rollback Display Functions
// =============================================================================

fn display_transaction_list(records: &[transaction::TransactionRecord]) {
    use owo_colors::OwoColorize;

    if records.is_empty() {
        println!("No transactions recorded yet.");
        println!("\nTransactions are recorded when you install, upgrade, or remove packages.");
        return;
    }

    println!("{}", "=== Transaction History ===".bold());
    println!();
    let header = format!(
        "{:<26} {:<12} {:<12} {:<6} {}",
        "ID", "Operation", "Status", "Pkgs", "Rollback"
    );
    println!("{}", header);
    println!("{}", "-".repeat(75));

    for record in records {
        let status = match &record.status {
            transaction::TransactionStatus::Completed => "Completed".green().to_string(),
            transaction::TransactionStatus::Failed(_) => "Failed".red().to_string(),
            transaction::TransactionStatus::InProgress => "InProgress".yellow().to_string(),
            transaction::TransactionStatus::Interrupted => "Interrupted".yellow().to_string(),
        };

        let rollback = match &record.rollback_status {
            transaction::RollbackStatus::Rollbackable => "Yes".green().to_string(),
            transaction::RollbackStatus::PartiallyRollbackable { .. } => {
                "Partial".yellow().to_string()
            }
            transaction::RollbackStatus::NotRollbackable { .. } => "No".red().to_string(),
            transaction::RollbackStatus::RolledBack { .. } => "Done".cyan().to_string(),
            transaction::RollbackStatus::Unknown => "?".dimmed().to_string(),
        };

        println!(
            "{:<26} {:<12} {:<12} {:<6} {}",
            record.id,
            record.operation.to_string(),
            status,
            record.affected_packages.len(),
            rollback
        );
    }

    println!();
    println!("Use {} to see details", "reap rollback show <txid>".cyan());
}

fn display_transaction_details(record: &transaction::TransactionRecord) {
    use owo_colors::OwoColorize;

    println!("{}", "=== Transaction Details ===".bold());
    println!();
    println!("ID:        {}", record.id.cyan());
    println!("Operation: {}", record.operation);
    println!(
        "Started:   {}",
        record.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(completed) = record.completed_at {
        println!("Completed: {}", completed.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    println!("Status:    {}", record.status);

    if let Some(profile) = &record.active_profile {
        println!("Profile:   {}", profile);
    }

    println!();
    println!("{}", "Requested packages:".bold());
    for pkg in &record.requested_packages {
        println!("  - {}", pkg);
    }

    println!();
    println!("{}", "Affected packages:".bold());
    for change in &record.affected_packages {
        let version_change = match (&change.previous_version, &change.new_version) {
            (Some(old), Some(new)) => format!("{} -> {}", old.red(), new.green()),
            (None, Some(new)) => format!("(new) -> {}", new.green()),
            (Some(old), None) => format!("{} -> (removed)", old.red()),
            (None, None) => "(unknown)".to_string(),
        };

        let artifact_status = if change
            .previous_artifact
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
        {
            "artifact available".green().to_string()
        } else if change.previous_version.is_some() {
            "artifact missing".red().to_string()
        } else {
            "n/a".dimmed().to_string()
        };

        println!("  {} ({})", change.name.bold(), change.change_type);
        println!("    Version:  {}", version_change);
        println!("    Source:   {}", change.source.label());
        println!("    Rollback: {}", artifact_status);
    }

    println!();
    println!("{}", "Rollback Status:".bold());
    match &record.rollback_status {
        transaction::RollbackStatus::Rollbackable => {
            println!("  {} All packages can be rolled back", "OK".green());
        }
        transaction::RollbackStatus::PartiallyRollbackable {
            rollbackable_count,
            not_rollbackable_count,
            reasons,
        } => {
            println!("  {} Partial rollback available", "WARN".yellow());
            println!("    {} packages can be rolled back", rollbackable_count);
            println!(
                "    {} packages cannot be rolled back:",
                not_rollbackable_count
            );
            for reason in reasons {
                println!("      - {}", reason);
            }
        }
        transaction::RollbackStatus::NotRollbackable { reasons } => {
            println!("  {} Rollback not available", "NO".red());
            for reason in reasons {
                println!("    - {}", reason);
            }
        }
        transaction::RollbackStatus::RolledBack { rolled_back_at } => {
            println!(
                "  {} Already rolled back at {}",
                "DONE".cyan(),
                rolled_back_at.format("%Y-%m-%d %H:%M:%S UTC")
            );
        }
        transaction::RollbackStatus::Unknown => {
            println!("  {} Eligibility not computed", "?".dimmed());
        }
    }
}

fn display_rollback_preview(record: &transaction::TransactionRecord) {
    use owo_colors::OwoColorize;

    println!("{}", "=== Rollback Preview (Dry Run) ===".bold());
    println!();
    println!("Transaction: {}", record.id.cyan());
    println!();

    // Check eligibility first
    match &record.rollback_status {
        transaction::RollbackStatus::NotRollbackable { reasons } => {
            println!("{} Rollback not possible:", "ERROR".red().bold());
            for reason in reasons {
                println!("  - {}", reason);
            }
            return;
        }
        transaction::RollbackStatus::RolledBack { .. } => {
            println!(
                "{} This transaction was already rolled back.",
                "INFO".cyan()
            );
            return;
        }
        _ => {}
    }

    println!("{}", "Planned actions:".bold());
    println!();

    // Drive the preview from the same plan the apply path executes, so the
    // dry-run and the real rollback never disagree (e.g. artifacts that are
    // only discoverable via the pacman cache fallback in create_rollback_plan).
    let plan = transaction::create_rollback_plan(record, &transaction::artifact_search_dirs());
    let downgrades = &plan.downgrades;
    let reinstalls = &plan.reinstalls;
    let removals = &plan.removals;
    let unavailable = &plan.unavailable;

    // Look up the post-transaction version for the downgrade arrow display.
    let new_version_of = |name: &str| -> String {
        record
            .affected_packages
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.new_version.clone())
            .unwrap_or_else(|| "?".to_string())
    };

    if !downgrades.is_empty() {
        println!("{} Packages to downgrade:", "DOWN".yellow());
        for (name, to, _artifact) in downgrades {
            let from = new_version_of(name);
            println!("    {} {} -> {}", name.bold(), from.red(), to.green());
        }
        println!();
    }

    if !reinstalls.is_empty() {
        println!("{} Packages to reinstall:", "INST".cyan());
        for (name, ver, _artifact) in reinstalls {
            println!("    {} {}", name.bold(), ver.green());
        }
        println!();
    }

    if !removals.is_empty() {
        println!("{} Packages to remove:", "DEL".red());
        for name in removals {
            println!("    {}", name.bold());
        }
        println!();
    }

    if !unavailable.is_empty() {
        println!("{} Cannot be rolled back:", "WARN".yellow());
        for (name, reason) in unavailable {
            println!("    {} - {}", name.bold(), reason);
        }
        println!();
    }

    // Show analysis warnings
    if plan.has_warnings() {
        println!("{}", "Dependency Analysis:".bold());
        println!();

        let critical: Vec<_> = plan
            .analysis
            .warnings
            .iter()
            .filter(|w| w.is_critical())
            .collect();
        let advisories: Vec<_> = plan
            .analysis
            .warnings
            .iter()
            .filter(|w| !w.is_critical())
            .collect();

        if !critical.is_empty() {
            println!(
                "{} Critical issues (high risk of breakage):",
                "CRITICAL".red()
            );
            for warning in critical {
                println!("    {} {}", "•".red(), warning.description());
            }
            println!();
        }

        if !advisories.is_empty() {
            println!("{} Advisory warnings:", "INFO".yellow());
            for warning in advisories {
                println!("    {} {}", "•".yellow(), warning.description());
            }
            println!();
        }

        if !plan.analysis.provider_changes.is_empty() {
            println!("{} Provider changes detected:", "INFO".cyan());
            for change in &plan.analysis.provider_changes {
                println!("    {} provides {}", change.old_provider, change.capability);
            }
            println!();
        }
    }

    // Summary
    println!("{}", "-".repeat(50));
    println!("Summary:");
    println!("  Downgrades:   {}", downgrades.len());
    println!("  Reinstalls:   {}", reinstalls.len());
    println!("  Removals:     {}", removals.len());
    println!("  Unavailable:  {}", unavailable.len());
    if plan.has_warnings() {
        let critical_count = plan
            .analysis
            .warnings
            .iter()
            .filter(|w| w.is_critical())
            .count();
        let advisory_count = plan.analysis.warnings.len() - critical_count;
        println!("  Critical:     {}", critical_count);
        println!("  Advisories:   {}", advisory_count);
    }

    if unavailable.is_empty() && !plan.has_critical_warnings() {
        println!();
        println!("{} All packages can be rolled back.", "OK".green());
        println!("Run {} to execute.", "reap rollback apply <txid>".cyan());
    } else if plan.has_critical_warnings() {
        println!();
        println!(
            "{} Rollback has critical issues. Proceed with caution.",
            "WARN".red()
        );
        println!(
            "Run {} to execute anyway.",
            "reap rollback apply <txid>".cyan()
        );
    } else {
        println!();
        println!(
            "{} Some packages cannot be rolled back due to missing artifacts.",
            "WARN".yellow()
        );
    }
}
