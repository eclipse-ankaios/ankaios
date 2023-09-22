use std::{collections::HashMap, fmt::format};

use async_trait::async_trait;
use common::{
    objects::{RuntimeWorkload, WorkloadSpec},
    std_extensions::IllegalStateResult,
};
use tokio::sync::mpsc;

use crate::{
    runtime::{OwnableRuntime, Runtime, RuntimeError},
    stoppable_state_checker::StoppableStateChecker,
    workload::{GenericWorkload, NewWorkload},
};

static COMMAND_BUFFER_SIZE: usize = 5;

#[async_trait]
trait WorkloadCreator {
    async fn create_workload(
        &self,
        runtime_workload: RuntimeWorkload,
    ) -> Result<Box<dyn NewWorkload>, RuntimeError>;
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
    async fn create_workload(
        &self,
        runtime_workload: RuntimeWorkload,
    ) -> Result<Box<dyn NewWorkload>, RuntimeError> {
        let (workload_id, state_checker) = self
            .runtime
            .create_workload(&runtime_workload)
            .await
            .unwrap();

        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);

        // TODO: create the task that decouples the creation here

        Ok(Box::new(GenericWorkload {
            channel: command_sender,
            workload_id,
            state_checker,
            runtime: self.runtime.to_owned(),
        }))
    }
}

pub struct WorkloadFactory {
    workload_creator_map: HashMap<String, Box<dyn WorkloadCreator>>,
}

impl WorkloadFactory {
    pub async fn create_workload(
        &self,
        runtime_id: String,
        workload_spec: WorkloadSpec,
    ) -> Result<Box<dyn NewWorkload>, RuntimeError> {
        if let Some(workload_creator) = self.workload_creator_map.get(&runtime_id) {
            workload_creator
                .create_workload(workload_spec.workload)
                .await
        } else {
            Err(RuntimeError::Create(format!(
                "Runtime '{}' not found.",
                runtime_id,
            )))
        }
    }
}
