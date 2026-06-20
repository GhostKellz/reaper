use crate::config::Config;
use crate::core::{self, Source};
use crate::pacman;
use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::{HashMap, HashSet};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanReason {
    Explicit,
    DependencyOf(String),
    MakeDependencyOf(String),
    CheckDependencyOf(String),
}

impl PlanReason {
    pub fn is_explicit(&self) -> bool {
        matches!(self, PlanReason::Explicit)
    }

    pub fn label(&self) -> String {
        match self {
            PlanReason::Explicit => "explicit".to_string(),
            PlanReason::DependencyOf(pkg) => format!("dependency of {}", pkg),
            PlanReason::MakeDependencyOf(pkg) => format!("make dependency of {}", pkg),
            PlanReason::CheckDependencyOf(pkg) => format!("check dependency of {}", pkg),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanStep {
    pub name: String,
    pub source: Source,
    pub version: Option<String>,
    pub reason: PlanReason,
    pub installed_version: Option<String>,
}

impl PlanStep {
    pub fn needs_install(&self) -> bool {
        self.installed_version.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct SkippedPackage {
    pub name: String,
    pub installed_version: String,
    pub reason: PlanReason,
    pub provided_by: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallPlan {
    pub requested: Vec<String>,
    pub steps: Vec<PlanStep>,
    pub skipped: Vec<SkippedPackage>,
    pub warnings: Vec<String>,
    pub conflicts: Vec<String>,
    pub unresolved: Vec<String>,
}

impl InstallPlan {
    pub fn is_blocked(&self) -> bool {
        !self.conflicts.is_empty() || !self.unresolved.is_empty()
    }

    pub fn aur_dependency_steps(&self) -> Vec<&PlanStep> {
        self.steps
            .iter()
            .filter(|step| {
                step.needs_install()
                    && !step.reason.is_explicit()
                    && matches!(step.source, Source::Aur | Source::Custom(_))
            })
            .collect()
    }

    pub fn print_summary(&self) {
        println!();
        println!("{}", "Install plan".bold());
        println!("{}", "------------".dimmed());

        let installs = self
            .steps
            .iter()
            .filter(|step| step.needs_install())
            .count();
        let already_present = self.skipped.len();
        let aur_deps = self
            .steps
            .iter()
            .filter(|step| !step.reason.is_explicit() && matches!(step.source, Source::Aur))
            .count();

        println!(
            "Requested: {}",
            if self.requested.is_empty() {
                "(none)".dimmed().to_string()
            } else {
                self.requested.join(", ").bright_white().to_string()
            }
        );
        println!(
            "Actions: {} install/build, {} already satisfied, {} AUR dependency build(s)",
            installs.to_string().bright_green(),
            already_present.to_string().cyan(),
            aur_deps.to_string().yellow()
        );

        if !self.steps.is_empty() {
            println!();
            println!("{}", "Order:".bold());
            for (idx, step) in self.steps.iter().enumerate() {
                let source = step.source.label();
                let version = step
                    .version
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .unwrap_or("unknown");
                let state = if let Some(installed) = &step.installed_version {
                    format!("installed {}", installed).dimmed().to_string()
                } else {
                    "pending".bright_green().to_string()
                };
                println!(
                    "  {:>2}. {:<28} {:<12} {:<18} {} ({})",
                    idx + 1,
                    step.name.bright_white(),
                    source,
                    version,
                    state,
                    step.reason.label()
                );
            }
        }

        if !self.skipped.is_empty() {
            println!();
            println!("{}", "Already satisfied:".bold());
            for skipped in &self.skipped {
                println!(
                    "  {} {}{} ({})",
                    skipped.name.cyan(),
                    skipped.installed_version,
                    skipped
                        .provided_by
                        .as_ref()
                        .map(|provider| format!(" via {}", provider))
                        .unwrap_or_default(),
                    skipped.reason.label()
                );
            }
        }

        if !self.warnings.is_empty() {
            println!();
            println!("{}", "Warnings:".yellow().bold());
            for warning in &self.warnings {
                println!("  - {}", warning);
            }
        }

        if !self.conflicts.is_empty() {
            println!();
            println!("{}", "Conflicts:".red().bold());
            for conflict in &self.conflicts {
                println!("  - {}", conflict);
            }
        }

        if !self.unresolved.is_empty() {
            println!();
            println!("{}", "Unresolved:".red().bold());
            for dep in &self.unresolved {
                println!("  - {}", dep);
            }
        }

        println!();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PkgbuildDeps {
    pub version: Option<String>,
    pub depends: Vec<String>,
    pub make_depends: Vec<String>,
    pub check_depends: Vec<String>,
    pub conflicts: Vec<String>,
    pub provides: Vec<String>,
}

#[derive(Default)]
struct PlanBuilder {
    plan: InstallPlan,
    planned: HashSet<String>,
    visiting: HashSet<String>,
    installed: InstalledIndex,
}

pub async fn create_install_plan(packages: &[String], config: &Config) -> Result<InstallPlan> {
    let mut builder = PlanBuilder {
        plan: InstallPlan {
            requested: packages.to_vec(),
            ..Default::default()
        },
        installed: InstalledIndex::load(),
        ..Default::default()
    };

    for package in packages {
        resolve_package(package, PlanReason::Explicit, config, &mut builder).await?;
    }

    Ok(builder.plan)
}

async fn resolve_package<'a>(
    package: &'a str,
    reason: PlanReason,
    config: &'a Config,
    builder: &'a mut PlanBuilder,
) -> Result<()> {
    let clean_name = clean_dependency_name(package);
    if clean_name.is_empty() {
        return Ok(());
    }

    if builder.planned.contains(&clean_name) {
        return Ok(());
    }

    if !builder.visiting.insert(clean_name.clone()) {
        builder
            .plan
            .conflicts
            .push(format!("circular dependency involving {}", clean_name));
        return Ok(());
    }

    if let Some(satisfied) = builder.installed.satisfies(package) {
        builder.plan.skipped.push(SkippedPackage {
            name: clean_name.clone(),
            installed_version: satisfied.version,
            reason,
            provided_by: satisfied.provider,
        });
        builder.visiting.remove(&clean_name);
        return Ok(());
    }

    let mut provider_note = None;
    let source_resolution = core::resolve_package_source(&clean_name, None, config);
    let (source, _, _, _) = if let Some(resolved) = source_resolution {
        resolved
    } else if let Some(provider) = repo_provider_for(&clean_name) {
        provider_note = Some(provider.clone());
        (Source::Pacman, None, 20, None)
    } else {
        builder.plan.unresolved.push(clean_name.clone());
        builder.visiting.remove(&clean_name);
        return Ok(());
    };

    let mut version = None;
    let mut deps = PkgbuildDeps::default();

    if matches!(source, Source::Aur) {
        match fetch_aur_pkgbuild_deps(&clean_name).await {
            Ok(parsed) => {
                version = parsed.version.clone();
                deps = parsed;
            }
            Err(err) => builder.plan.warnings.push(format!(
                "failed to inspect AUR metadata for {}: {}",
                clean_name, err
            )),
        }
    }

    for conflict in &deps.conflicts {
        let conflict_name = clean_dependency_name(conflict);
        if let Some(installed_version) = pacman::get_version(&conflict_name) {
            builder.plan.conflicts.push(format!(
                "{} conflicts with installed {} {}",
                clean_name, conflict_name, installed_version
            ));
        }
        if builder.planned.contains(&conflict_name) {
            builder.plan.conflicts.push(format!(
                "{} conflicts with planned {}",
                clean_name, conflict_name
            ));
        }
    }

    for dep in &deps.depends {
        let dep_name = clean_dependency_name(dep);
        if should_recurse_dependency(&dep_name, config, &builder.installed) {
            Box::pin(resolve_package(
                &dep_name,
                PlanReason::DependencyOf(clean_name.clone()),
                config,
                builder,
            ))
            .await?;
        }
    }

    for dep in &deps.make_depends {
        let dep_name = clean_dependency_name(dep);
        if should_recurse_dependency(&dep_name, config, &builder.installed) {
            Box::pin(resolve_package(
                &dep_name,
                PlanReason::MakeDependencyOf(clean_name.clone()),
                config,
                builder,
            ))
            .await?;
        }
    }

    for dep in &deps.check_depends {
        let dep_name = clean_dependency_name(dep);
        if should_recurse_dependency(&dep_name, config, &builder.installed) {
            Box::pin(resolve_package(
                &dep_name,
                PlanReason::CheckDependencyOf(clean_name.clone()),
                config,
                builder,
            ))
            .await?;
        }
    }

    builder.planned.insert(clean_name.clone());
    builder.plan.steps.push(PlanStep {
        name: provider_note.unwrap_or_else(|| clean_name.clone()),
        source,
        version,
        reason,
        installed_version: None,
    });
    builder.visiting.remove(&clean_name);
    Ok(())
}

fn should_recurse_dependency(dep: &str, config: &Config, installed: &InstalledIndex) -> bool {
    if dep.is_empty() {
        return false;
    }

    if installed.satisfies(dep).is_some()
        || repo_has_package(dep)
        || repo_provider_for(dep).is_some()
    {
        return false;
    }

    config.backend_order.iter().any(|backend| backend == "aur")
}

#[derive(Debug, Clone, Default)]
struct InstalledIndex {
    packages: HashMap<String, String>,
    providers: HashMap<String, InstalledProvider>,
}

#[derive(Debug, Clone)]
struct InstalledProvider {
    package: String,
    version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SatisfiedDependency {
    version: String,
    provider: Option<String>,
}

impl InstalledIndex {
    fn load() -> Self {
        let mut index = Self::default();
        for (name, version) in pacman::list_installed_with_versions() {
            index.add_package(name, version, pacman::get_provides);
        }
        index
    }

    fn add_package<F>(&mut self, name: String, version: String, provides_for: F)
    where
        F: Fn(&str) -> Vec<String>,
    {
        self.packages.insert(name.clone(), version.clone());
        for provided in provides_for(&name) {
            let (provided_name, provided_constraint) = pacman::parse_dependency(&provided);
            let provided_version = provided_constraint
                .as_deref()
                .and_then(|constraint| constraint.strip_prefix('='))
                .unwrap_or(&version)
                .to_string();
            self.providers.insert(
                provided_name,
                InstalledProvider {
                    package: name.clone(),
                    version: provided_version,
                },
            );
        }
    }

    fn satisfies(&self, dep: &str) -> Option<SatisfiedDependency> {
        let (name, constraint) = pacman::parse_dependency(dep);
        if let Some(version) = self.packages.get(&name)
            && constraint
                .as_deref()
                .map(|required| pacman::version_satisfies(version, required))
                .unwrap_or(true)
        {
            return Some(SatisfiedDependency {
                version: version.clone(),
                provider: None,
            });
        }

        if let Some(provider) = self.providers.get(&name)
            && constraint
                .as_deref()
                .map(|required| pacman::version_satisfies(&provider.version, required))
                .unwrap_or(true)
        {
            return Some(SatisfiedDependency {
                version: provider.version.clone(),
                provider: Some(provider.package.clone()),
            });
        }

        None
    }
}

fn repo_provider_for(dep: &str) -> Option<String> {
    let dep_name = clean_dependency_name(dep);
    let output = Command::new("pacman")
        .args(["-Sii"])
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    parse_repo_provider(&String::from_utf8_lossy(&output.stdout), &dep_name)
}

fn parse_repo_provider(pacman_sii: &str, dep: &str) -> Option<String> {
    let mut current_name = None;
    let mut current_provides = Vec::new();

    for line in pacman_sii.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if provides_dependency(&current_provides, dep) {
                return current_name;
            }
            current_name = None;
            current_provides.clear();
            continue;
        }

        if let Some(value) = line.strip_prefix("Name") {
            current_name = value.split(':').nth(1).map(|name| name.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Provides") {
            let provides = value.split(':').nth(1).unwrap_or("").trim();
            if provides != "None" {
                current_provides.extend(provides.split_whitespace().map(ToString::to_string));
            }
        }
    }

    None
}

fn provides_dependency(provides: &[String], dep: &str) -> bool {
    provides
        .iter()
        .any(|provided| clean_dependency_name(provided) == dep)
}

fn repo_has_package(package: &str) -> bool {
    Command::new("pacman")
        .args(["-Si", package])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

async fn fetch_aur_pkgbuild_deps(package: &str) -> Result<PkgbuildDeps> {
    let url = format!(
        "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}",
        package
    );
    let content = crate::http::client()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_pkgbuild_deps(&content))
}

pub fn parse_pkgbuild_deps(content: &str) -> PkgbuildDeps {
    let mut deps = PkgbuildDeps::default();
    let mut active_array: Option<String> = None;
    let mut buffer = String::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(active) = &active_array {
            buffer.push(' ');
            buffer.push_str(line.trim_end_matches(')'));
            if line.ends_with(')') {
                apply_array(&mut deps, active, &buffer);
                active_array = None;
                buffer.clear();
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("pkgver=") {
            deps.version = Some(clean_scalar(value));
            continue;
        }

        if let Some((name, value)) = array_assignment(line) {
            if line.ends_with(')') {
                apply_array(&mut deps, name, value);
            } else {
                active_array = Some(name.to_string());
                buffer = value.to_string();
            }
        }
    }

    deps
}

fn array_assignment(line: &str) -> Option<(&str, &str)> {
    for name in [
        "depends",
        "makedepends",
        "checkdepends",
        "conflicts",
        "provides",
    ] {
        let prefix = format!("{}=(", name);
        if let Some(value) = line.strip_prefix(&prefix) {
            return Some((name, value.trim_end_matches(')')));
        }
    }
    None
}

fn apply_array(deps: &mut PkgbuildDeps, name: &str, value: &str) {
    let parsed = parse_array_items(value);
    match name {
        "depends" => deps.depends = parsed,
        "makedepends" => deps.make_depends = parsed,
        "checkdepends" => deps.check_depends = parsed,
        "conflicts" => deps.conflicts = parsed,
        "provides" => deps.provides = parsed,
        _ => {}
    }
}

fn parse_array_items(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|item| item.trim_matches(',').trim_matches('"').trim_matches('\''))
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn clean_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

pub fn clean_dependency_name(dep: &str) -> String {
    let dep = dep
        .trim()
        .trim_matches(',')
        .trim_matches('"')
        .trim_matches('\'');
    pacman::parse_dependency(dep).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_pkgbuild_dependency_metadata() {
        let input = r#"
pkgname=example
pkgver=1.2.3
depends=(
  'glibc>=2.39'
  "openssl"
)
makedepends=('git' 'rust')
checkdepends=(
  'pytest'
)
conflicts=('example-bin')
provides=('example=1.2.3')
"#;

        let parsed = parse_pkgbuild_deps(input);
        assert_eq!(parsed.version.as_deref(), Some("1.2.3"));
        assert_eq!(parsed.depends, vec!["glibc>=2.39", "openssl"]);
        assert_eq!(parsed.make_depends, vec!["git", "rust"]);
        assert_eq!(parsed.check_depends, vec!["pytest"]);
        assert_eq!(parsed.conflicts, vec!["example-bin"]);
        assert_eq!(parsed.provides, vec!["example=1.2.3"]);
    }

    #[test]
    fn cleans_versioned_dependencies() {
        assert_eq!(clean_dependency_name("glibc>=2.39"), "glibc");
        assert_eq!(clean_dependency_name("'openssl'"), "openssl");
        assert_eq!(clean_dependency_name("foo=1.0"), "foo");
    }

    #[test]
    fn installed_index_satisfies_virtual_provider_with_version() {
        let mut index = InstalledIndex::default();
        index.add_package("jre-openjdk".to_string(), "21.0.1-1".to_string(), |_| {
            vec![
                "java-runtime=21".to_string(),
                "java-runtime-headless=21".to_string(),
            ]
        });

        assert_eq!(
            index.satisfies("java-runtime>=17"),
            Some(SatisfiedDependency {
                version: "21".to_string(),
                provider: Some("jre-openjdk".to_string())
            })
        );
        assert_eq!(index.satisfies("java-runtime>=22"), None);
    }

    #[test]
    fn parses_repo_provider_from_pacman_sii() {
        let input = r#"
Repository      : extra
Name            : mesa
Version         : 1:24.0.0-1
Provides        : libgl=1.7 libegl

Repository      : extra
Name            : unrelated
Version         : 1.0-1
Provides        : None
"#;

        assert_eq!(
            parse_repo_provider(input, "libgl"),
            Some("mesa".to_string())
        );
        assert_eq!(parse_repo_provider(input, "java-runtime"), None);
    }
}
