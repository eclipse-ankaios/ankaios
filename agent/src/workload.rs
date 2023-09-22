use std::sync::Arc;

use crate::{
    runtime::{Runtime, RuntimeError},
    stoppable_state_checker::StoppableStateChecker,
};
use async_trait::async_trait;
use common::objects::RuntimeWorkload;
use tokio::sync::mpsc;

// #[derive(Debug)]
#[async_trait]
pub trait NewWorkload {
    async fn update(&self, spec: RuntimeWorkload) -> Result<(), RuntimeError>;
    async fn delete(self: Box<Self>) -> Result<(), RuntimeError>;
}

#[derive(Debug)]
enum WorkloadCommand {
    Stop,
    Update(RuntimeWorkload),
}

// #[derive(Debug)]
pub struct GenericWorkload<Id, StateChecker: StoppableStateChecker> {
    channel: mpsc::Sender<WorkloadCommand>,
    // workload_spec: WorkloadSpec,
    pub workload_id: Id,
    pub runtime: Arc<dyn Runtime<Id = Id, StateChecker = StateChecker>>,
}

#[async_trait]
impl<Id: Send + Sync, StateChecker: StoppableStateChecker + Send> NewWorkload
    for GenericWorkload<Id, StateChecker>
{
    async fn update(&self, spec: RuntimeWorkload) -> Result<(), RuntimeError> {
        self.channel
            .send(WorkloadCommand::Update(spec))
            .await
            .map_err(|err| RuntimeError::Update(err.to_string()))
    }

    async fn delete(self: Box<Self>) -> Result<(), RuntimeError> {
        self.runtime.delete_workload(self.workload_id).await
    }
}
