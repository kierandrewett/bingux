use bxc_shim::dispatch::DispatchEntry;
use bxc_shim::resolver::{resolve_binary, resolve_versioned};

#[test]
fn resolve_binary_returns_correct_store_path() {
    let entry = DispatchEntry {
        package: "firefox-129.0-x86_64-linux".to_string(),
        binary: "bin/firefox".to_string(),
        sandbox: "standard".to_string(),
    };

    let path = resolve_binary(&entry);
    assert_eq!(
        path.to_str().unwrap(),
        "/system/packages/firefox-129.0-x86_64-linux/bin/firefox"
    );
}

#[test]
fn resolve_binary_nested_path() {
    let entry = DispatchEntry {
        package: "neovim-0.10.0-x86_64-linux".to_string(),
        binary: "share/nvim/bin/nvim".to_string(),
        sandbox: "none".to_string(),
    };

    let path = resolve_binary(&entry);
    assert_eq!(
        path.to_str().unwrap(),
        "/system/packages/neovim-0.10.0-x86_64-linux/share/nvim/bin/nvim"
    );
}

#[test]
fn resolve_versioned_returns_correct_path() {
    let path = resolve_versioned("firefox", "128.0.1", "x86_64-linux");
    assert_eq!(
        path.to_str().unwrap(),
        "/system/packages/firefox-128.0.1-x86_64-linux/bin/firefox"
    );
}
