use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use crate::core::Source;

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
        let deps: Vec<String> = package.dependencies.iter().map(|d| d.name.clone()).collect();

        // Add reverse edges for dependency tracking
        for dep in &deps {
            self.reverse_edges
                .entry(dep.clone())
                .or_insert_with(Vec::new)
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
            if !visited.contains(node) {
                if let Some(cycle) = self.dfs_cycle_detection(node, &mut visited, &mut rec_stack, &mut Vec::new()) {
                    return Some(cycle);
                }
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
                            description: format!("{} and {} both provide {}", pkg_name, other_pkg, provides),
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
            return Err(anyhow!("Circular dependency detected: {}", cycle.join(" -> ")));
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
        if let Some(max) = max_depth {
            if depth > max {
                return;
            }
        }

        let indent = "  ".repeat(depth);
        let marker = if depth == 0 { "📦" } else { "└─" };

        if let Some(node) = self.nodes.get(package) {
            println!("{}{}  {} {} ({})", indent, marker, package, node.version, node.source.label());
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

impl InstallPlan {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
        }
    }

    pub fn print_summary(&self) {
        println!("\n📋 Installation Plan:");
        println!("━━━━━━━━━━━━━━━━━━━━━");

        let explicit_count = self.steps.iter().filter(|s| s.reason == InstallReason::Explicit).count();
        let dependency_count = self.steps.len() - explicit_count;

        println!("Packages to install: {} ({} explicit, {} dependencies)",
                 self.steps.len(), explicit_count, dependency_count);

        println!("\nInstallation order:");
        for (i, step) in self.steps.iter().enumerate() {
            let icon = match step.reason {
                InstallReason::Explicit => "📦",
                InstallReason::Dependency => "🔗",
            };
            println!("  {}. {} {} {} ({})",
                     i + 1, icon, step.package.name, step.package.version, step.package.source.label());
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
            println!("⚠️  {} package(s) depend on this:", self.direct_dependents.len());
            for dependent in &self.direct_dependents {
                println!("    • {}", dependent);
            }

            if !self.orphaned_packages.is_empty() {
                println!("\n🚨 {} package(s) would become orphaned:", self.orphaned_packages.len());
                for orphaned in &self.orphaned_packages {
                    println!("    • {}", orphaned);
                }
            }
        }

        println!("\nTotal affected packages: {}", self.total_affected);
    }
}

// Builder pattern for creating dependency graphs
pub struct DependencyGraphBuilder {
    graph: DependencyGraph,
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
}