use std::fmt::Display;

use async_trait::async_trait;

use common::objects::{AgentName, RuntimeWorkload, WorkloadExecutionInstanceName};

#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    Create(String),
    Update(String),
    Delete(String),
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
        }
    }
}

#[async_trait]
pub trait Runtime<WorkloadId, StateChecker>: Sync + Send {
    fn name(&self) -> String;

    async fn get_reusable_running_workloads(
        &self,
        agent_name: AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError>;

    async fn create_workload(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
        runtime_workload_config: RuntimeWorkload,
    ) -> Result<(WorkloadId, StateChecker), RuntimeError>;

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<WorkloadId, RuntimeError>;

    async fn start_checker(
        &self,
        workload_id: &WorkloadId,
        runtime_workload_config: RuntimeWorkload,
    ) -> Result<StateChecker, RuntimeError>;

    async fn delete_workload(&self, workload_id: &WorkloadId) -> Result<(), RuntimeError>;
}

pub trait OwnableRuntime<WorkloadId, StateChecker>: Runtime<WorkloadId, StateChecker> {
    fn to_owned(&self) -> Box<dyn Runtime<WorkloadId, StateChecker>>;
}

impl<R, WorkloadId, StateChecker> OwnableRuntime<WorkloadId, StateChecker> for R
where
    R: Runtime<WorkloadId, StateChecker> + Clone + 'static,
{
    fn to_owned(&self) -> Box<dyn Runtime<WorkloadId, StateChecker>> {
        Box::new(self.clone())
    }
}
