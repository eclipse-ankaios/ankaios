use std::fmt::Display;

use async_trait::async_trait;

use common::objects::WorkloadSpec;

use crate::stoppable_state_checker::StoppableStateChecker;

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
// #[cfg_attr(test, automock(type State=String; type Id=String;))]
pub trait Runtime: Sync + Send {
    type Id;
    type StateChecker: Send + StoppableStateChecker; // This is definitely not Clone

    async fn create_workload(
        &self,
        workload_spec: &WorkloadSpec,
    ) -> Result<(Self::Id, Self::StateChecker), RuntimeError>;

    async fn delete_workload(&self, workload_id: Self::Id) -> Result<(), RuntimeError>;
}
