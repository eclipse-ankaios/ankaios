use std::{fmt::Display, path::PathBuf};

use async_trait::async_trait;

use common::{
    objects::{AgentName, WorkloadExecutionInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

use crate::state_checker::StateChecker;

#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    Create(String),
    Update(String),
    Delete(String),
    CompleteState(String),
    List(String),
}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Create(msg) => {
                write!(f, "Could not create workload: '{}'", msg)
            }
            RuntimeError::Update(msg) => {
                write!(f, "Could not update workload: '{}'", msg)
            }
            RuntimeError::Delete(msg) => {
                write!(f, "Could not delete workload '{}'", msg)
            }
            RuntimeError::CompleteState(msg) => {
                write!(f, "Could not forward complete state '{}'", msg)
            }
            RuntimeError::List(msg) => {
                write!(f, "Could not get a list of workloads '{}'", msg)
            }
        }
    }
}

#[async_trait]
pub trait Runtime<WorkloadId, StChecker>: Sync + Send
where
    StChecker: StateChecker<WorkloadId>,
    WorkloadId: Send + Sync + 'static,
{
    fn name(&self) -> String;

    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError>;

    async fn create_workload(
        &self,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: StateChangeSender,
    ) -> Result<(WorkloadId, StChecker), RuntimeError>;

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<WorkloadId, RuntimeError>;

    async fn start_checker(
        &self,
        workload_id: &WorkloadId,
        runtime_workload_config: WorkloadSpec,
        update_state_tx: StateChangeSender,
    ) -> Result<StChecker, RuntimeError>;

    async fn delete_workload(&self, workload_id: &WorkloadId) -> Result<(), RuntimeError>;
}

pub trait OwnableRuntime<WorkloadId, StChecker>: Runtime<WorkloadId, StChecker>
where
    StChecker: StateChecker<WorkloadId>,
    WorkloadId: Send + Sync + 'static,
{
    fn to_owned(&self) -> Box<dyn Runtime<WorkloadId, StChecker>>;
}

impl<R, WorkloadId, StChecker> OwnableRuntime<WorkloadId, StChecker> for R
where
    R: Runtime<WorkloadId, StChecker> + Clone + 'static,
    StChecker: StateChecker<WorkloadId>,
    WorkloadId: Send + Sync + 'static,
{
    fn to_owned(&self) -> Box<dyn Runtime<WorkloadId, StChecker>> {
        Box::new(self.clone())
    }
}
