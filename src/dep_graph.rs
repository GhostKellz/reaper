use crate::core::Source;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version_constraint: Option<String>,
    pub optional: bool,
    pub arch_specific: bool,
}

impl Dependency {
    pub fn new(name: String) -> Self {
        Self {
            name,
            version_constraint: None,
            optional: false,
            arch_specific: false,
        }
    }

    pub fn with_version(mut self, version: String) -> Self {
        self.version_constraint = Some(version);
        self
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref version) = self.version_constraint {
            write!(f, " ({})", version)?;
        }
        if self.optional {
            write!(f, " [optional]")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PackageNode {
    pub name: String,
    pub version: String,
    pub source: Source,
    pub dependencies: Vec<Dependency>,
    pub conflicts: Vec<String>,
    pub provides: Vec<String>,
    pub installed: bool,
    pub explicit: bool, // User explicitly requested this package
}

impl PackageNode {
    pub fn new(name: String, version: String, source: Source) -> Self {
        Self {
            name,
            version,
            source,
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            provides: Vec::new(),
            installed: false,
            explicit: false,
        }
    }

    pub fn with_dependencies(mut self, deps: Vec<Dependency>) -> Self {
        self.dependencies = deps;
        self
    }

    pub fn with_conflicts(mut self, conflicts: Vec<String>) -> Self {
        self.conflicts = conflicts;
        self
    }

    pub fn explicit(mut self) -> Self {
        self.explicit = true;
        self
    }
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    nodes: HashMap<String, PackageNode>,
    edges: HashMap<String, Vec<String>>, // package -> dependencies
    reverse_edges: HashMap<String, Vec<String>>, // package -> dependents
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
        }
    }

    pub fn add_package(&mut self, package: PackageNode) {
        let name = package.name.clone();
        let deps: Vec<String> = package
            .dependencies
            .iter()
            .map(|d| d.name.clone())
            .collect();

        // Add reverse edges for dependency tracking
        for dep in &deps {
            self.reverse_edges
                .entry(dep.clone())
                .or_default()
                .push(name.clone());
        }

        self.edges.insert(name.clone(), deps);
        self.nodes.insert(name, package);
    }

    pub fn get_package(&self, name: &str) -> Option<&PackageNode> {
        self.nodes.get(name)
    }

    pub fn get_dependencies(&self, name: &str) -> Vec<String> {
        self.edges.get(name).cloned().unwrap_or_default()
    }

    pub fn get_dependents(&self, name: &str) -> Vec<String> {
        self.reverse_edges.get(name).cloned().unwrap_or_default()
    }

