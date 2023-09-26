use async_trait::async_trait;

use common::objects::{RuntimeWorkload, WorkloadExecutionInstanceName};

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime::{Runtime, RuntimeError},
};

#[derive(Debug, Clone)]
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
impl Runtime<PodmanKubeWorkloadId, GenericPollingStateChecker> for PodmanKubeRuntime {
    async fn create_workload(
        &self,
        runtime_workload: RuntimeWorkload,
    ) -> Result<(PodmanKubeWorkloadId, GenericPollingStateChecker), RuntimeError> {
        todo!()
    }

    async fn get_workload_id(
        &self,
        instance_name: WorkloadExecutionInstanceName,
    ) -> Result<PodmanKubeWorkloadId, RuntimeError> {
        todo!()
    }

    async fn start_checker(
        &self,
        workload_id: &PodmanKubeWorkloadId,
        runtime_workload_config: RuntimeWorkload,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        todo!()
    }

    async fn delete_workload(
        &self,
        workload_id: &PodmanKubeWorkloadId,
    ) -> Result<(), RuntimeError> {
        todo!()
    }
}
