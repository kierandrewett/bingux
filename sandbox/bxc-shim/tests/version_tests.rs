use bxc_shim::version::parse_version_syntax;

#[test]
fn parse_with_version() {
    let (name, version) = parse_version_syntax("firefox@128.0.1");
    assert_eq!(name, "firefox");
    assert_eq!(version, Some("128.0.1".to_string()));
}

#[test]
fn parse_without_version() {
    let (name, version) = parse_version_syntax("firefox");
    assert_eq!(name, "firefox");
    assert_eq!(version, None);
}

#[test]
fn parse_empty_version_returns_none() {
    let (name, version) = parse_version_syntax("firefox@");
    assert_eq!(name, "firefox@");
    assert_eq!(version, None);
}

#[test]
fn parse_empty_name_returns_full_input() {
    let (name, version) = parse_version_syntax("@1.0");
    assert_eq!(name, "@1.0");
    assert_eq!(version, None);
}

#[test]
fn parse_complex_version() {
    let (name, version) = parse_version_syntax("gcc@14.1.0");
    assert_eq!(name, "gcc");
    assert_eq!(version, Some("14.1.0".to_string()));
}
