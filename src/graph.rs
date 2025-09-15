use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DependencyNode {
    pub package: String,
    pub version: String,
    pub source: crate::core::Source,
    pub trust_score: Option<f32>,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
    pub optional: bool,
    pub make_only: bool,
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub nodes: HashMap<String, DependencyNode>,
    pub root_packages: Vec<String>,
}

pub struct GraphVisualizer {
    pub graph: DependencyGraph,
}

impl GraphVisualizer {
    pub fn new() -> Self {
        Self {
            graph: DependencyGraph {
                nodes: HashMap::new(),
                root_packages: Vec::new(),
            },
        }
    }

    /// Build dependency graph for a package
    pub async fn build_graph(&mut self, packages: &[String]) -> Result<()> {
        let mut visited = HashSet::new();

        for pkg in packages {
            self.build_graph_recursive(pkg, &mut visited, 0).await?;
            if !self.graph.root_packages.contains(pkg) {
                self.graph.root_packages.push(pkg.clone());
            }
        }

        Ok(())
    }

    async fn build_graph_recursive(
        &mut self,
        pkg: &str,
        visited: &mut HashSet<String>,
        depth: usize,
    ) -> Result<()> {
        if visited.contains(pkg) || depth > 20 {
            return Ok(());
        }

        visited.insert(pkg.to_string());

        // Get package info
        let (deps, make_deps) = self.get_package_dependencies(pkg).await?;
        let version = self.get_package_version(pkg).await.unwrap_or_default();
        let source = self.detect_package_source(pkg);

        // Create node
        let mut all_deps = deps.clone();
        all_deps.extend(make_deps.iter().cloned());

        let node = DependencyNode {
            package: pkg.to_string(),
            version,
            source,
            trust_score: None, // Will be filled by trust engine
            dependencies: all_deps.clone(),
            dependents: Vec::new(),
            optional: false,
            make_only: false,
        };

        self.graph.nodes.insert(pkg.to_string(), node);

        // Recursively build dependencies with Box::pin for async recursion
        for dep in &all_deps {
            // Update dependent lists
            if let Some(dep_node) = self.graph.nodes.get_mut(dep) {
                dep_node.dependents.push(pkg.to_string());
            }

            // Box::pin is required for recursive async calls
            Box::pin(self.build_graph_recursive(dep, visited, depth + 1)).await?;
        }

        Ok(())
    }

    /// Generate ASCII art dependency tree
    pub fn print_tree(&self, package: &str, max_depth: Option<usize>) {
        println!("\n🌳 Dependency Tree for {}", package);
        println!("{}", "=".repeat(60));

        if let Some(node) = self.graph.nodes.get(package) {
            self.print_node(node, "", true, HashSet::new(), 0, max_depth.unwrap_or(10));
        } else {
            println!("Package not found in graph");
        }
    }

    fn print_node(
        &self,
        node: &DependencyNode,
        prefix: &str,
        is_last: bool,
        mut visited: HashSet<String>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth || visited.contains(&node.package) {
            return;
        }

        visited.insert(node.package.clone());

        // Print current node
        let connector = if is_last { "└── " } else { "├── " };
        let trust_badge = node
            .trust_score
            .map(|score| self.get_trust_badge(score))
            .unwrap_or("❓");

        println!(
            "{}{}{} {} v{} ({})",
            prefix,
            connector,
            trust_badge,
            node.package,
            node.version,
            node.source.label()
        );

        // Print dependencies
        let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

        for (i, dep_name) in node.dependencies.iter().enumerate() {
            let is_last_dep = i == node.dependencies.len() - 1;

            if let Some(dep_node) = self.graph.nodes.get(dep_name) {
                self.print_node(
                    dep_node,
                    &new_prefix,
                    is_last_dep,
                    visited.clone(),
                    depth + 1,
                    max_depth,
                );
            } else {
                let connector = if is_last_dep {
                    "└── "
                } else {
                    "├── "
                };
                println!("{}{}❓ {} (not analyzed)", new_prefix, connector, dep_name);
            }
        }
    }

