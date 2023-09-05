use std::fmt::Display;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

use common::objects::WorkloadExecutionInstanceName;

#[derive(Debug, PartialEq, Eq)]
pub enum WorkloadError {
    StartError(String),
    DeleteError(String),
}

impl Display for WorkloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkloadError::StartError(msg) => {
                write!(f, "Could not start workload: '{}'", msg)
            }
            WorkloadError::DeleteError(msg) => {
                write!(f, "Could not delete workload '{}'", msg)
            }
        }
    }
}

#[async_trait]
#[cfg_attr(test, automock(type State=String; type Id=String;))]
pub trait Workload {
    type Id: Send;
    type State: Send;
    async fn start(&self) -> Result<Self::State, WorkloadError>;

    async fn replace(
        &self,
        existing_instance_name: WorkloadExecutionInstanceName,
        existing_id: Self::Id,
    ) -> Result<Self::State, WorkloadError>;

    fn resume(&self, id: Self::Id) -> Result<Self::State, WorkloadError>;

    async fn delete(&mut self, running_state: Self::State) -> Result<(), WorkloadError>;

    fn name(&self) -> String;
}
