use async_trait::async_trait;

use common::{
    objects::{ExecutionState, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

// [impl->swdd~agent-general-runtime-state-getter-interface~1]
#[async_trait]
pub trait RuntimeStateChecker<WorkloadId>: Send + Sync + 'static {
    async fn get_state(&self, workload_id: &WorkloadId) -> ExecutionState;
}

// [impl->swdd~agent-general-state-checker-interface~1]
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
