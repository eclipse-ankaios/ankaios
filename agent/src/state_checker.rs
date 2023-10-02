use async_trait::async_trait;

use common::{
    objects::{ExecutionState, WorkloadExecutionInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

#[async_trait]
pub trait RuntimeStateChecker: Send + Sync + 'static {
    async fn check_state(&self, instance_name: &WorkloadExecutionInstanceName) -> ExecutionState;
}

#[async_trait]
pub trait StateChecker {
    fn start_checker(
        workload_spec: &WorkloadSpec,
        manager_interface: StateChangeSender,
        state_checker: impl RuntimeStateChecker,
    ) -> Self;
    async fn stop_checker(self);
}
