use async_trait::async_trait;
use common::objects::WorkloadSpec;

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime::{Runtime, RuntimeError},
};

#[derive(Debug, Copy, Clone)]
pub struct PodmanKubeRuntime {}

#[derive(Debug)]
pub struct PodmanKubeConfig {}

#[derive(Clone, Debug)]
pub struct PodmanKubeWorkloadId {
    // Podman currently does not provide an Id for a created manifest
    // and one needs the compete manifest to tear down the deployed resources.
    pub manifest: String,
}

#[derive(Debug)]
pub struct PlayKubeOutput {}

#[derive(Debug)]
pub struct PlayKubeError {}

#[async_trait]
impl Runtime for PodmanKubeRuntime {
    type Id = PodmanKubeWorkloadId;
    type StateChecker = GenericPollingStateChecker;

    async fn create_workload(
        &self,
        workload_spec: &WorkloadSpec,
    ) -> Result<(Self::Id, Self::StateChecker), RuntimeError> {
        todo!()
    }

    async fn delete_workload(&self, workload_id: Self::Id) -> Result<(), RuntimeError> {
        todo!()
    }
}
