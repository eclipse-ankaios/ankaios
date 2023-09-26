use crate::runtime::RuntimeError;
use common::objects::{RuntimeWorkload, WorkloadExecutionInstanceName};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug)]
pub enum WorkloadCommand {
    Stop,
    Update(WorkloadExecutionInstanceName, RuntimeWorkload),
}

// #[derive(Debug)]
pub struct Workload {
    pub channel: mpsc::Sender<WorkloadCommand>,
    pub task_handle: JoinHandle<()>,
}

impl Workload {
    pub async fn update(
        &self,
        instance_name: WorkloadExecutionInstanceName,
        spec: RuntimeWorkload,
    ) -> Result<(), RuntimeError> {
        self.channel
            .send(WorkloadCommand::Update(instance_name, spec))
            .await
            .map_err(|err| RuntimeError::Update(err.to_string()))
    }

    pub async fn delete(self) -> Result<(), RuntimeError> {
        self.channel
            .send(WorkloadCommand::Stop)
            .await
            .map_err(|err| RuntimeError::Delete(err.to_string()))
    }
}