    pub fn has_circular_dependency(&self) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.nodes.keys() {
            if !visited.contains(node)
                && let Some(cycle) =
                    self.dfs_cycle_detection(node, &mut visited, &mut rec_stack, &mut Vec::new())
            {
                return Some(cycle);
            }
        }
        None
    }

    fn dfs_cycle_detection(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(dependencies) = self.edges.get(node) {
            for dep in dependencies {
                if !visited.contains(dep) {
                    if let Some(cycle) = self.dfs_cycle_detection(dep, visited, rec_stack, path) {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(dep) {
                    // Found a cycle - extract the cycle from the path
                    if let Some(cycle_start) = path.iter().position(|x| x == dep) {
                        let mut cycle = path[cycle_start..].to_vec();
                        cycle.push(dep.clone()); // Close the cycle
                        return Some(cycle);
                    }
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
        None
    }

    pub fn detect_conflicts(&self) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();

        for (pkg_name, package) in &self.nodes {
            // Check direct conflicts
            for conflict in &package.conflicts {
                if self.nodes.contains_key(conflict) {
                    conflicts.push(ConflictInfo {
                        package1: pkg_name.clone(),
                        package2: conflict.clone(),
                        conflict_type: ConflictType::DirectConflict,
                        description: format!("{} conflicts with {}", pkg_name, conflict),
                    });
                }
            }

            // Check provides conflicts
            for provides in &package.provides {
                for (other_pkg, other_package) in &self.nodes {
                    if other_pkg != pkg_name && other_package.provides.contains(provides) {
                        conflicts.push(ConflictInfo {
                            package1: pkg_name.clone(),
                            package2: other_pkg.clone(),
                            conflict_type: ConflictType::ProvidesConflict,
                            description: format!(
                                "{} and {} both provide {}",
                                pkg_name, other_pkg, provides
                            ),
                        });
                    }
                }
            }
        }

        conflicts
    }

    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Calculate in-degree for each node
        for node in self.nodes.keys() {
            in_degree.insert(node.clone(), 0);
        }

        for dependencies in self.edges.values() {
            for dep in dependencies {
                if let Some(degree) = in_degree.get_mut(dep) {
                    *degree += 1;
                }
            }
        }

        // Add nodes with no incoming edges to queue
        for (node, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(node.clone());
            }
        }

        // Process queue
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());

            if let Some(dependencies) = self.edges.get(&node) {
                for dep in dependencies {
                    if let Some(degree) = in_degree.get_mut(dep) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(anyhow!("Circular dependency detected in topological sort"));
        }

        // Reverse to get dependency order (dependencies first)
        result.reverse();
        Ok(result)
    }

    pub fn get_install_order(&self) -> Result<InstallPlan> {
        let conflicts = self.detect_conflicts();
        if !conflicts.is_empty() {
            return Err(anyhow!("Conflicts detected: {:?}", conflicts));
        }

        if let Some(cycle) = self.has_circular_dependency() {
            return Err(anyhow!(
                "Circular dependency detected: {}",
                cycle.join(" -> ")
            ));
        }

        let sorted = self.topological_sort()?;
        let mut plan = InstallPlan::new();

        // Group packages by their role in the dependency chain
        for package_name in sorted {
            if let Some(package) = self.nodes.get(&package_name) {
                let step = InstallStep {
                    package: package.clone(),
                    reason: if package.explicit {
                        InstallReason::Explicit
                    } else {
                        InstallReason::Dependency
                    },
                };
                plan.steps.push(step);
            }
        }

        Ok(plan)
    }

    pub fn calculate_removal_impact(&self, package: &str) -> RemovalImpact {
        let mut impact = RemovalImpact {
            package: package.to_string(),
            direct_dependents: Vec::new(),
            orphaned_packages: Vec::new(),
            total_affected: 0,
        };

        // Get direct dependents
        if let Some(dependents) = self.reverse_edges.get(package) {
            impact.direct_dependents = dependents.clone();

            // Find packages that would become orphaned
            for dependent in dependents {
                if self.would_be_orphaned(dependent, package) {
                    impact.orphaned_packages.push(dependent.clone());
                }
            }
        }

        impact.total_affected = impact.direct_dependents.len() + impact.orphaned_packages.len();
        impact
    }

    fn would_be_orphaned(&self, package: &str, removed_package: &str) -> bool {
        // Check if removing 'removed_package' would leave 'package' without any dependencies
        if let Some(package_node) = self.nodes.get(package) {
            if package_node.explicit {
                return false; // Explicitly installed packages are not orphaned
            }

            // Count how many of this package's dependents would remain
            if let Some(dependents) = self.reverse_edges.get(package) {
                let remaining_dependents: Vec<&String> = dependents
                    .iter()
                    .filter(|&dep| dep != removed_package)
                    .collect();

                return remaining_dependents.is_empty();
            }
        }
        false
    }

    pub fn print_dependency_tree(&self, root: &str, max_depth: Option<usize>) {
        println!("Dependency tree for {}:", root);
        let mut visited = HashSet::new();
        self.print_tree_recursive(root, 0, &mut visited, max_depth);
    }

    fn print_tree_recursive(
        &self,
        package: &str,
        depth: usize,
        visited: &mut HashSet<String>,
        max_depth: Option<usize>,
    ) {
        if let Some(max) = max_depth
            && depth > max
        {
            return;
        }

        let indent = "  ".repeat(depth);
        let marker = if depth == 0 { "📦" } else { "└─" };

        if let Some(node) = self.nodes.get(package) {
            println!(
                "{}{}  {} {} ({})",
                indent,
                marker,
                package,
                node.version,
                node.source.label()
            );
        } else {
            println!("{}{}  {} (not found)", indent, marker, package);
        }

        if visited.contains(package) {
            println!("{}   [circular dependency]", "  ".repeat(depth + 1));
            return;
        }

        visited.insert(package.to_string());

        if let Some(dependencies) = self.edges.get(package) {
            for dep in dependencies {
                self.print_tree_recursive(dep, depth + 1, visited, max_depth);
            }
        }

        visited.remove(package);
    }
}

#[derive(Debug, Clone)]
pub struct ConflictInfo {
    pub package1: String,
    pub package2: String,
    pub conflict_type: ConflictType,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    DirectConflict,
    ProvidesConflict,
    FileConflict,
    VersionConflict,
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub steps: Vec<InstallStep>,
}

impl Default for InstallPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl InstallPlan {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn print_summary(&self) {
        println!("\n📋 Installation Plan:");
        println!("━━━━━━━━━━━━━━━━━━━━━");

        let explicit_count = self
            .steps
            .iter()
            .filter(|s| s.reason == InstallReason::Explicit)
            .count();
        let dependency_count = self.steps.len() - explicit_count;

        println!(
            "Packages to install: {} ({} explicit, {} dependencies)",
            self.steps.len(),
            explicit_count,
            dependency_count
        );

        println!("\nInstallation order:");
        for (i, step) in self.steps.iter().enumerate() {
            let icon = match step.reason {
                InstallReason::Explicit => "📦",
                InstallReason::Dependency => "🔗",
            };
            println!(
                "  {}. {} {} {} ({})",
                i + 1,
                icon,
                step.package.name,
                step.package.version,
                step.package.source.label()
            );
        }
        println!();
    }
}

#[derive(Debug, Clone)]
pub struct InstallStep {
    pub package: PackageNode,
    pub reason: InstallReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallReason {
    Explicit,
    Dependency,
}

#[derive(Debug, Clone)]
pub struct RemovalImpact {
    pub package: String,
    pub direct_dependents: Vec<String>,
    pub orphaned_packages: Vec<String>,
    pub total_affected: usize,
}

impl RemovalImpact {
    pub fn print_summary(&self) {
        println!("\n🗑️  Removal Impact for '{}':", self.package);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        if self.direct_dependents.is_empty() {
            println!("✅ Safe to remove - no packages depend on this");
        } else {
            println!(
                "⚠️  {} package(s) depend on this:",
                self.direct_dependents.len()
            );
            for dependent in &self.direct_dependents {
                println!("    • {}", dependent);
            }

            if !self.orphaned_packages.is_empty() {
                println!(
                    "\n🚨 {} package(s) would become orphaned:",
                    self.orphaned_packages.len()
                );
                for orphaned in &self.orphaned_packages {
                    println!("    • {}", orphaned);
                }
            }
        }

        println!("\nTotal affected packages: {}", self.total_affected);
    }
}

/// Dependency resolver that handles providers, conflicts, and version constraints
pub struct DependencyResolver {
    /// Mapping of virtual packages to their providers
    providers: HashMap<String, Vec<ProviderInfo>>,
    /// Track conflicts between packages
    conflicts: HashMap<String, Vec<String>>,
    /// Track split packages (base package -> members)
    split_packages: HashMap<String, Vec<String>>,
    /// Installed packages for reference
    installed: HashMap<String, String>, // name -> version
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub package_name: String,
    pub version: Option<String>,
    pub priority: i32, // Higher = preferred
}

#[derive(Debug, Clone)]
pub struct ResolutionResult {
    pub resolved_packages: Vec<String>,
    pub provider_choices: Vec<ProviderChoice>,
    pub conflicts_found: Vec<ConflictInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderChoice {
    pub virtual_package: String,
    pub chosen_provider: String,
    pub alternatives: Vec<String>,
    pub reason: String,
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            conflicts: HashMap::new(),
            split_packages: HashMap::new(),
            installed: HashMap::new(),
        }
    }

    /// Register a provider for a virtual package
    pub fn add_provider(&mut self, virtual_pkg: &str, provider: ProviderInfo) {
        self.providers
            .entry(virtual_pkg.to_string())
            .or_default()
            .push(provider);
    }

    /// Register a conflict between packages
    pub fn add_conflict(&mut self, pkg1: &str, pkg2: &str) {
        self.conflicts
            .entry(pkg1.to_string())
            .or_default()
            .push(pkg2.to_string());
        self.conflicts
            .entry(pkg2.to_string())
            .or_default()
            .push(pkg1.to_string());
    }

    /// Register a split package
    pub fn add_split_package(&mut self, base: &str, members: Vec<String>) {
        self.split_packages.insert(base.to_string(), members);
    }

    /// Register an installed package
    pub fn set_installed(&mut self, pkg: &str, version: &str) {
        self.installed.insert(pkg.to_string(), version.to_string());
    }

    /// Check if a package or its provider is installed
    pub fn is_satisfied(&self, dep: &str) -> bool {
        // Direct check
        if self.installed.contains_key(dep) {
            return true;
        }

        // Check if any provider of this virtual package is installed
        if let Some(providers) = self.providers.get(dep) {
            for provider in providers {
                if self.installed.contains_key(&provider.package_name) {
                    return true;
                }
            }
        }

        false
    }

    /// Resolve a dependency, returning the actual package name
    pub fn resolve_dependency(&self, dep: &str) -> Option<String> {
        // Direct package
        if self.installed.contains_key(dep) || !self.providers.contains_key(dep) {
            return Some(dep.to_string());
        }

        // Virtual package - find best provider
        if let Some(providers) = self.providers.get(dep) {
            // First, check if any provider is already installed
            for provider in providers {
                if self.installed.contains_key(&provider.package_name) {
                    return Some(provider.package_name.clone());
                }
            }

            // Otherwise, pick the highest priority provider
            if let Some(best) = providers.iter().max_by_key(|p| p.priority) {
                return Some(best.package_name.clone());
            }
        }

        None
    }

    /// Get all providers for a virtual package
    pub fn get_providers(&self, virtual_pkg: &str) -> Vec<&ProviderInfo> {
        self.providers
            .get(virtual_pkg)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Check for conflicts with a package
    pub fn check_conflicts(&self, pkg: &str) -> Vec<String> {
        let mut conflicts = Vec::new();

        if let Some(pkg_conflicts) = self.conflicts.get(pkg) {
            for conflict in pkg_conflicts {
                if self.installed.contains_key(conflict) {
                    conflicts.push(conflict.clone());
                }
            }
        }

        conflicts
    }

    /// Resolve a list of packages to install
    pub fn resolve(&self, packages: &[String]) -> Result<ResolutionResult> {
        let mut result = ResolutionResult {
            resolved_packages: Vec::new(),
            provider_choices: Vec::new(),
            conflicts_found: Vec::new(),
            warnings: Vec::new(),
        };

        let mut to_resolve: VecDeque<String> = packages.iter().cloned().collect();
        let mut resolved: HashSet<String> = HashSet::new();

        while let Some(pkg) = to_resolve.pop_front() {
            if resolved.contains(&pkg) {
                continue;
            }

            // Check if this is a virtual package
            if let Some(providers) = self.providers.get(&pkg) {
                // Check if any provider is already installed
                let installed_provider = providers
                    .iter()
                    .find(|p| self.installed.contains_key(&p.package_name));

                let chosen = if let Some(provider) = installed_provider {
                    provider.package_name.clone()
                } else {
                    // Pick highest priority
                    let best = providers
                        .iter()
                        .max_by_key(|p| p.priority)
                        .map(|p| p.package_name.clone())
                        .ok_or_else(|| anyhow!("No providers for virtual package: {}", pkg))?;

                    result.provider_choices.push(ProviderChoice {
                        virtual_package: pkg.clone(),
                        chosen_provider: best.clone(),
                        alternatives: providers
                            .iter()
                            .filter(|p| p.package_name != best)
                            .map(|p| p.package_name.clone())
                            .collect(),
                        reason: "Highest priority provider".to_string(),
                    });

                    best
                };

                resolved.insert(pkg);
                if !resolved.contains(&chosen) {
                    to_resolve.push_back(chosen);
                }
            } else {
                // Direct package
                // Check conflicts
                let conflicts = self.check_conflicts(&pkg);
                if !conflicts.is_empty() {
                    for conflict in &conflicts {
                        result.conflicts_found.push(ConflictInfo {
                            package1: pkg.clone(),
                            package2: conflict.clone(),
                            conflict_type: ConflictType::DirectConflict,
                            description: format!("{} conflicts with installed {}", pkg, conflict),
                        });
                    }
                }

                if !self.is_satisfied(&pkg) {
                    result.resolved_packages.push(pkg.clone());
                }
                resolved.insert(pkg);
            }
        }

        Ok(result)
    }

    /// Parse provides from a PKGBUILD line
    pub fn parse_provides(provides_line: &str) -> Vec<(String, Option<String>)> {
        let mut result = Vec::new();
        let content = provides_line
            .trim_start_matches("provides=")
            .trim_matches(['(', ')', '"', '\'', ' ']);

        for item in content.split_whitespace() {
            let item = item.trim_matches(['"', '\'']);
            if item.is_empty() {
                continue;
            }

            // Format: pkgname or pkgname=version
            if let Some((name, ver)) = item.split_once('=') {
                result.push((name.to_string(), Some(ver.to_string())));
            } else {
                result.push((item.to_string(), None));
            }
        }

        result
    }

    /// Parse conflicts from a PKGBUILD line
    pub fn parse_conflicts(conflicts_line: &str) -> Vec<String> {
        let content = conflicts_line
            .trim_start_matches("conflicts=")
            .trim_matches(['(', ')', '"', '\'', ' ']);

        content
            .split_whitespace()
            .map(|s| s.trim_matches(['"', '\'']).to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

// Builder pattern for creating dependency graphs
pub struct DependencyGraphBuilder {
    graph: DependencyGraph,
}

impl Default for DependencyGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraphBuilder {
    pub fn new() -> Self {
        Self {
            graph: DependencyGraph::new(),
        }
    }

    pub fn add_package(mut self, package: PackageNode) -> Self {
        self.graph.add_package(package);
        self
    }

    pub fn build(self) -> DependencyGraph {
        self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dependency_chain() {
        let mut graph = DependencyGraph::new();

        let pkg_a = PackageNode::new("pkg-a".to_string(), "1.0.0".to_string(), Source::Aur)
            .with_dependencies(vec![Dependency::new("pkg-b".to_string())])
            .explicit();

        let pkg_b = PackageNode::new("pkg-b".to_string(), "1.0.0".to_string(), Source::Aur)
            .with_dependencies(vec![Dependency::new("pkg-c".to_string())]);

        let pkg_c = PackageNode::new("pkg-c".to_string(), "1.0.0".to_string(), Source::Pacman);

        graph.add_package(pkg_a);
        graph.add_package(pkg_b);
        graph.add_package(pkg_c);

        let order = graph.topological_sort().unwrap();
        assert_eq!(order, vec!["pkg-c", "pkg-b", "pkg-a"]);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut graph = DependencyGraph::new();

        let pkg_a = PackageNode::new("pkg-a".to_string(), "1.0.0".to_string(), Source::Aur)
            .with_dependencies(vec![Dependency::new("pkg-b".to_string())]);

        let pkg_b = PackageNode::new("pkg-b".to_string(), "1.0.0".to_string(), Source::Aur)
            .with_dependencies(vec![Dependency::new("pkg-a".to_string())]);

        graph.add_package(pkg_a);
        graph.add_package(pkg_b);

        assert!(graph.has_circular_dependency().is_some());
    }

    #[test]
    fn test_conflict_detection() {
        let mut graph = DependencyGraph::new();

        let pkg_a = PackageNode::new("pkg-a".to_string(), "1.0.0".to_string(), Source::Aur)
            .with_conflicts(vec!["pkg-b".to_string()]);

        let pkg_b = PackageNode::new("pkg-b".to_string(), "1.0.0".to_string(), Source::Aur);

        graph.add_package(pkg_a);
        graph.add_package(pkg_b);

        let conflicts = graph.detect_conflicts();
        assert!(!conflicts.is_empty());
    }

    #[test]
    fn test_dependency_resolver_providers() {
        let mut resolver = DependencyResolver::new();

        // java-runtime is provided by multiple packages
        resolver.add_provider(
            "java-runtime",
            ProviderInfo {
                package_name: "jre-openjdk".to_string(),
                version: Some("21".to_string()),
                priority: 10,
            },
        );
        resolver.add_provider(
            "java-runtime",
            ProviderInfo {
                package_name: "jdk-openjdk".to_string(),
                version: Some("21".to_string()),
                priority: 5,
            },
        );

        // Should resolve to highest priority
        let resolved = resolver.resolve_dependency("java-runtime");
        assert_eq!(resolved, Some("jre-openjdk".to_string()));

        // If jdk is installed, should use that
        resolver.set_installed("jdk-openjdk", "21");
        let resolved = resolver.resolve_dependency("java-runtime");
        assert_eq!(resolved, Some("jdk-openjdk".to_string()));
    }

    #[test]
    fn test_dependency_resolver_conflicts() {
        let mut resolver = DependencyResolver::new();

        resolver.add_conflict("pkg-a", "pkg-b");
        resolver.set_installed("pkg-b", "1.0");

        let conflicts = resolver.check_conflicts("pkg-a");
        assert_eq!(conflicts, vec!["pkg-b".to_string()]);
    }

    #[test]
    fn test_parse_provides() {
        let line = r#"provides=("java-runtime=21" "java-runtime-headless")"#;
        let provides = DependencyResolver::parse_provides(line);

        assert_eq!(provides.len(), 2);
        assert_eq!(provides[0].0, "java-runtime");
        assert_eq!(provides[0].1, Some("21".to_string()));
        assert_eq!(provides[1].0, "java-runtime-headless");
        assert_eq!(provides[1].1, None);
    }

    #[test]
    fn test_parse_conflicts() {
        let line = r#"conflicts=("other-pkg" "another-pkg")"#;
        let conflicts = DependencyResolver::parse_conflicts(line);

        assert_eq!(conflicts.len(), 2);
        assert!(conflicts.contains(&"other-pkg".to_string()));
        assert!(conflicts.contains(&"another-pkg".to_string()));
    }

    #[test]
    fn test_resolve_with_providers() {
        let mut resolver = DependencyResolver::new();

        resolver.add_provider(
            "terminal-emulator",
            ProviderInfo {
                package_name: "alacritty".to_string(),
                version: None,
                priority: 10,
            },
        );
        resolver.add_provider(
            "terminal-emulator",
            ProviderInfo {
                package_name: "kitty".to_string(),
                version: None,
                priority: 5,
            },
        );

        let result = resolver
            .resolve(&["terminal-emulator".to_string()])
            .unwrap();

        // Should pick alacritty (highest priority) and note the choice
        assert_eq!(result.resolved_packages.len(), 1);
        assert_eq!(result.resolved_packages[0], "alacritty");
        assert_eq!(result.provider_choices.len(), 1);
        assert_eq!(
            result.provider_choices[0].virtual_package,
            "terminal-emulator"
        );
    }
}
