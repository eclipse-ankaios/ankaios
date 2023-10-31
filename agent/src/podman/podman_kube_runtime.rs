use std::{cmp::min, path::PathBuf};

use common::{
    objects::{AgentName, ExecutionState, WorkloadExecutionInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

use async_trait::async_trait;
use futures_util::TryFutureExt;

#[cfg(test)]
use mockall_double::double;

#[cfg_attr(test, double)]
use crate::podman::podman_cli::PodmanCli;
use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    podman::podman_cli,
    runtime::{Runtime, RuntimeError},
    state_checker::{RuntimeStateChecker, StateChecker},
};

use super::podman_kube_runtime_config::PodmanKubeRuntimeConfig;

const CONFIG_VOLUME_SUFFIX: &str = ".config";
const PODS_VOLUME_SUFFIX: &str = ".pods";

#[derive(Debug, Clone)]
pub struct PodmanKubeRuntime {}

#[derive(Debug)]
pub struct PodmanKubeConfig {}

#[derive(Clone, Debug)]
pub struct PodmanKubeWorkloadId {
    // Podman currently does not provide an Id for a created manifest
    // and one needs the complete manifest to tear down the deployed resources.
    pub name: WorkloadExecutionInstanceName,
    pub pods: Option<Vec<String>>,
    pub manifest: String,
    pub down_options: Vec<String>,
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
        Ok(PodmanCli::list_volumes_by_name(&name_filter)
            .await
            .map_err(|err| {
                RuntimeError::Create(format!("Could not list volume containing config: {}", err))
            })?
            .into_iter()
            .map(|volume_name| {
                volume_name[..volume_name.len().saturating_sub(CONFIG_VOLUME_SUFFIX.len())]
                    .to_string()
                    .try_into() as Result<WorkloadExecutionInstanceName, String>
            })
            .filter_map(|x| match x {
                Ok(value) => Some(value),
                Err(err) => {
                    log::warn!("Could not recreate workload from volume: {}", err);
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

        let workload_config =
            PodmanKubeRuntimeConfig::try_from(&workload_spec).map_err(RuntimeError::Create)?;

        PodmanCli::store_data_as_volume(
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

        let created_pods = PodmanCli::play_kube(
            &workload_config.play_options,
            workload_config.manifest.as_bytes(),
        )
        .await
        .map_err(RuntimeError::Create)?;

        match serde_json::to_string(&created_pods) {
            Ok(pods_as_json) => {
                PodmanCli::store_data_as_volume(
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
            manifest: workload_config.manifest,
            down_options: workload_config.down_options,
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
        let runtime_config =
            PodmanCli::read_data_from_volume(&(instance_name.to_string() + CONFIG_VOLUME_SUFFIX))
                .await
                .map_err(|err| format!("Could not read config from volume: {:?}", err))
                .and_then(|json| {
                    serde_yaml::from_str::<PodmanKubeRuntimeConfig>(&json).map_err(|err| {
                        format!("Could not parse config read from volume: {:?}", err)
                    })
                })
                .map_err(RuntimeError::Create)?;
        let pods =
            PodmanCli::read_data_from_volume(&(instance_name.to_string() + PODS_VOLUME_SUFFIX))
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
            manifest: runtime_config.manifest,
            down_options: runtime_config.down_options,
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
        PodmanCli::down_kube(&workload_id.down_options, workload_id.manifest.as_bytes())
            .map_err(RuntimeError::Delete)
            .await?;
        let _ =
            PodmanCli::remove_volume(&(workload_id.name.to_string() + PODS_VOLUME_SUFFIX)).await;
        let _ =
            PodmanCli::remove_volume(&(workload_id.name.to_string() + CONFIG_VOLUME_SUFFIX)).await;
        Ok(())
    }
}

#[async_trait]
impl RuntimeStateChecker<PodmanKubeWorkloadId> for PodmanKubeRuntime {
    async fn get_state(&self, id: &PodmanKubeWorkloadId) -> ExecutionState {
        if let Some(pods) = &id.pods {
            let x = PodmanCli::list_states_from_pods(pods).await;

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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~functions-required-by-runtime-connector~1]
#[cfg(test)]
mod tests {
    use common::objects::{ExecutionState, WorkloadExecutionInstanceName, WorkloadSpec};
    use mockall::predicate::eq;

    use super::PodmanCli;
    use crate::podman::podman_cli::ContainerState;
    use crate::podman::PodmanKubeWorkloadId;
    use crate::runtime::{Runtime, RuntimeError};
    use crate::state_checker::RuntimeStateChecker;
    use crate::{podman::PodmanKubeRuntime, test_helper::MOCKALL_CONTEXT_SYNC};

    const SAMPLE_ERROR: &str = "sample error";
    const SAMPLE_KUBE_CONFIG: &str = "kube_config";
    const SAMPLE_RUNTIME_CONFIG: &str = r#"{"playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], "manifest": "kube_config"}"#;
    const SAMPLE_AGENT: &str = "agent_A";
    const SAMPLE_WORKLOAD_1: &str = "workload_1";

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_volumes_by_name_context();
        context
            .expect()
            .with(eq(".agent_A.config$".to_string()))
            .return_const(Ok(vec![
                "workload_1.hash_1.agent_A.config".into(),
                "workload_2.hash_2.agent_A.config".into(),
            ]));

        let runtime = PodmanKubeRuntime {};

        let agent = "agent_A".into();
        let workloads = runtime.get_reusable_running_workloads(&agent).await;

        assert!(
            matches!(workloads, Ok(res) if res == [WorkloadExecutionInstanceName::new("workload_1.hash_1.agent_A").unwrap(), WorkloadExecutionInstanceName::new("workload_2.hash_2.agent_A").unwrap()])
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_request_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_volumes_by_name_context();
        context
            .expect()
            .with(eq(".agent_A.config$".to_string()))
            .return_const(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};

        let agent = "agent_A".into();
        let workloads = runtime.get_reusable_running_workloads(&agent).await;

        assert!(matches!(workloads, Err(RuntimeError::Create(msg)) if msg.ends_with(SAMPLE_ERROR)));
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_one_volume_cant_be_parsed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_volumes_by_name_context();
        context
            .expect()
            .with(eq(".agent_A.config$".to_string()))
            .return_const(Ok(vec![
                "hash_1.agent_A.config".into(),
                "workload_2.hash_2.agent_A.config".into(),
            ]));
        let runtime = PodmanKubeRuntime {};

        let agent = "agent_A".into();
        let workloads = runtime.get_reusable_running_workloads(&agent).await;

        assert!(
            matches!(workloads, Ok(res) if res == [WorkloadExecutionInstanceName::new("workload_2.hash_2.agent_A").unwrap()])
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_handles_to_short_volume_name() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_volumes_by_name_context();
        context
            .expect()
            .with(eq(".agent_A.config$".to_string()))
            .return_const(Ok(vec![
                "config".into(),
                "workload_2.hash_2.agent_A.config".into(),
            ]));
        let runtime = PodmanKubeRuntime {};

        let agent = "agent_A".into();
        let workloads = runtime.get_reusable_running_workloads(&agent).await;

        assert!(
            matches!(workloads, Ok(res) if res == [WorkloadExecutionInstanceName::new("workload_2.hash_2.agent_A").unwrap()])
        );
    }

    #[tokio::test]
    async fn utest_create_workload_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let store_data_context = PodmanCli::store_data_as_volume_context();
        let play_kube_context = PodmanCli::play_kube_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        store_data_context
            .expect()
            .with(
                eq(format!("{}.config", workload_instance_name)),
                eq(SAMPLE_RUNTIME_CONFIG),
            )
            .once()
            .return_const(Ok(()));

        play_kube_context
            .expect()
            .with(
                eq(["-pl".into(), "--ay".into()]),
                eq(SAMPLE_KUBE_CONFIG.as_bytes()),
            )
            .once()
            .return_const(Ok(vec!["pod1".into(), "pod2".into()]));

        store_data_context
            .expect()
            .with(
                eq(format!("{}.pods", workload_instance_name)),
                eq(r#"["pod1","pod2"]"#),
            )
            .once()
            .return_const(Ok(()));

        let runtime = PodmanKubeRuntime {};

        let workload_spec = WorkloadSpec {
            agent: SAMPLE_AGENT.into(),
            name: SAMPLE_WORKLOAD_1.into(),
            runtime_config: SAMPLE_RUNTIME_CONFIG.into(),
            ..Default::default()
        };

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;
        assert!(matches!(workload, Ok((workload_id, _)) if 
                workload_id.name == workload_instance_name &&
                workload_id.manifest == SAMPLE_KUBE_CONFIG &&
                workload_id.pods == Some(vec!["pod1".into(), "pod2".into()]) &&
                workload_id.down_options == vec!["-do".to_string(), "--wn".to_string()]));
    }

    #[tokio::test]
    async fn utest_create_workload_handle_cant_store_config() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let store_data_context = PodmanCli::store_data_as_volume_context();
        let play_kube_context = PodmanCli::play_kube_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        store_data_context
            .expect()
            .with(
                eq(format!("{}.config", workload_instance_name)),
                eq(SAMPLE_RUNTIME_CONFIG),
            )
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));

        play_kube_context
            .expect()
            .with(
                eq(["-pl".into(), "--ay".into()]),
                eq(SAMPLE_KUBE_CONFIG.as_bytes()),
            )
            .once()
            .return_const(Ok(vec!["pod1".into(), "pod2".into()]));

        store_data_context
            .expect()
            .with(
                eq(format!("{}.pods", workload_instance_name)),
                eq(r#"["pod1","pod2"]"#),
            )
            .once()
            .return_const(Ok(()));

        let runtime = PodmanKubeRuntime {};

        let workload_spec = WorkloadSpec {
            agent: SAMPLE_AGENT.into(),
            name: SAMPLE_WORKLOAD_1.into(),
            runtime_config: SAMPLE_RUNTIME_CONFIG.into(),
            ..Default::default()
        };

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;
        assert!(matches!(workload, Ok((workload_id, _)) if 
                workload_id.name == workload_instance_name &&
                workload_id.manifest == SAMPLE_KUBE_CONFIG &&
                workload_id.pods == Some(vec!["pod1".into(), "pod2".into()]) &&
                workload_id.down_options == vec!["-do".to_string(), "--wn".to_string()]));
    }

    #[tokio::test]
    async fn utest_create_workload_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let store_data_context = PodmanCli::store_data_as_volume_context();
        let play_kube_context = PodmanCli::play_kube_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        store_data_context
            .expect()
            .with(
                eq(format!("{}.config", workload_instance_name)),
                eq(SAMPLE_RUNTIME_CONFIG),
            )
            .once()
            .return_const(Ok(()));

        play_kube_context
            .expect()
            .with(
                eq(["-pl".into(), "--ay".into()]),
                eq(SAMPLE_KUBE_CONFIG.as_bytes()),
            )
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};

        let workload_spec = WorkloadSpec {
            agent: SAMPLE_AGENT.into(),
            name: SAMPLE_WORKLOAD_1.into(),
            runtime_config: SAMPLE_RUNTIME_CONFIG.into(),
            ..Default::default()
        };

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;

        assert!(matches!(workload, Err(RuntimeError::Create(msg)) if msg == SAMPLE_ERROR));
    }

    #[tokio::test]
    async fn utest_get_workload_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let read_data_context = PodmanCli::read_data_from_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        read_data_context
            .expect()
            .with(eq(format!("{}.config", workload_instance_name)))
            .once()
            .return_const(Ok(SAMPLE_RUNTIME_CONFIG.into()));
        read_data_context
            .expect()
            .with(eq(format!("{}.pods", workload_instance_name)))
            .once()
            .return_const(Ok(r#"["pod1","pod2"]"#.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&workload_instance_name).await;

        assert!(matches!(workload, Ok(workload) if
            workload.name == workload_instance_name &&
            workload.pods == Some(vec!["pod1".into(), "pod2".into()]) &&
            workload.manifest == SAMPLE_KUBE_CONFIG &&
            workload.down_options == vec!["-do".to_string(), "--wn".to_string()]
        ));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_read_pods() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let read_data_context = PodmanCli::read_data_from_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        read_data_context
            .expect()
            .with(eq(format!("{}.config", workload_instance_name)))
            .once()
            .return_const(Ok(SAMPLE_RUNTIME_CONFIG.into()));
        read_data_context
            .expect()
            .with(eq(format!("{}.pods", workload_instance_name)))
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&workload_instance_name).await;

        assert!(matches!(workload, Ok(workload) if
            workload.name == workload_instance_name &&
            workload.pods.is_none() &&
            workload.manifest == SAMPLE_KUBE_CONFIG &&
            workload.down_options == vec!["-do".to_string(), "--wn".to_string()]
        ));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_parse_pods() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let read_data_context = PodmanCli::read_data_from_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        read_data_context
            .expect()
            .with(eq(format!("{}.config", workload_instance_name)))
            .once()
            .return_const(Ok(SAMPLE_RUNTIME_CONFIG.into()));
        read_data_context
            .expect()
            .with(eq(format!("{}.pods", workload_instance_name)))
            .once()
            .return_const(Ok(r#"{"#.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&workload_instance_name).await;

        assert!(matches!(workload, Ok(workload) if
            workload.name == workload_instance_name &&
            workload.pods.is_none() &&
            workload.manifest == SAMPLE_KUBE_CONFIG &&
            workload.down_options == vec!["-do".to_string(), "--wn".to_string()]
        ));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_read_config() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let read_data_context = PodmanCli::read_data_from_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        read_data_context
            .expect()
            .with(eq(format!("{}.config", workload_instance_name)))
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&workload_instance_name).await;

        assert!(matches!(workload, Err(..)));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_parse_config() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let read_data_context = PodmanCli::read_data_from_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        read_data_context
            .expect()
            .with(eq(format!("{}.config", workload_instance_name)))
            .once()
            .return_const(Ok("{".into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&workload_instance_name).await;

        assert!(matches!(workload, Err(..)));
    }

    #[tokio::test]
    async fn utest_delete_workload_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let down_kube_context = PodmanCli::down_kube_context();
        let remove_volume_context = PodmanCli::remove_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        down_kube_context
            .expect()
            .with(
                eq(["-do".into(), "--wn".into()]),
                eq(SAMPLE_KUBE_CONFIG.as_bytes()),
            )
            .once()
            .return_const(Ok(()));
        remove_volume_context
            .expect()
            .with(eq(format!("{workload_instance_name}.config")))
            .once()
            .return_const(Ok(()));
        remove_volume_context
            .expect()
            .with(eq(format!("{workload_instance_name}.pods")))
            .once()
            .return_const(Ok(()));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.delete_workload(&workload_id).await;

        assert!(matches!(workload, Ok(())));
    }

    #[tokio::test]
    async fn utest_delete_workload_handles_remove_volume_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let down_kube_context = PodmanCli::down_kube_context();
        let remove_volume_context = PodmanCli::remove_volume_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        down_kube_context
            .expect()
            .with(
                eq(["-do".into(), "--wn".into()]),
                eq(SAMPLE_KUBE_CONFIG.as_bytes()),
            )
            .once()
            .return_const(Ok(()));
        remove_volume_context
            .expect()
            .with(eq(format!("{workload_instance_name}.config")))
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));
        remove_volume_context
            .expect()
            .with(eq(format!("{workload_instance_name}.pods")))
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.delete_workload(&workload_id).await;

        assert!(matches!(workload, Ok(())));
    }

    #[tokio::test]
    async fn utest_delete_workload_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let down_kube_context = PodmanCli::down_kube_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        down_kube_context
            .expect()
            .with(
                eq(["-do".into(), "--wn".into()]),
                eq(SAMPLE_KUBE_CONFIG.as_bytes()),
            )
            .once()
            .return_const(Err(SAMPLE_ERROR.into()));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.delete_workload(&workload_id).await;

        assert!(matches!(workload, Err(..)));
    }

    #[tokio::test]
    async fn utest_get_state_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| {
                Ok(vec![
                    ContainerState::Created,
                    ContainerState::Exited(1),
                    ContainerState::Exited(0),
                    ContainerState::Paused,
                    ContainerState::Running,
                    ContainerState::Unknown,
                ])
            });

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecFailed);
    }

    #[tokio::test]
    async fn utest_get_state_pending() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| {
                Ok(vec![
                    ContainerState::Created,
                    ContainerState::Exited(0),
                    ContainerState::Paused,
                    ContainerState::Running,
                    ContainerState::Unknown,
                ])
            });

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecPending);
    }

    #[tokio::test]
    async fn utest_get_state_unkown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| {
                Ok(vec![
                    ContainerState::Exited(0),
                    ContainerState::Paused,
                    ContainerState::Running,
                    ContainerState::Unknown,
                ])
            });

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecUnknown);
    }

    #[tokio::test]
    async fn utest_get_state_unkown_from_paused() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| {
                Ok(vec![
                    ContainerState::Exited(0),
                    ContainerState::Paused,
                    ContainerState::Running,
                ])
            });

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecUnknown);
    }

    #[tokio::test]
    async fn utest_get_state_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| Ok(vec![ContainerState::Exited(0), ContainerState::Running]));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecRunning);
    }

    #[tokio::test]
    async fn utest_get_state_succeeded() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| Ok(vec![ContainerState::Exited(0)]));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecSucceeded);
    }

    #[tokio::test]
    async fn utest_get_state_removed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| Ok(vec![]));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecRemoved);
    }

    #[tokio::test]
    async fn utest_get_state_unkown_as_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_from_pods_context = PodmanCli::list_states_from_pods_context();

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        list_states_from_pods_context
            .expect()
            .with(eq(["pod1".into(), "pod2".into()]))
            .returning(|_| Err(SAMPLE_ERROR.into()));

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: Some(vec!["pod1".into(), "pod2".into()]),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecUnknown);
    }

    #[tokio::test]
    async fn utest_get_state_unkown_as_pods_unkown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(SAMPLE_AGENT)
            .workload_name(SAMPLE_WORKLOAD_1)
            .config(&SAMPLE_RUNTIME_CONFIG.to_string())
            .build();

        let workload_id = PodmanKubeWorkloadId {
            name: workload_instance_name,
            pods: None,
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: vec!["-do".into(), "--wn".into()],
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::ExecUnknown);
    }
}
