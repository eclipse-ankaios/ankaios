use std::{cmp::min, path::PathBuf};

use common::{
    objects::{AgentName, ExecutionState, WorkloadExecutionInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

use async_trait::async_trait;
use futures_util::TryFutureExt;

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    podman::podman_cli,
    runtime::{Runtime, RuntimeError},
    state_checker::{RuntimeStateChecker, StateChecker},
};

const CONFIG_VOLUME_SUFFIX: &str = ".config";
const PODS_VOLUME_SUFFIX: &str = ".pods";

#[derive(Debug, Clone)]
pub struct PodmanKubeRuntime {}

#[derive(Debug)]
pub struct PodmanKubeConfig {}

#[derive(Clone, Debug)]
pub struct PodmanKubeWorkloadId {
    // Podman currently does not provide an Id for a created manifest
    // and one needs the compete manifest to tear down the deployed resources.
    pub name: WorkloadExecutionInstanceName,
    pub pods: Option<Vec<String>>,
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
        let name_filter = format!(
            "{}{}$",
            agent_name.get_filter_suffix(),
            CONFIG_VOLUME_SUFFIX
        );
        Ok(podman_cli::list_volumes_by_name(&name_filter)
            .await
            .map_err(|err| {
                RuntimeError::Create(format!("Could not list volume containing config: {}", err))
            })?
            .into_iter()
            .map(|volume_name| {
                volume_name[..volume_name.len() - CONFIG_VOLUME_SUFFIX.len()]
                    .to_string()
                    .try_into() as Result<WorkloadExecutionInstanceName, String>
            })
            .filter_map(|x| match x {
                Ok(value) => Some(value),
                Err(err) => {
                    log::warn!("Could not recreate volume from workload: {}", err);
                    None
                }
            })
            .collect())
    }

    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        _control_interface_path: Option<PathBuf>,
        update_state_tx: StateChangeSender,
    ) -> Result<(PodmanKubeWorkloadId, GenericPollingStateChecker), RuntimeError> {
        let instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(&workload_spec.agent)
            .workload_name(&workload_spec.name)
            .config(&workload_spec.runtime_config)
            .build();

        podman_cli::store_data_as_volume(
            &(instance_name.to_string() + CONFIG_VOLUME_SUFFIX),
            &workload_spec.runtime_config,
        )
        .await
        .unwrap_or_else(|err| {
            log::warn!(
                "Could not store config for '{}' in volume: {}",
                workload_spec.name,
                err
            )
        });

        let created_pods = podman_cli::play_kube(workload_spec.runtime_config.as_bytes())
            .await
            .map_err(RuntimeError::Create)?;

        match serde_json::to_string(&created_pods) {
            Ok(pods_as_json) => {
                podman_cli::store_data_as_volume(
                    &(instance_name.to_string() + PODS_VOLUME_SUFFIX),
                    &pods_as_json,
                )
                .await
            }
            Err(err) => Err(format!("Could not encoded pods as json: {:?}", err)),
        }
        .unwrap_or_else(|err| {
            log::warn!(
                "Could not store pods for '{}' in volume: {}",
                workload_spec.name,
                err
            )
        });

        let workload_id = PodmanKubeWorkloadId {
            name: instance_name,
            pods: Some(created_pods),
            manifest: workload_spec.runtime_config.clone(),
        };

        let state_checker = self
            .start_checker(&workload_id, workload_spec, update_state_tx)
            .await?;

        Ok((workload_id, state_checker))
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<PodmanKubeWorkloadId, RuntimeError> {
        let manifest =
            podman_cli::read_data_from_volume(&(instance_name.to_string() + CONFIG_VOLUME_SUFFIX))
                .await
                .map_err(|err| format!("Could not read config from volume: {:?}", err))
                .map_err(RuntimeError::Create)?;

        let pods =
            podman_cli::read_data_from_volume(&(instance_name.to_string() + PODS_VOLUME_SUFFIX))
                .await
                .map_err(|err| format!("Could not read pods from volume: {:?}", err))
                .and_then(|json| {
                    serde_json::from_str(&json).map_err(|err| {
                        format!("Could not parse pod list read from volume: {:?}", err)
                    })
                });

        let pods = match pods {
            Ok(pods) => Some(pods),
            Err(err) => {
                log::warn!("{}", err);
                None
            }
        };

        Ok(PodmanKubeWorkloadId {
            name: instance_name.clone(),
            pods,
            manifest,
        })
    }

    async fn start_checker(
        &self,
        workload_id: &PodmanKubeWorkloadId,
        workload_spec: WorkloadSpec,
        update_state_tx: StateChangeSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        Ok(GenericPollingStateChecker::start_checker(
            &workload_spec,
            workload_id.clone(),
            update_state_tx,
            PodmanKubeRuntime {},
        ))
    }

    async fn delete_workload(
        &self,
        workload_id: &PodmanKubeWorkloadId,
    ) -> Result<(), RuntimeError> {
        podman_cli::down_kube(workload_id.manifest.as_bytes())
            .map_err(RuntimeError::Delete)
            .await?;
        let _ =
            podman_cli::remove_volume(&(workload_id.name.to_string() + PODS_VOLUME_SUFFIX)).await;
        let _ =
            podman_cli::remove_volume(&(workload_id.name.to_string() + CONFIG_VOLUME_SUFFIX)).await;
        Ok(())
    }
}

#[async_trait]
impl RuntimeStateChecker<PodmanKubeWorkloadId> for PodmanKubeRuntime {
    async fn get_state(&self, id: &PodmanKubeWorkloadId) -> ExecutionState {
        if let Some(pods) = &id.pods {
            let x = podman_cli::list_states_from_pods(pods).await;

            match x {
                Ok(x) => x
                    .into_iter()
                    .map(OrderedExecutionState::from)
                    .fold(OrderedExecutionState::Removed, min)
                    .into(),

                Err(err) => {
                    log::warn!("Could not get state of workload '{}': {}", id.name, err);
                    ExecutionState::ExecUnknown
                }
            }
        } else {
            ExecutionState::ExecUnknown
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum OrderedExecutionState {
    Failed,
    Pending,
    Unknown,
    Running,
    Succeeded,
    Removed,
}

impl From<podman_cli::ContainerState> for OrderedExecutionState {
    fn from(value: podman_cli::ContainerState) -> Self {
        match value {
            podman_cli::ContainerState::Created => OrderedExecutionState::Pending,
            podman_cli::ContainerState::Exited(exit_code) if exit_code == 0 => {
                OrderedExecutionState::Succeeded
            }
            podman_cli::ContainerState::Exited(_) => OrderedExecutionState::Failed,
            podman_cli::ContainerState::Paused => OrderedExecutionState::Unknown,
            podman_cli::ContainerState::Running => OrderedExecutionState::Running,
            podman_cli::ContainerState::Unknown => OrderedExecutionState::Unknown,
        }
    }
}

impl From<OrderedExecutionState> for ExecutionState {
    fn from(value: OrderedExecutionState) -> Self {
        match value {
            OrderedExecutionState::Failed => ExecutionState::ExecFailed,
            OrderedExecutionState::Pending => ExecutionState::ExecPending,
            OrderedExecutionState::Unknown => ExecutionState::ExecUnknown,
            OrderedExecutionState::Running => ExecutionState::ExecRunning,
            OrderedExecutionState::Succeeded => ExecutionState::ExecSucceeded,
            OrderedExecutionState::Removed => ExecutionState::ExecRemoved,
        }
    }
}
