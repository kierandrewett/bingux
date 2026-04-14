use std::collections::{HashMap, HashSet, VecDeque};

use bingux_common::BinguxError;

/// A dependency graph that supports topological sorting and cycle detection.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Adjacency list: package_id → set of packages it depends on.
    edges: HashMap<String, Vec<String>>,
    /// All known package IDs (including those with no dependencies).
    nodes: HashSet<String>,
}

impl DependencyGraph {
    /// Build a dependency graph from a list of (package_id, depends_on) pairs.
    ///
    /// Each pair declares that the first element depends on all elements in the
    /// second.  Packages that appear only as dependencies (never as the first
    /// element) are treated as having no dependencies of their own.
    pub fn from_pairs(pairs: &[(String, Vec<String>)]) -> Self {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut nodes: HashSet<String> = HashSet::new();

        for (pkg, deps) in pairs {
            nodes.insert(pkg.clone());
            for dep in deps {
                nodes.insert(dep.clone());
            }
            edges.insert(pkg.clone(), deps.clone());
        }

        // Ensure all nodes that appear only as dependencies have an entry.
        for node in &nodes {
            edges.entry(node.clone()).or_default();
        }

        Self { edges, nodes }
    }

    /// Return a topological ordering of all packages (dependencies before
    /// dependents).  Returns an error if the graph contains a cycle.
    ///
    /// Uses Kahn's algorithm (BFS-based) for deterministic output and clear
    /// cycle reporting.
    pub fn topological_sort(&self) -> Result<Vec<String>, BinguxError> {
        // Compute in-degrees.
        // edges[A] = [B, C] means A depends on B and C, i.e. edges B→A, C→A.
        // So in_degree[A] = len(edges[A]).
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            in_degree.insert(node.as_str(), 0);
        }
        for (pkg, deps) in &self.edges {
            // pkg has edges FROM each dep TO pkg, so in_degree[pkg] = deps.len()
            *in_degree.entry(pkg.as_str()).or_insert(0) = deps.len();
        }

        // Collect nodes with in-degree 0.
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut sorted_in_degree: Vec<_> = in_degree.iter().collect();
        sorted_in_degree.sort_by_key(|(name, _)| *name);
        for (node, deg) in &sorted_in_degree {
            if **deg == 0 {
                queue.push_back(node.to_string());
            }
        }

        let mut result: Vec<String> = Vec::new();

        // Build reverse adjacency: dep → list of packages that depend on dep.
        let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
        for (pkg, deps) in &self.edges {
            for dep in deps {
                reverse
                    .entry(dep.as_str())
                    .or_default()
                    .push(pkg.as_str());
            }
        }

        while let Some(node) = queue.pop_front() {
            result.push(node.clone());
            if let Some(dependents) = reverse.get(node.as_str()) {
                let mut sorted_dependents: Vec<&str> = dependents.clone();
                sorted_dependents.sort();
                for dependent in sorted_dependents {
                    let deg = in_degree.get_mut(dependent).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dependent.to_string());
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            // Find the cycle for error reporting.
            let in_result: HashSet<&str> = result.iter().map(|s| s.as_str()).collect();
            let cycle_members: Vec<String> = self
                .nodes
                .iter()
                .filter(|n| !in_result.contains(n.as_str()))
                .cloned()
                .collect();
            return Err(BinguxError::DependencyCycle(cycle_members.join(" -> ")));
        }

        Ok(result)
    }

    /// Return the direct dependencies of a package.
    pub fn direct_deps(&self, package: &str) -> Option<&[String]> {
        self.edges.get(package).map(|v| v.as_slice())
    }

    /// Return all transitive dependencies of a package in depth-first order
    /// (no duplicates).
    pub fn transitive_deps(&self, package: &str) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.dfs_deps(package, &mut visited, &mut result);
        result
    }

    fn dfs_deps(&self, package: &str, visited: &mut HashSet<String>, result: &mut Vec<String>) {
        if let Some(deps) = self.edges.get(package) {
            for dep in deps {
                if visited.insert(dep.clone()) {
                    result.push(dep.clone());
                    self.dfs_deps(dep, visited, result);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_chain() {
        // A depends on B, B depends on C
        let pairs = vec![
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["c".to_string()]),
            ("c".to_string(), vec![]),
        ];
        let graph = DependencyGraph::from_pairs(&pairs);
        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted, vec!["c", "b", "a"]);
    }

    #[test]
    fn diamond() {
        // A → B, A → C, B → D, C → D
        let pairs = vec![
            ("a".to_string(), vec!["b".to_string(), "c".to_string()]),
            ("b".to_string(), vec!["d".to_string()]),
            ("c".to_string(), vec!["d".to_string()]),
            ("d".to_string(), vec![]),
        ];
        let graph = DependencyGraph::from_pairs(&pairs);
        let sorted = graph.topological_sort().unwrap();

        // D must come first, A must come last.
        assert_eq!(sorted[0], "d");
        assert_eq!(sorted[sorted.len() - 1], "a");
        // No duplicates.
        let unique: HashSet<&str> = sorted.iter().map(|s| s.as_str()).collect();
        assert_eq!(unique.len(), 4);
        // B and C must come before A but after D.
        let pos = |name: &str| sorted.iter().position(|s| s == name).unwrap();
        assert!(pos("d") < pos("b"));
        assert!(pos("d") < pos("c"));
        assert!(pos("b") < pos("a"));
        assert!(pos("c") < pos("a"));
    }

    #[test]
    fn cycle_detection() {
        // A → B → A
        let pairs = vec![
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ];
        let graph = DependencyGraph::from_pairs(&pairs);
        let result = graph.topological_sort();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BinguxError::DependencyCycle(_)));
    }

    #[test]
    fn transitive_deps_depth_first() {
        // A → B → D, A → C → D
        let pairs = vec![
            ("a".to_string(), vec!["b".to_string(), "c".to_string()]),
            ("b".to_string(), vec!["d".to_string()]),
            ("c".to_string(), vec!["d".to_string()]),
            ("d".to_string(), vec![]),
        ];
        let graph = DependencyGraph::from_pairs(&pairs);
        let deps = graph.transitive_deps("a");
        // b first (DFS), then d (b's dep), then c (a's second dep)
        // d should not appear twice
        assert_eq!(deps, vec!["b", "d", "c"]);
    }
}
