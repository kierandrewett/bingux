//! Integration tests for bingux-init: verify that the boot plan includes
//! all required steps in the correct order and that the executor handles
//! them without error.

use bingux_init::executor::BootExecutor;
use bingux_init::plan::{BootPlan, BootStep};

// ── Boot plan ordering ────────────────────────────────────────────

#[test]
fn boot_plan_persistent_mounts_come_before_tmpfs() {
    let plan = BootPlan::standard();

    let first_persistent = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::MountPersistent { .. }))
        .expect("should have a persistent mount");
    let first_tmpfs = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::MountTmpfs { .. }))
        .expect("should have a tmpfs mount");

    assert!(
        first_persistent < first_tmpfs,
        "persistent mounts must come before tmpfs mounts"
    );
}

#[test]
fn boot_plan_config_read_comes_before_etc_generation() {
    let plan = BootPlan::standard();

    let config_pos = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::ReadConfig { .. }))
        .expect("should have ReadConfig step");
    let etc_pos = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::GenerateEtc))
        .expect("should have GenerateEtc step");

    assert!(
        config_pos < etc_pos,
        "ReadConfig must come before GenerateEtc"
    );
}

#[test]
fn boot_plan_etc_generation_comes_before_symlinks() {
    let plan = BootPlan::standard();

    let etc_pos = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::GenerateEtc))
        .expect("should have GenerateEtc step");
    let first_symlink = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::CreateSymlink { .. }))
        .expect("should have symlink step");

    assert!(
        etc_pos < first_symlink,
        "GenerateEtc must come before symlink creation"
    );
}

#[test]
fn boot_plan_switch_root_is_always_last() {
    let plan = BootPlan::standard();
    let last = plan.steps.last().expect("plan should not be empty");
    assert!(
        matches!(last, BootStep::SwitchRoot { .. }),
        "last step must be SwitchRoot, got: {last:?}"
    );
}

#[test]
fn boot_plan_runtime_dirs_created_after_tmpfs() {
    let plan = BootPlan::standard();

    // /run must be mounted as tmpfs before /run/bingux is created
    let run_tmpfs = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::MountTmpfs { target, .. } if target == "/run"))
        .expect("should mount /run as tmpfs");
    let run_bingux = plan
        .steps
        .iter()
        .position(|s| matches!(s, BootStep::CreateDirectory { path } if path == "/run/bingux"))
        .expect("should create /run/bingux");

    assert!(
        run_tmpfs < run_bingux,
        "/run tmpfs must be mounted before /run/bingux is created"
    );
}

// ── Boot plan completeness ────────────────────────────────────────

#[test]
fn boot_plan_includes_all_required_step_types() {
    let plan = BootPlan::standard();

    let has = |f: fn(&BootStep) -> bool| plan.steps.iter().any(f);

    assert!(has(|s| matches!(s, BootStep::MountPersistent { .. })), "missing MountPersistent");
    assert!(has(|s| matches!(s, BootStep::MountTmpfs { .. })), "missing MountTmpfs");
    assert!(has(|s| matches!(s, BootStep::CreateDirectory { .. })), "missing CreateDirectory");
    assert!(has(|s| matches!(s, BootStep::CreateSymlink { .. })), "missing CreateSymlink");
    assert!(has(|s| matches!(s, BootStep::ReadConfig { .. })), "missing ReadConfig");
    assert!(has(|s| matches!(s, BootStep::GenerateEtc)), "missing GenerateEtc");
    assert!(has(|s| matches!(s, BootStep::SwitchRoot { .. })), "missing SwitchRoot");
}

#[test]
fn boot_plan_mounts_system_and_users() {
    let plan = BootPlan::standard();
    let persistent_targets: Vec<&str> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            BootStep::MountPersistent { target, .. } => Some(target.as_str()),
            _ => None,
        })
        .collect();

    assert!(persistent_targets.contains(&"/system"), "must mount /system");
    assert!(persistent_targets.contains(&"/users"), "must mount /users");
}

#[test]
fn boot_plan_creates_compatibility_symlinks() {
    let plan = BootPlan::standard();
    let symlinks: Vec<(&str, &str)> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            BootStep::CreateSymlink { target, link } => Some((target.as_str(), link.as_str())),
            _ => None,
        })
        .collect();

    assert!(symlinks.contains(&("/system/profiles/current/bin", "/bin")));
    assert!(symlinks.contains(&("/system/profiles/current/lib", "/lib")));
    assert!(symlinks.contains(&("/users", "/home")));
}

// ── Executor ──────────────────────────────────────────────────────

#[test]
fn executor_handles_standard_plan_without_error() {
    let plan = BootPlan::standard();
    assert!(BootExecutor::execute_plan(&plan).is_ok());
}

#[test]
fn executor_handles_empty_plan_without_error() {
    let plan = BootPlan { steps: Vec::new() };
    assert!(BootExecutor::execute_plan(&plan).is_ok());
}

#[test]
fn executor_handles_custom_plan() {
    let plan = BootPlan {
        steps: vec![
            BootStep::MountTmpfs {
                target: "/custom".into(),
                size: Some("10M".into()),
            },
            BootStep::CreateDirectory {
                path: "/custom/data".into(),
            },
        ],
    };
    assert!(BootExecutor::execute_plan(&plan).is_ok());
}
