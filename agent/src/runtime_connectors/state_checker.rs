use async_trait::async_trait;

use common::objects::{ExecutionState, WorkloadSpec};

#[cfg(test)]
use mockall::automock;

use crate::{resource_measurement::ResourceMeasurementSender, workload_state::WorkloadStateSender};

// [impl->swdd~agent-general-runtime-state-getter-interface~1]
#[async_trait]
#[cfg_attr(test, automock)]
pub trait RuntimeStateGetter<WorkloadId>: Send + Sync + 'static
where
    WorkloadId: ToString + Send + Sync + 'static,
{
    // [impl->swdd~allowed-workload-states~2]
    async fn get_state(&self, workload_id: &WorkloadId) -> ExecutionState;
}

// [impl->swdd~agent-general-state-checker-interface~1]
#[async_trait]
pub trait StateChecker<WorkloadId>
where
    WorkloadId: ToString + Send + Sync + 'static,
{
    fn start_checker(
        workload_spec: &WorkloadSpec,
        workload_id: WorkloadId,
        manager_interface: WorkloadStateSender,
        state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self;
    async fn stop_checker(self);
}
