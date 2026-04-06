//! AUR Integration Tests
//!
//! These tests verify dependency resolution, provider handling, and conflict detection
//! against real AUR package metadata patterns. Tests use mocked data to avoid
//! hitting the AUR API during CI.

// Import from lib
use reap::core::Source;
use reap::dep_graph::{
    ConflictType, Dependency, DependencyGraph, DependencyResolver, PackageNode, ProviderInfo,
};
use reap::devel::{VcsType, is_devel_package_name, parse_vcs_sources};
use reap::version::vercmp;

/// Test that java-runtime providers are resolved correctly
#[test]
fn test_java_runtime_provider_resolution() {
    let mut resolver = DependencyResolver::new();

    // Real providers for java-runtime on Arch
    resolver.add_provider(
        "java-runtime",
        ProviderInfo {
            package_name: "jre-openjdk".to_string(),
            version: Some("21".to_string()),
            priority: 10, // Default JRE
        },
    );
    resolver.add_provider(
        "java-runtime",
        ProviderInfo {
            package_name: "jdk-openjdk".to_string(),
            version: Some("21".to_string()),
            priority: 8, // Also provides runtime
        },
    );
    resolver.add_provider(
        "java-runtime",
        ProviderInfo {
            package_name: "jre-openjdk-headless".to_string(),
            version: Some("21".to_string()),
            priority: 5, // Headless variant
        },
    );
    resolver.add_provider(
        "java-runtime",
        ProviderInfo {
            package_name: "jdk17-openjdk".to_string(),
            version: Some("17".to_string()),
            priority: 6, // Older LTS
        },
    );

    // Should resolve to highest priority (jre-openjdk)
    let resolved = resolver.resolve_dependency("java-runtime");
    assert_eq!(resolved, Some("jre-openjdk".to_string()));

    // If jdk is already installed, use that instead
    resolver.set_installed("jdk-openjdk", "21");
    let resolved = resolver.resolve_dependency("java-runtime");
    assert_eq!(resolved, Some("jdk-openjdk".to_string()));
}

/// Test that libgl providers work correctly
#[test]
fn test_libgl_provider_resolution() {
    let mut resolver = DependencyResolver::new();

    // Real providers for libgl on Arch
    resolver.add_provider(
        "libgl",
        ProviderInfo {
            package_name: "mesa".to_string(),
            version: None,
            priority: 10, // Most common
        },
    );
    resolver.add_provider(
        "libgl",
        ProviderInfo {
            package_name: "nvidia-utils".to_string(),
            version: None,
            priority: 8,
        },
    );
    resolver.add_provider(
        "libgl",
        ProviderInfo {
            package_name: "amdgpu-pro-libgl".to_string(),
            version: None,
            priority: 7,
        },
    );

    // Default should be mesa
    let resolved = resolver.resolve_dependency("libgl");
    assert_eq!(resolved, Some("mesa".to_string()));

    // User has nvidia installed
    resolver.set_installed("nvidia-utils", "545.29.06");
    let resolved = resolver.resolve_dependency("libgl");
    assert_eq!(resolved, Some("nvidia-utils".to_string()));
}

/// Test split package handling (linux kernel packages)
#[test]
fn test_split_package_detection() {
    // Linux kernel is a split package providing multiple sub-packages
    let mut graph = DependencyGraph::new();

    // Main linux package provides linux-headers and linux-docs
    let linux = PackageNode::new("linux".to_string(), "6.7.0-1".to_string(), Source::Pacman)
        .with_dependencies(vec![
            Dependency::new("coreutils".to_string()),
            Dependency::new("kmod".to_string()),
            Dependency::new("mkinitcpio".to_string()),
        ]);

    let linux_headers = PackageNode::new(
        "linux-headers".to_string(),
        "6.7.0-1".to_string(),
        Source::Pacman,
    );

    graph.add_package(linux);
    graph.add_package(linux_headers);

    // Should be able to query both
    assert!(graph.get_package("linux").is_some());
    assert!(graph.get_package("linux-headers").is_some());
}

/// Test conflict detection between packages
#[test]
fn test_nvidia_conflict_detection() {
    let mut graph = DependencyGraph::new();

    // nvidia-utils conflicts with nvidia-340xx-utils
    let nvidia = PackageNode::new(
        "nvidia-utils".to_string(),
        "545.29.06".to_string(),
        Source::Pacman,
    )
    .with_conflicts(vec!["nvidia-340xx-utils".to_string()]);

    let nvidia_340xx = PackageNode::new(
        "nvidia-340xx-utils".to_string(),
        "340.108-1".to_string(),
        Source::Aur,
    )
    .with_conflicts(vec!["nvidia-utils".to_string()]);

    graph.add_package(nvidia);
    graph.add_package(nvidia_340xx);

    let conflicts = graph.detect_conflicts();
    assert!(!conflicts.is_empty());
    assert!(
        conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::DirectConflict)
    );
}

