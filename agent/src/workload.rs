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
pub enum WorkloadCommand {
    Stop,
    Update(RuntimeWorkload),
}

// #[derive(Debug)]
pub struct GenericWorkload<Id, StateChecker: StoppableStateChecker> {
    pub channel: mpsc::Sender<WorkloadCommand>,
    pub workload_id: Id,
    pub state_checker: StateChecker,
    pub runtime: Box<dyn Runtime<Id, StateChecker>>,
}

#[async_trait]
impl<Id: Send + Sync, StateChecker: StoppableStateChecker + Send + Sync> NewWorkload
    for GenericWorkload<Id, StateChecker>
{
    async fn update(&self, spec: RuntimeWorkload) -> Result<(), RuntimeError> {
        self.channel
            .send(WorkloadCommand::Update(spec))
            .await
            .map_err(|err| RuntimeError::Update(err.to_string()))
    }

    async fn delete(self: Box<Self>) -> Result<(), RuntimeError> {
        self.channel
            .send(WorkloadCommand::Stop)
            .await
            .map_err(|err| RuntimeError::Delete(err.to_string()))
    }
}
