use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::graph::DependencyGraph;

/// Compute the RUNPATH string for a package given its resolved dependencies.
///
/// Ordering:
/// 1. The package's own `lib/` directory
/// 2. Direct dependencies' `lib/` directories (in depends order)
/// 3. Transitive dependencies' `lib/` directories (depth-first order)
///
/// Duplicate directories are suppressed (first occurrence wins).
/// The result is a colon-separated string suitable for ELF RUNPATH.
pub fn compute_runpath(
    store_root: &Path,
    package_id: &str,
    graph: &DependencyGraph,
) -> String {
    let mut seen = HashSet::new();
    let mut dirs: Vec<PathBuf> = Vec::new();

    // 1. Package's own lib/
    let own_lib = store_root.join(package_id).join("lib");
    seen.insert(own_lib.clone());
    dirs.push(own_lib);

    // 2. Direct deps in depends order
    if let Some(direct) = graph.direct_deps(package_id) {
        for dep in direct {
            let dep_lib = store_root.join(dep).join("lib");
            if seen.insert(dep_lib.clone()) {
                dirs.push(dep_lib);
            }
        }
    }

    // 3. Transitive deps (depth-first, skipping already-seen)
    let transitive = graph.transitive_deps(package_id);
    for dep in &transitive {
        let dep_lib = store_root.join(dep).join("lib");
        if seen.insert(dep_lib.clone()) {
            dirs.push(dep_lib);
        }
    }

    dirs.iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DependencyGraph;
    use std::path::Path;

    #[test]
    fn runpath_linear_chain() {
        // A depends on B, B depends on C
        let pairs = vec![
            ("a-1.0-x86_64-linux".to_string(), vec!["b-1.0-x86_64-linux".to_string()]),
            ("b-1.0-x86_64-linux".to_string(), vec!["c-1.0-x86_64-linux".to_string()]),
            ("c-1.0-x86_64-linux".to_string(), vec![]),
        ];
        let graph = DependencyGraph::from_pairs(&pairs);
        let store = Path::new("/system/packages");

        let rp = compute_runpath(store, "a-1.0-x86_64-linux", &graph);
        assert_eq!(
            rp,
            "/system/packages/a-1.0-x86_64-linux/lib:\
             /system/packages/b-1.0-x86_64-linux/lib:\
             /system/packages/c-1.0-x86_64-linux/lib"
        );
    }

    #[test]
    fn runpath_diamond_no_duplicates() {
        // A → B, A → C, B → D, C → D
        let pairs = vec![
            (
                "a-1.0-x86_64-linux".to_string(),
                vec!["b-1.0-x86_64-linux".to_string(), "c-1.0-x86_64-linux".to_string()],
            ),
            ("b-1.0-x86_64-linux".to_string(), vec!["d-1.0-x86_64-linux".to_string()]),
            ("c-1.0-x86_64-linux".to_string(), vec!["d-1.0-x86_64-linux".to_string()]),
            ("d-1.0-x86_64-linux".to_string(), vec![]),
        ];
        let graph = DependencyGraph::from_pairs(&pairs);
        let store = Path::new("/system/packages");

        let rp = compute_runpath(store, "a-1.0-x86_64-linux", &graph);
        let parts: Vec<&str> = rp.split(':').collect();

        // Own lib first
        assert_eq!(parts[0], "/system/packages/a-1.0-x86_64-linux/lib");
        // Direct deps next (b, c)
        assert_eq!(parts[1], "/system/packages/b-1.0-x86_64-linux/lib");
        assert_eq!(parts[2], "/system/packages/c-1.0-x86_64-linux/lib");
        // Transitive dep d (only once, from b's transitive traversal)
        assert_eq!(parts[3], "/system/packages/d-1.0-x86_64-linux/lib");
        // No duplicates
        assert_eq!(parts.len(), 4);
    }

    #[test]
    fn runpath_no_deps() {
        let pairs = vec![("standalone-1.0-x86_64-linux".to_string(), vec![])];
        let graph = DependencyGraph::from_pairs(&pairs);
        let store = Path::new("/system/packages");

        let rp = compute_runpath(store, "standalone-1.0-x86_64-linux", &graph);
        assert_eq!(rp, "/system/packages/standalone-1.0-x86_64-linux/lib");
    }
}