/// Test provides conflict (multiple packages providing same thing)
#[test]
fn test_provides_conflict_detection() {
    let mut graph = DependencyGraph::new();

    // Both iptables and iptables-nft provide iptables
    let mut iptables = PackageNode::new(
        "iptables".to_string(),
        "1.8.10-1".to_string(),
        Source::Pacman,
    );
    iptables.provides = vec!["iptables".to_string()];

    let mut iptables_nft = PackageNode::new(
        "iptables-nft".to_string(),
        "1.8.10-1".to_string(),
        Source::Pacman,
    );
    iptables_nft.provides = vec!["iptables".to_string()];

    graph.add_package(iptables);
    graph.add_package(iptables_nft);

    let conflicts = graph.detect_conflicts();
    assert!(
        conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::ProvidesConflict)
    );
}

/// Test circular dependency detection
#[test]
fn test_circular_dependency_detected() {
    let mut graph = DependencyGraph::new();

    // Create a circular dependency: A -> B -> C -> A
    let pkg_a = PackageNode::new("pkg-a".to_string(), "1.0".to_string(), Source::Aur)
        .with_dependencies(vec![Dependency::new("pkg-b".to_string())]);

    let pkg_b = PackageNode::new("pkg-b".to_string(), "1.0".to_string(), Source::Aur)
        .with_dependencies(vec![Dependency::new("pkg-c".to_string())]);

    let pkg_c = PackageNode::new("pkg-c".to_string(), "1.0".to_string(), Source::Aur)
        .with_dependencies(vec![Dependency::new("pkg-a".to_string())]);

    graph.add_package(pkg_a);
    graph.add_package(pkg_b);
    graph.add_package(pkg_c);

    let cycle = graph.has_circular_dependency();
    assert!(cycle.is_some());
    let cycle_pkgs = cycle.unwrap();
    assert!(cycle_pkgs.len() >= 3);
}

/// Test topological sort for install order
#[test]
fn test_install_order_topological_sort() {
    let mut graph = DependencyGraph::new();

    // Real-world dependency chain: yay -> go -> gcc
    let gcc = PackageNode::new("gcc".to_string(), "14.1.0".to_string(), Source::Pacman);
    let go = PackageNode::new("go".to_string(), "1.22.0".to_string(), Source::Pacman)
        .with_dependencies(vec![Dependency::new("gcc".to_string())]);
    let yay = PackageNode::new("yay".to_string(), "12.3.0".to_string(), Source::Aur)
        .with_dependencies(vec![Dependency::new("go".to_string())])
        .explicit();

    graph.add_package(gcc);
    graph.add_package(go);
    graph.add_package(yay);

    let order = graph.topological_sort().unwrap();

    // Dependencies should come before dependents
    let gcc_pos = order.iter().position(|p| p == "gcc").unwrap();
    let go_pos = order.iter().position(|p| p == "go").unwrap();
    let yay_pos = order.iter().position(|p| p == "yay").unwrap();

    assert!(gcc_pos < go_pos);
    assert!(go_pos < yay_pos);
}

/// Test devel package name detection
#[test]
fn test_devel_package_detection() {
    // Real AUR devel packages
    assert!(is_devel_package_name("yay-git"));
    assert!(is_devel_package_name("neovim-git"));
    assert!(is_devel_package_name("firefox-nightly-git"));
    assert!(is_devel_package_name("linux-git"));
    assert!(is_devel_package_name("mesa-git"));

    // SVN packages
    assert!(is_devel_package_name("llvm-svn"));

    // Non-devel packages
    assert!(!is_devel_package_name("yay"));
    assert!(!is_devel_package_name("git"));
    assert!(!is_devel_package_name("github-cli"));
    assert!(!is_devel_package_name("gitui"));
    assert!(!is_devel_package_name("forgit"));
}

/// Test VCS source parsing from PKGBUILDs
#[test]
fn test_vcs_source_parsing() {
    // Real yay-git PKGBUILD pattern
    let pkgbuild = r#"
pkgname=yay-git
pkgver=12.3.0.r0.g1234567
pkgrel=1
pkgdesc="Yet another yogurt. Pacman wrapper and AUR helper"
arch=('x86_64')
url="https://github.com/Jguer/yay"
license=('GPL3')
depends=('pacman' 'git')
makedepends=('go')
source=("git+https://github.com/Jguer/yay.git")
sha256sums=('SKIP')
"#;

    let sources = parse_vcs_sources(pkgbuild);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].0, VcsType::Git);
    assert!(sources[0].1.contains("github.com/Jguer/yay"));
}

