use async_trait::async_trait;

use common::{
    objects::{ExecutionState, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

#[cfg(test)]
use mockall::automock;

// [impl->swdd~agent-general-runtime-state-getter-interface~1]
#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeStateGetter<WorkloadId>: Send + Sync + 'static
where
    WorkloadId: Send + Sync + 'static,
{
    // [impl->swdd~allowed-workload-states~1]
    async fn get_state(&self, workload_id: &WorkloadId) -> ExecutionState;
}

// [impl->swdd~agent-general-state-checker-interface~1]
#[async_trait]
pub trait StateChecker<WorkloadId>
where
    WorkloadId: Send + Sync + 'static,
{
    fn start_checker(
        workload_spec: &WorkloadSpec,
        workload_id: WorkloadId,
        manager_interface: StateChangeSender,
        state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self;
    async fn stop_checker(self);
}
