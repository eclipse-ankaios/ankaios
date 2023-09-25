use crate::runtime::RuntimeError;
use common::objects::RuntimeWorkload;
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug)]
pub enum WorkloadCommand {
    Stop,
    Update(RuntimeWorkload),
}

// #[derive(Debug)]
pub struct Workload {
    pub channel: mpsc::Sender<WorkloadCommand>,
    pub task_handle: JoinHandle<()>,
}

impl Workload {
    pub async fn update(&self, spec: RuntimeWorkload) -> Result<(), RuntimeError> {
        self.channel
            .send(WorkloadCommand::Update(spec))
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