    /// Generate DOT format for Graphviz
    pub fn export_dot(&self, output_path: &PathBuf) -> Result<()> {
        let mut dot = String::from("digraph dependencies {\n");
        dot.push_str("  rankdir=TB;\n");
        dot.push_str("  node [shape=box, style=rounded];\n\n");

        // Add nodes with styling based on trust scores
        for (pkg, node) in &self.graph.nodes {
            let color = match node.trust_score {
                Some(score) if score >= 8.0 => "green",
                Some(score) if score >= 6.0 => "lightgreen",
                Some(score) if score >= 4.0 => "yellow",
                Some(score) if score >= 2.0 => "orange",
                Some(_) => "red",
                None => "lightgray",
            };

            let label = format!("{}\\nv{}\\n{}", pkg, node.version, node.source.label());
            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\", fillcolor={}, style=filled];\n",
                pkg, label, color
            ));
        }

        dot.push('\n');

        // Add edges
        for (pkg, node) in &self.graph.nodes {
            for dep in &node.dependencies {
                dot.push_str(&format!("  \"{}\" -> \"{}\";\n", pkg, dep));
            }
        }

        dot.push_str("}\n");

        fs::write(output_path, dot)?;
        println!("📊 Dependency graph exported to: {}", output_path.display());

        Ok(())
    }

    /// Detect circular dependencies
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();

        for pkg in self.graph.nodes.keys() {
            if !visited.contains(pkg) {
                let mut path = Vec::new();
                self.dfs_cycles(
                    pkg,
                    &mut visited,
                    &mut recursion_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_cycles(
        &self,
        pkg: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(pkg.to_string());
        recursion_stack.insert(pkg.to_string());
        path.push(pkg.to_string());

        if let Some(node) = self.graph.nodes.get(pkg) {
            for dep in &node.dependencies {
                if !visited.contains(dep) {
                    self.dfs_cycles(dep, visited, recursion_stack, path, cycles);
                } else if recursion_stack.contains(dep) {
                    // Found a cycle
                    if let Some(start_idx) = path.iter().position(|p| p == dep) {
                        let cycle = path[start_idx..].to_vec();
                        cycles.push(cycle);
                    }
                }
            }
        }

        path.pop();
        recursion_stack.remove(pkg);
    }

    /// Find packages that would be orphaned if a package is removed
    pub fn find_orphans_if_removed(&self, package: &str) -> Vec<String> {
        let mut orphans = Vec::new();

        if let Some(node) = self.graph.nodes.get(package) {
            for dep in &node.dependencies {
                if let Some(dep_node) = self.graph.nodes.get(dep) {
                    // Check if this dependency would become orphaned
                    let other_dependents: Vec<_> = dep_node
                        .dependents
                        .iter()
                        .filter(|&d| d != package)
                        .collect();

                    if other_dependents.is_empty() && !self.graph.root_packages.contains(dep) {
                        orphans.push(dep.clone());
                        // Recursively find orphans of this orphan
                        let recursive_orphans = self.find_orphans_if_removed(dep);
                        orphans.extend(recursive_orphans);
                    }
                }
            }
        }

        // Remove duplicates
        orphans.sort();
        orphans.dedup();
        orphans
    }

    async fn get_package_dependencies(&self, pkg: &str) -> Result<(Vec<String>, Vec<String>)> {
        // Try AUR first
        let pkgbuild = crate::aur::get_pkgbuild_preview(pkg);
        if !pkgbuild.contains("not found") {
            let (depends, makedepends, _, _, _) = crate::utils::resolve_deps(&pkgbuild);
            return Ok((depends, makedepends));
        }

        // Fall back to pacman
        let output = std::process::Command::new("pacman")
            .args(["-Si", pkg])
            .output()?;

        if output.status.success() {
            let info = String::from_utf8_lossy(&output.stdout);
            let mut depends = Vec::new();
            let mut makedepends = Vec::new();

            for line in info.lines() {
                if line.starts_with("Depends On") {
                    depends = line
                        .split(':')
                        .nth(1)
                        .unwrap_or("")
                        .split_whitespace()
                        .filter(|s| *s != "None")
                        .map(|s| s.to_string())
                        .collect();
                } else if line.starts_with("Make Deps") {
                    makedepends = line
                        .split(':')
                        .nth(1)
                        .unwrap_or("")
                        .split_whitespace()
                        .filter(|s| *s != "None")
                        .map(|s| s.to_string())
                        .collect();
                }
            }

            Ok((depends, makedepends))
        } else {
            Ok((Vec::new(), Vec::new()))
        }
    }

    async fn get_package_version(&self, pkg: &str) -> Option<String> {
        // Try pacman first
        if let Some(version) = crate::pacman::get_version(pkg) {
            return Some(version);
        }

        // Try AUR
        if let Ok(info) = crate::aur::fetch_package_info(pkg) {
            return Some(info.version);
        }

        None
    }

    fn detect_package_source(&self, pkg: &str) -> crate::core::Source {
        // Simple source detection - could be enhanced
        if crate::pacman::is_installed(pkg) {
            crate::core::Source::Pacman
        } else {
            crate::core::Source::Aur
        }
    }

    fn get_trust_badge(&self, score: f32) -> &'static str {
        match score {
            s if s >= 8.0 => "🛡️",
            s if s >= 6.0 => "✅",
            s if s >= 4.0 => "⚠️",
            s if s >= 2.0 => "🚨",
            _ => "❌",
        }
    }
}

impl Default for GraphVisualizer {
    fn default() -> Self {
        Self::new()
    }
}
