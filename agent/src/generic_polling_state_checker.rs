use async_trait::async_trait;
use std::time::Duration;
use tokio::{task::JoinHandle, time};

use crate::state_checker::{RuntimeStateChecker, StateChecker};
use common::{
    objects::{ExecutionState, WorkloadSpec},
    state_change_interface::{StateChangeInterface, StateChangeSender},
    std_extensions::IllegalStateResult,
};

// [impl->swdd~agent-provides-generic-state-checker-implementation~1]
const STATUS_CHECK_INTERVAL_MS: u64 = 1000;

#[derive(Debug)]
pub struct GenericPollingStateChecker {
    workload_name: String,
    task_handle: JoinHandle<()>,
}

#[async_trait]
impl<WorkloadId> StateChecker<WorkloadId> for GenericPollingStateChecker
where
    WorkloadId: Send + Sync + 'static,
{
    // [impl->swdd~agent-provides-generic-state-checker-implementation~1]
    fn start_checker(
        workload_spec: &WorkloadSpec,
        workload_id: WorkloadId,
        manager_interface: StateChangeSender,
        state_checker: impl RuntimeStateChecker<WorkloadId>,
    ) -> Self {
        let workload_spec = workload_spec.clone();
        let workload_name = workload_spec.name.clone();
        let task_handle = tokio::spawn(async move {
            let mut last_state = ExecutionState::ExecUnknown;
            let mut interval = time::interval(Duration::from_millis(STATUS_CHECK_INTERVAL_MS));
            loop {
                interval.tick().await;
                let current_state = state_checker.get_state(&workload_id).await;

                if current_state != last_state {
                    log::debug!(
                        "The workload {} has changed its state to {:?}",
                        workload_spec.name,
                        current_state
                    );
                    last_state = current_state.clone();

                    // [impl->swdd~podman-workload-sends-workload-state~1]
                    manager_interface
                        .update_workload_state(vec![common::objects::WorkloadState {
                            agent_name: workload_spec.agent.clone(),
                            workload_name: workload_spec.name.to_string(),
                            execution_state: current_state,
                        }])
                        .await
                        .unwrap_or_illegal_state();
                }
            }
        });

        GenericPollingStateChecker {
            workload_name,
            task_handle,
        }
    }

    async fn stop_checker(self) {
        drop(self);
    }
}

impl Drop for GenericPollingStateChecker {
    fn drop(&mut self) {
        self.task_handle.abort();
        log::trace!("Over and out for workload '{}'", self.workload_name);
    }
}
