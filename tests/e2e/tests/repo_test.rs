use bpkg_repo::index::RepoIndex;
use std::path::Path;

#[test]
fn test_local_repo_index() {
    let index_path = Path::new("/tmp/bingux-repo/index.toml");
    if !index_path.exists() {
        eprintln!("Skipping: /tmp/bingux-repo not built yet");
        return;
    }
    
    let index = RepoIndex::load(index_path).unwrap();
    assert_eq!(index.meta.scope, "bingux");
    assert!(!index.packages.is_empty(), "Index should have packages");
    
    println!("Repository: @{}", index.meta.scope);
    println!("Packages: {}", index.packages.len());
    for pkg in &index.packages {
        println!("  {} {} ({} bytes)", pkg.name, pkg.version, pkg.size);
    }
    
    // Test search
    let results = index.search("jq");
    assert!(!results.is_empty(), "Should find jq");
    
    let results = index.search("ripgrep");
    assert!(!results.is_empty(), "Should find ripgrep");
    
    // Test find
    let jq = index.find("jq");
    assert!(jq.is_some());
    assert_eq!(jq.unwrap().version, "1.7.1");
    
    println!("\nAll assertions passed!");
}
