use bingux_init::executor::BootExecutor;
use bingux_init::plan::BootPlan;

#[test]
fn execute_standard_plan_succeeds() {
    // The executor stubs should all return Ok
    let plan = BootPlan::standard();
    let result = BootExecutor::execute_plan(&plan);
    assert!(result.is_ok());
}

#[test]
fn execute_empty_plan_succeeds() {
    let plan = BootPlan {
        steps: Vec::new(),
    };
    let result = BootExecutor::execute_plan(&plan);
    assert!(result.is_ok());
}