/// Test version comparison for AUR updates
#[test]
fn test_aur_version_comparison() {
    use std::cmp::Ordering;

    // Simple version bumps
    assert_eq!(vercmp("1.0.0", "1.0.1"), Ordering::Less);
    assert_eq!(vercmp("1.0.1", "1.0.0"), Ordering::Greater);
    assert_eq!(vercmp("1.0.0", "1.0.0"), Ordering::Equal);

    // Real AUR package versions
    assert_eq!(vercmp("12.3.0-1", "12.3.1-1"), Ordering::Less);
    assert_eq!(vercmp("0.9.0-1", "0.10.0-1"), Ordering::Less);

    // Epoch handling
    assert_eq!(vercmp("1:1.0.0-1", "2.0.0-1"), Ordering::Greater); // Epoch wins

    // Git versions
    assert_eq!(
        vercmp("12.3.0.r0.g1234567-1", "12.3.0.r5.gabcdef0-1"),
        Ordering::Less
    );
}

/// Test resolver with multiple packages
#[test]
fn test_multi_package_resolution() {
    let mut resolver = DependencyResolver::new();

    // Set up some installed packages
    resolver.set_installed("gcc", "14.1.0");
    resolver.set_installed("glibc", "2.39");
    resolver.set_installed("coreutils", "9.4");

    // Add provider for java-runtime
    resolver.add_provider(
        "java-runtime",
        ProviderInfo {
            package_name: "jre-openjdk".to_string(),
            version: Some("21".to_string()),
            priority: 10,
        },
    );

    // Resolve a package that needs java-runtime
    let result = resolver
        .resolve(&["java-runtime".to_string(), "gcc".to_string()])
        .unwrap();

    // gcc is already installed, should not be in resolved list
    assert!(!result.resolved_packages.contains(&"gcc".to_string()));

    // jre-openjdk should be chosen for java-runtime
    assert!(
        result
            .resolved_packages
            .contains(&"jre-openjdk".to_string())
    );

    // Provider choice should be recorded
    assert_eq!(result.provider_choices.len(), 1);
    assert_eq!(
        result.provider_choices[0].virtual_package,
        "java-runtime".to_string()
    );
}

/// Test parsing provides from real PKGBUILD
#[test]
fn test_parse_real_provides() {
    // jre-openjdk provides java-runtime, java-runtime-headless, etc.
    let provides_line =
        r#"provides=("java-runtime=21" "java-runtime-headless=21" "java-runtime-openjdk=21")"#;
    let provides = DependencyResolver::parse_provides(provides_line);

    assert_eq!(provides.len(), 3);
    assert!(
        provides
            .iter()
            .any(|(name, ver)| name == "java-runtime" && ver == &Some("21".to_string()))
    );
    assert!(
        provides
            .iter()
            .any(|(name, _)| name == "java-runtime-headless")
    );
    assert!(
        provides
            .iter()
            .any(|(name, _)| name == "java-runtime-openjdk")
    );
}

/// Test parsing conflicts from real PKGBUILD
#[test]
fn test_parse_real_conflicts() {
    // nvidia-utils conflicts
    let conflicts_line = r#"conflicts=("nvidia-340xx-utils" "nvidia-390xx-utils")"#;
    let conflicts = DependencyResolver::parse_conflicts(conflicts_line);

    assert_eq!(conflicts.len(), 2);
    assert!(conflicts.contains(&"nvidia-340xx-utils".to_string()));
    assert!(conflicts.contains(&"nvidia-390xx-utils".to_string()));
}

/// Test dependency graph with optional dependencies
#[test]
fn test_optional_dependencies() {
    let mut graph = DependencyGraph::new();

    // Package with optional deps (like neovim)
    let neovim = PackageNode::new("neovim".to_string(), "0.10.0".to_string(), Source::Pacman)
        .with_dependencies(vec![
            Dependency::new("luajit".to_string()),
            Dependency::new("msgpack-c".to_string()),
            Dependency::new("tree-sitter".to_string()).optional(), // Optional
            Dependency::new("python-pynvim".to_string()).optional(), // Optional
        ]);

    graph.add_package(neovim);

    let pkg = graph.get_package("neovim").unwrap();

    // Should have 4 deps total
    assert_eq!(pkg.dependencies.len(), 4);

    // 2 optional, 2 required
    let optional_count = pkg.dependencies.iter().filter(|d| d.optional).count();
    assert_eq!(optional_count, 2);
}
