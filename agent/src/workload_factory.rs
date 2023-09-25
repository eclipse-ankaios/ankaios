use async_trait::async_trait;
use common::objects::RuntimeWorkload;
use tokio::sync::mpsc;

use crate::{
    runtime::{OwnableRuntime, RuntimeError},
    stoppable_state_checker::StoppableStateChecker,
    workload::Workload,
};

static COMMAND_BUFFER_SIZE: usize = 5;

#[async_trait]
pub trait WorkloadFactory {
    async fn create_workload(
        &self,
        runtime_workload: RuntimeWorkload,
    ) -> Result<Workload, RuntimeError>;
}

pub struct GenericWorkloadFactory<
    WorkloadId: Send + Sync,
    StateChecker: StoppableStateChecker + Send + Sync,
> {
    runtime: dyn OwnableRuntime<WorkloadId, StateChecker>,
}

#[async_trait]
impl<
        WorkloadId: Send + Sync + 'static,
        StateChecker: StoppableStateChecker + Send + Sync + 'static,
    > WorkloadFactory for GenericWorkloadFactory<WorkloadId, StateChecker>
{
    async fn create_workload(
        &self,
        runtime_workload: RuntimeWorkload,
    ) -> Result<Workload, RuntimeError> {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        let runtime = self.runtime.to_owned();

        let task_handle = tokio::spawn(async move {
            let (workload_id, state_checker) =
                runtime.create_workload(&runtime_workload).await.unwrap();

            // TODO: spawn the task here
        });

        Ok(Workload {
            channel: command_sender,
            task_handle,
        })
    }
}
