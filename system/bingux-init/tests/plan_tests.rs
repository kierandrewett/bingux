use bingux_init::plan::{BootPlan, BootStep};

#[test]
fn standard_plan_has_expected_step_count() {
    let plan = BootPlan::standard();
    assert_eq!(plan.steps.len(), 13);
}

#[test]
fn standard_plan_starts_with_persistent_mounts() {
    let plan = BootPlan::standard();

    // First two steps should be persistent mounts for /system and /users
    match &plan.steps[0] {
        BootStep::MountPersistent {
            target, subvol, ..
        } => {
            assert_eq!(target, "/system");
            assert_eq!(subvol.as_deref(), Some("@system"));
        }
        other => panic!("expected MountPersistent, got {:?}", other),
    }

    match &plan.steps[1] {
        BootStep::MountPersistent {
            target, subvol, ..
        } => {
            assert_eq!(target, "/users");
            assert_eq!(subvol.as_deref(), Some("@users"));
        }
        other => panic!("expected MountPersistent, got {:?}", other),
    }
}

#[test]
fn standard_plan_includes_tmpfs_mounts() {
    let plan = BootPlan::standard();
    let tmpfs_targets: Vec<&str> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            BootStep::MountTmpfs { target, .. } => Some(target.as_str()),
            _ => None,
        })
        .collect();

    assert!(tmpfs_targets.contains(&"/etc"), "missing /etc tmpfs");
    assert!(tmpfs_targets.contains(&"/run"), "missing /run tmpfs");
    assert!(tmpfs_targets.contains(&"/tmp"), "missing /tmp tmpfs");
}

#[test]
fn standard_plan_etc_tmpfs_has_size_limit() {
    let plan = BootPlan::standard();
    let etc_step = plan.steps.iter().find(|step| matches!(step, BootStep::MountTmpfs { target, .. } if target == "/etc"));

    match etc_step {
        Some(BootStep::MountTmpfs { size, .. }) => {
            assert_eq!(size.as_deref(), Some("50M"));
        }
        other => panic!("expected /etc MountTmpfs with size, got {:?}", other),
    }
}

#[test]
fn standard_plan_includes_compatibility_symlinks() {
    let plan = BootPlan::standard();
    let symlinks: Vec<(&str, &str)> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            BootStep::CreateSymlink { target, link } => {
                Some((target.as_str(), link.as_str()))
            }
            _ => None,
        })
        .collect();

    assert!(
        symlinks.contains(&("/system/profiles/current/bin", "/bin")),
        "missing /bin symlink"
    );
    assert!(
        symlinks.contains(&("/system/profiles/current/lib", "/lib")),
        "missing /lib symlink"
    );
    assert!(
        symlinks.contains(&("/users", "/home")),
        "missing /home symlink"
    );
}

#[test]
fn standard_plan_ends_with_switch_root() {
    let plan = BootPlan::standard();
    let last = plan.steps.last().unwrap();

    match last {
        BootStep::SwitchRoot { new_root, init } => {
            assert_eq!(new_root, "/");
            assert_eq!(init, "/system/profiles/current/bin/systemd");
        }
        other => panic!("expected SwitchRoot as last step, got {:?}", other),
    }
}

#[test]
fn standard_plan_includes_generate_etc() {
    let plan = BootPlan::standard();
    let has_generate_etc = plan
        .steps
        .iter()
        .any(|step| matches!(step, BootStep::GenerateEtc));
    assert!(has_generate_etc, "missing GenerateEtc step");
}

#[test]
fn standard_plan_includes_read_config() {
    let plan = BootPlan::standard();
    let config_step = plan
        .steps
        .iter()
        .find(|step| matches!(step, BootStep::ReadConfig { .. }));

    match config_step {
        Some(BootStep::ReadConfig { path }) => {
            assert_eq!(path, "/system/config/system.toml");
        }
        other => panic!("expected ReadConfig step, got {:?}", other),
    }
}

#[test]
fn standard_plan_creates_runtime_directories() {
    let plan = BootPlan::standard();
    let dirs: Vec<&str> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            BootStep::CreateDirectory { path } => Some(path.as_str()),
            _ => None,
        })
        .collect();

    assert!(dirs.contains(&"/run/bingux"), "missing /run/bingux");
    assert!(
        dirs.contains(&"/run/bingux/system"),
        "missing /run/bingux/system"
    );
}

#[test]
fn boot_step_debug_formatting() {
    let step = BootStep::MountTmpfs {
        target: "/tmp".into(),
        size: None,
    };
    let debug = format!("{:?}", step);
    assert!(debug.contains("MountTmpfs"));
    assert!(debug.contains("/tmp"));
}
