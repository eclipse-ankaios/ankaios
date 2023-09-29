use std::path::PathBuf;

use async_trait::async_trait;

use common::{
    objects::{AgentName, WorkloadExecutionInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

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
    fn name(&self) -> String {
        "podman-kube".to_string()
    }

    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError> {
        todo!()
    }

    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: StateChangeSender,
    ) -> Result<(PodmanKubeWorkloadId, GenericPollingStateChecker), RuntimeError> {
        Ok((
            PodmanKubeWorkloadId {
                manifest: "sdcdsc".to_string(),
            },
            GenericPollingStateChecker { task_handle: None },
        ))
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<PodmanKubeWorkloadId, RuntimeError> {
        todo!()
    }

    async fn start_checker(
        &self,
        workload_id: &PodmanKubeWorkloadId,
        workload_spec: WorkloadSpec,
        update_state_tx: StateChangeSender,
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
