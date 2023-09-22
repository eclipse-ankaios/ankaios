use std::sync::Arc;

use crate::{
    runtime::{Runtime, RuntimeConfig},
    stoppable_state_checker::StoppableStateChecker,
    workload_id::WorkloadId,
};
use async_trait::async_trait;
use common::objects::RuntimeWorkload;

// #[derive(Debug)]
#[async_trait]
pub trait NewWorkload {
    fn update(&self, spec: RuntimeWorkload);
    async fn delete(self: Box<Self>);
}

// #[derive(Debug)]
pub struct GenericWorkload<Id, StateChecker: StoppableStateChecker> {
    // channel: CommandChannel,
    // workload_spec: WorkloadSpec,
    pub workload_id: Id,
    pub runtime: Arc<dyn Runtime<Id = Id, StateChecker = StateChecker>>,
}

#[async_trait]
impl<Id: Send, StateChecker: StoppableStateChecker + Send> NewWorkload
    for GenericWorkload<Id, StateChecker>
{
    fn update(&self, spec: RuntimeWorkload) {
        todo!()
    }

    async fn delete(self: Box<Self>) {
        self.runtime.delete_workload(self.workload_id).await;
    }
}
