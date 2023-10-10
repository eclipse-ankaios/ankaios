use async_trait::async_trait;

use common::{
    objects::{ExecutionState, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

#[async_trait]
pub trait RuntimeStateChecker<WorkloadId>: Send + Sync + 'static {
    async fn check_state(&self, instance_name: &WorkloadId) -> ExecutionState;
}

#[async_trait]
pub trait StateChecker<WorkloadId> {
    fn start_checker(
        workload_spec: &WorkloadSpec,
        workload_id: WorkloadId,
        manager_interface: StateChangeSender,
        state_checker: impl RuntimeStateChecker<WorkloadId>,
    ) -> Self;
    async fn stop_checker(self);
}
