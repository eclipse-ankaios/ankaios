use std::collections::HashMap;

use async_trait::async_trait;
use common::{
    objects::{RuntimeWorkload, WorkloadSpec},
    std_extensions::IllegalStateResult,
};
use tokio::sync::mpsc;

use crate::{
    runtime::{OwnableRuntime, Runtime},
    stoppable_state_checker::StoppableStateChecker,
    workload::{GenericWorkload, NewWorkload},
};

static COMMAND_BUFFER_SIZE: usize = 5;

#[async_trait]
trait WorkloadCreator {
    async fn create_workload(&self, runtime_workload: RuntimeWorkload) -> Box<dyn NewWorkload>;
}

struct GenericWorkloadCreator<
    WorkloadId: Send + Sync,
    StateChecker: StoppableStateChecker + Send + Sync,
> {
    runtime: dyn OwnableRuntime<WorkloadId, StateChecker>,
}

#[async_trait]
impl<
        WorkloadId: Send + Sync + 'static,
        StateChecker: StoppableStateChecker + Send + Sync + 'static,
    > WorkloadCreator for GenericWorkloadCreator<WorkloadId, StateChecker>
{
    async fn create_workload(&self, runtime_workload: RuntimeWorkload) -> Box<dyn NewWorkload> {
        let (workload_id, state_checker) = self
            .runtime
            .create_workload(&runtime_workload)
            .await
            .unwrap();

        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        Box::new(GenericWorkload {
            channel: command_sender,
            workload_id,
            state_checker,
            runtime: self.runtime.to_owned(),
        })
    }
}

pub struct WorkloadFactory {}

impl WorkloadFactory {
    pub fn create_workload(
        &self,
        runtime_id: String,
        workload_spec: WorkloadSpec,
    ) -> Box<dyn NewWorkload> {
        todo!()
    }
}
