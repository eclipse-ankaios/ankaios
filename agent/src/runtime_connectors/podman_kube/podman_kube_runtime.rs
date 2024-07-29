use std::{cmp::min, fmt::Display, path::PathBuf};

use common::objects::{
    AgentName, ExecutionState, WorkloadInstanceName, WorkloadSpec, WorkloadState,
};

use async_trait::async_trait;
use futures_util::TryFutureExt;

#[cfg(test)]
use mockall_double::double;

// [impl->swdd~podman-kube-uses-podman-cli~1]
#[cfg_attr(test, double)]
use crate::runtime_connectors::podman_cli::PodmanCli;
use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime_connectors::{
        podman_cli, RuntimeConnector, RuntimeError, RuntimeStateGetter, StateChecker,
    },
    workload_state::WorkloadStateSender,
};

use super::podman_kube_runtime_config::PodmanKubeRuntimeConfig;

pub const PODMAN_KUBE_RUNTIME_NAME: &str = "podman-kube";
const CONFIG_VOLUME_SUFFIX: &str = ".config";
const PODS_VOLUME_SUFFIX: &str = ".pods";

#[derive(Debug, Clone)]
pub struct PodmanKubeRuntime {}

// [impl->swdd~podman-kube-workload-id]
#[derive(Clone, Debug)]
pub struct PodmanKubeWorkloadId {
    // Podman currently does not provide an Id for a created manifest
    // and one needs the complete manifest to tear down the deployed resources.
    pub name: WorkloadInstanceName,
    pub pods: Option<Vec<String>>,
    pub manifest: String,
    pub down_options: Vec<String>,
}

impl Display for PodmanKubeWorkloadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pods) = &self.pods {
            write!(f, "{}", sha256::digest(pods.join("")))
        } else {
            Ok(())
        }
    }
}

impl PodmanKubeRuntime {
    async fn workload_instance_names_to_workload_states(
        &self,
        workload_instance_names: &Vec<WorkloadInstanceName>,
    ) -> Result<Vec<WorkloadState>, RuntimeError> {
        let mut workload_states = Vec::<WorkloadState>::default();
        for instance_name in workload_instance_names {
            let execution_state = self
                .get_state(&self.get_workload_id(instance_name).await?)
                .await;
            workload_states.push(WorkloadState {
                instance_name: instance_name.clone(),
                execution_state,
            });
        }
        Ok(workload_states)
    }
}

#[async_trait]
// [impl->swdd~podman-kube-implements-runtime-connector~1]
impl RuntimeConnector<PodmanKubeWorkloadId, GenericPollingStateChecker> for PodmanKubeRuntime {
    // [impl->swdd~podman-kube-name-returns-podman-kube~1]
    fn name(&self) -> String {
        PODMAN_KUBE_RUNTIME_NAME.to_string()
    }

    // [impl->swdd~podman-kube-list-existing-workloads-using-config-volumes~1]
    async fn get_reusable_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadState>, RuntimeError> {
        let name_filter = format!(
            "{}{}$",
            agent_name.get_filter_suffix(),
            CONFIG_VOLUME_SUFFIX
        );
        let workload_instance_names: Vec<WorkloadInstanceName> =
            PodmanCli::list_volumes_by_name(&name_filter)
                .await
                .map_err(|err| {
                    RuntimeError::Create(format!(
                        "Could not list volume containing config: '{}'",
                        err
                    ))
                })?
                .into_iter()
                .map(|volume_name| {
                    volume_name[..volume_name.len().saturating_sub(CONFIG_VOLUME_SUFFIX.len())]
                        .to_string()
                        .try_into() as Result<WorkloadInstanceName, String>
                })
                .filter_map(|x| match x {
                    Ok(value) => Some(value),
                    Err(err) => {
                        log::warn!("Could not recreate workload from volume: '{}'", err);
                        None
                    }
                })
                .collect();

        self.workload_instance_names_to_workload_states(&workload_instance_names)
            .await
    }

    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        _control_interface_path: Option<PathBuf>,
        update_state_tx: WorkloadStateSender,
    ) -> Result<(PodmanKubeWorkloadId, GenericPollingStateChecker), RuntimeError> {
        let instance_name = workload_spec.instance_name.clone();

        let workload_config =
            PodmanKubeRuntimeConfig::try_from(&workload_spec).map_err(RuntimeError::Create)?;

        // [impl->swdd~podman-kube-create-workload-creates-config-volume~1]
        // [impl->swdd~podman-kube-create-continues-if-cannot-create-volume~1]
        PodmanCli::store_data_as_volume(
            &(instance_name.to_string() + CONFIG_VOLUME_SUFFIX),
            &workload_spec.runtime_config,
        )
        .await
        .unwrap_or_else(|err| {
            log::warn!(
                "Could not store config for '{}' in volume: '{}'",
                workload_spec.instance_name,
                err
            )
        });

        // [impl->swdd~podman-kube-create-workload-apply-manifest~1]
        let created_pods = PodmanCli::play_kube(
            &workload_config.general_options,
            &workload_config.play_options,
            workload_config.manifest.as_bytes(),
        )
        .await
        .map_err(RuntimeError::Create)?;

        // [impl->swdd~podman-kube-create-workload-creates-pods-volume~1]
        // [impl->swdd~podman-kube-create-continues-if-cannot-create-volume~1]
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
                "Could not store pods for '{}' in volume: '{}'",
                workload_spec.instance_name,
                err
            )
        });

        let workload_id = PodmanKubeWorkloadId {
            name: instance_name,
            pods: Some(created_pods),
            manifest: workload_config.manifest,
            down_options: workload_config.down_options,
        };

        log::debug!(
            "The workload '{}' has been created.",
            workload_spec.instance_name,
        );

        // [impl->swdd~podman-kube-create-starts-podman-kube-state-getter~1]
        let state_checker = self
            .start_checker(&workload_id, workload_spec, update_state_tx)
            .await?;

        // [impl->swdd~podman-kube-create-workload-returns-workload-id~1]
        Ok((workload_id, state_checker))
    }

    // [impl->swdd~podman-kube-get-workload-id-uses-volumes~1]
    async fn get_workload_id(
        &self,
        instance_name: &WorkloadInstanceName,
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
        update_state_tx: WorkloadStateSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        // [impl->swdd~podman-kube-state-getter-reset-cache~1]
        PodmanCli::reset_ps_cache().await;
        log::debug!(
            "Starting the checker for the workload '{}'.",
            workload_spec.instance_name,
        );
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
        log::debug!(
            "Deleting workload with workload execution instance name '{}'",
            workload_id.name
        );

        // [impl->swdd~podman-kube-delete-workload-downs-manifest-file~1]
        PodmanCli::down_kube(&workload_id.down_options, workload_id.manifest.as_bytes())
            .map_err(RuntimeError::Delete)
            .await?;
        // [impl->swdd~podman-kube-delete-removes-volumes~1]
        PodmanCli::remove_volume(&(workload_id.name.to_string() + PODS_VOLUME_SUFFIX))
            .await
            .unwrap_or_else(|err| log::warn!("Could not remove pods volume: '{}'", err));
        // [impl->swdd~podman-kube-delete-removes-volumes~1]

        PodmanCli::remove_volume(&(workload_id.name.to_string() + CONFIG_VOLUME_SUFFIX))
            .await
            .unwrap_or_else(|err| log::warn!("Could not remove configs volume: '{}'", err));
        Ok(())
    }
}

#[async_trait]
// [impl->swdd~podman-kube-implements-runtime-state-getter~1]
impl RuntimeStateGetter<PodmanKubeWorkloadId> for PodmanKubeRuntime {
    async fn get_state(&self, id: &PodmanKubeWorkloadId) -> ExecutionState {
        log::trace!("Getting the state for the workload '{}'", id.name);
        if let Some(pods) = &id.pods {
            // [impl->swdd~podman-kube-state-getter-uses-container-states~1]
            match PodmanCli::list_states_from_pods(pods).await {
                // [impl->swdd~podman-kube-state-getter-removed-if-no-container~1]
                // [impl->swdd~podman-kube-state-getter-combines-states~2]
                Ok(container_states) => {
                    log::trace!(
                        "Received following states for workload '{}': '{:?}'",
                        id.name,
                        container_states
                    );
                    container_states
                        .into_iter()
                        .map(OrderedExecutionState::from)
                        .fold(OrderedExecutionState::Lost, min)
                        .into()
                }

                Err(err) => {
                    log::warn!("Could not get state of workload '{}': {}", id.name, err);
                    ExecutionState::unknown("Error getting state from pods.")
                }
            }
        } else {
            log::warn!("No pods in the workload '{}'", id.name.workload_name());
            ExecutionState::succeeded()
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]

// [impl->swdd~podman-kube-state-getter-removed-if-no-container~1]
enum OrderedExecutionState {
    Failed(String),
    Starting,
    Unknown,
    Running,
    Stopping,
    Succeeded,
    Lost,
}

// [impl->swdd~podman-kube-state-getter-maps-state~2]
impl From<podman_cli::ContainerState> for OrderedExecutionState {
    fn from(value: podman_cli::ContainerState) -> Self {
        match value {
            podman_cli::ContainerState::Starting => OrderedExecutionState::Starting,
            podman_cli::ContainerState::Exited(0) => OrderedExecutionState::Succeeded,
            podman_cli::ContainerState::Exited(value) => {
                OrderedExecutionState::Failed(format!("Exit code: '{value}'"))
            }
            podman_cli::ContainerState::Paused => OrderedExecutionState::Unknown,
            podman_cli::ContainerState::Running => OrderedExecutionState::Running,
            podman_cli::ContainerState::Stopping => OrderedExecutionState::Stopping,
            podman_cli::ContainerState::Unknown => OrderedExecutionState::Unknown,
        }
    }
}

// [impl->swdd~podman-kube-state-getter-maps-state~2]
impl From<OrderedExecutionState> for ExecutionState {
    fn from(value: OrderedExecutionState) -> Self {
        match value {
            OrderedExecutionState::Failed(value) => ExecutionState::failed(value),
            OrderedExecutionState::Starting => ExecutionState::starting("starting container"),
            OrderedExecutionState::Unknown => ExecutionState::unknown("unknown container state"),
            OrderedExecutionState::Running => ExecutionState::running(),
            OrderedExecutionState::Stopping => ExecutionState::stopping("stopping container"),
            OrderedExecutionState::Succeeded => ExecutionState::succeeded(),
            OrderedExecutionState::Lost => ExecutionState::lost(),
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
    use common::objects::{
        generate_test_workload_spec_with_param, generate_test_workload_spec_with_runtime_config,
    };
    use mockall::Sequence;

    use std::fmt::Display;

    use common::objects::{ExecutionState, WorkloadInstanceName};
    use mockall::{lazy_static, predicate::eq};

    use super::PodmanCli;
    use crate::runtime_connectors::podman_cli::__mock_MockPodmanCli as podman_cli_mock;
    use crate::runtime_connectors::{podman_cli::ContainerState, RuntimeConnector, RuntimeError};

    use super::{
        PodmanKubeRuntime, PodmanKubeWorkloadId, CONFIG_VOLUME_SUFFIX, PODMAN_KUBE_RUNTIME_NAME,
        PODS_VOLUME_SUFFIX,
    };
    use crate::runtime_connectors::RuntimeStateGetter;
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    const SAMPLE_ERROR: &str = "sample error";
    const SAMPLE_KUBE_CONFIG: &str = "kube_config";
    const SAMPLE_RUNTIME_CONFIG: &str = r#"{"generalOptions": ["-gen", "--eral"], "playOptions": ["-pl", "--ay"], "downOptions": ["-do", "--wn"], "manifest": "kube_config"}"#;
    const SAMPLE_AGENT: &str = "agent_A";
    const SAMPLE_WORKLOAD_1: &str = "workload_1";

    lazy_static! {
        pub static ref WORKLOAD_INSTANCE_NAME: WorkloadInstanceName =
            WorkloadInstanceName::builder()
                .agent_name(SAMPLE_AGENT)
                .workload_name(SAMPLE_WORKLOAD_1)
                .config(&SAMPLE_RUNTIME_CONFIG.to_string())
                .build();
        pub static ref SAMPLE_POD_LIST: Vec<String> = vec!["pod1".to_string(), "pod2".to_string()];
        pub static ref SAMPLE_GENERAL_OPTIONS: Vec<String> =
            vec!["-gen".to_string(), "--eral".to_string()];
        pub static ref SAMPLE_PLAY_OPTIONS: Vec<String> =
            vec!["-pl".to_string(), "--ay".to_string()];
        pub static ref SAMPLE_DOWN_OPTIONS: Vec<String> =
            vec!["-do".to_string(), "--wn".to_string()];
        pub static ref WORKLOAD_ID: PodmanKubeWorkloadId = PodmanKubeWorkloadId {
            name: WORKLOAD_INSTANCE_NAME.clone(),
            pods: Some(SAMPLE_POD_LIST.clone()),
            manifest: SAMPLE_KUBE_CONFIG.into(),
            down_options: SAMPLE_DOWN_OPTIONS.clone(),
        };
    }

    // [utest->swdd~podman-kube-name-returns-podman-kube~1]
    #[test]
    fn utest_name_podman_kube() {
        let runtime = PodmanKubeRuntime {};
        assert_eq!(runtime.name(), "podman-kube");
    }

    // [utest->swdd~podman-kube-list-existing-workloads-using-config-volumes~1]
    #[tokio::test]
    async fn utest_get_reusable_workloads_success() {
        let workload_instance_1 = "workload_1.hash_1.agent_A";
        let workload_instance_2 = "workload_2.hash_2.agent_A";

        let mock_context = MockContext::new().await;
        mock_context.list_agent_config_volumes_returns(Ok(vec![
            workload_instance_1.as_config_volume(),
            workload_instance_2.as_config_volume(),
        ]));

        let mut workload_spec = generate_test_workload_spec_with_param(
            "agent_A".to_string(),
            "workload_2".to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = SAMPLE_RUNTIME_CONFIG.to_string();

        mock_context
            .read_data
            .expect()
            .return_const(Ok(workload_spec.runtime_config));

        let runtime = PodmanKubeRuntime {};

        let workloads = runtime.get_reusable_workloads(&SAMPLE_AGENT.into()).await;

        assert!(
            matches!(workloads, Ok(res) if res.iter().map(|x| x.instance_name.clone()).collect::<Vec<WorkloadInstanceName>>() == [workload_instance_1.try_into().unwrap(), workload_instance_2.try_into().unwrap()])
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_request_fails() {
        let mock_context = MockContext::new().await;
        mock_context.list_agent_config_volumes_returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};

        let workloads = runtime.get_reusable_workloads(&SAMPLE_AGENT.into()).await;

        assert!(matches!(workloads, Err(RuntimeError::Create(msg)) if msg.contains(SAMPLE_ERROR)));
    }

    #[tokio::test]
    async fn utest_get_reusable_workloads_one_volume_cant_be_parsed() {
        let invalid_workload_instance = "hash_1.agent_A";
        let workload_instance = "workload_2.hash_2.agent_A";

        let mut workload_spec = generate_test_workload_spec_with_param(
            "agent_A".to_string(),
            "workload_2".to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = SAMPLE_RUNTIME_CONFIG.to_string();

        let mock_context = MockContext::new().await;
        mock_context.list_agent_config_volumes_returns(Ok(vec![
            invalid_workload_instance.as_config_volume(),
            workload_instance.as_config_volume(),
        ]));

        mock_context
            .read_data
            .expect()
            .return_const(Ok(workload_spec.runtime_config));

        let runtime = PodmanKubeRuntime {};

        let workloads = runtime.get_reusable_workloads(&SAMPLE_AGENT.into()).await;

        assert!(
            matches!(workloads, Ok(res) if res.iter().map(|x| x.instance_name.clone()).collect::<Vec<WorkloadInstanceName>>() == [workload_instance.try_into().unwrap()])
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_workloads_handles_to_short_volume_name() {
        let workload_instance = "workload_2.hash_2.agent_A";

        let mut workload_spec = generate_test_workload_spec_with_param(
            "agent_A".to_string(),
            "workload_2".to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = SAMPLE_RUNTIME_CONFIG.to_string();

        let mock_context = MockContext::new().await;
        mock_context.list_agent_config_volumes_returns(Ok(vec![
            "config".into(),
            workload_instance.as_config_volume(),
        ]));

        mock_context
            .read_data
            .expect()
            .return_const(Ok(workload_spec.runtime_config));

        // let list_states_by_id_context = PodmanCli::list_states_by_id_context();
        // list_states_by_id_context
        mock_context
            .list_states_from_pods
            .expect()
            .return_const(Ok(vec![ContainerState::Unknown]));

        let runtime = PodmanKubeRuntime {};

        let workloads = runtime.get_reusable_workloads(&SAMPLE_AGENT.into()).await;
        println!("{:?}", workloads);

        assert!(
            matches!(workloads, Ok(res) if res.iter().map(|x| x.instance_name.clone()).collect::<Vec<WorkloadInstanceName>>() == [workload_instance.try_into().unwrap()])
        );
    }

    #[tokio::test]
    async fn utest_create_workload_success() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-create-workload-creates-config-volume~1]
        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_config_volume(),
                SAMPLE_RUNTIME_CONFIG,
            )
            .returns(Ok(()));

        // [utest->swdd~podman-kube-create-workload-apply-manifest~1]
        mock_context
            .play_kube(
                &*SAMPLE_GENERAL_OPTIONS,
                &*SAMPLE_PLAY_OPTIONS,
                SAMPLE_KUBE_CONFIG,
            )
            .returns(Ok(SAMPLE_POD_LIST.clone()));

        // [utest->swdd~podman-kube-create-workload-creates-pods-volume~1]
        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_pods_volume(),
                r#"["pod1","pod2"]"#,
            )
            .returns(Ok(()));

        mock_context.reset_ps_cache.expect().return_const(());

        let runtime = PodmanKubeRuntime {};

        let workload_spec = generate_test_workload_spec_with_runtime_config(
            SAMPLE_AGENT.to_string(),
            SAMPLE_WORKLOAD_1.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            SAMPLE_RUNTIME_CONFIG.to_string(),
        );

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;
        // [utest->swdd~podman-kube-create-workload-returns-workload-id~1]
        assert!(matches!(workload, Ok((workload_id, _)) if
                workload_id.name == *WORKLOAD_INSTANCE_NAME &&
                workload_id.manifest == SAMPLE_KUBE_CONFIG &&
                workload_id.pods == Some(SAMPLE_POD_LIST.clone()) &&
                workload_id.down_options == *SAMPLE_DOWN_OPTIONS));
    }

    // [utest->swdd~podman-kube-create-continues-if-cannot-create-volume~1]
    #[tokio::test]
    async fn utest_create_workload_handle_cant_store_config() {
        let mock_context = MockContext::new().await;

        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_config_volume(),
                SAMPLE_RUNTIME_CONFIG,
            )
            .returns(Err(SAMPLE_ERROR.into()));

        mock_context
            .play_kube(
                &*SAMPLE_GENERAL_OPTIONS,
                &*SAMPLE_PLAY_OPTIONS,
                SAMPLE_KUBE_CONFIG,
            )
            .returns(Ok(SAMPLE_POD_LIST.clone()));

        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_pods_volume(),
                r#"["pod1","pod2"]"#,
            )
            .returns(Ok(()));

        mock_context.reset_ps_cache.expect().return_const(());

        let runtime = PodmanKubeRuntime {};

        let workload_spec = generate_test_workload_spec_with_runtime_config(
            SAMPLE_AGENT.to_string(),
            SAMPLE_WORKLOAD_1.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            SAMPLE_RUNTIME_CONFIG.to_string(),
        );

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;
        assert!(matches!(workload, Ok((workload_id, _)) if
                workload_id.name == *WORKLOAD_INSTANCE_NAME &&
                workload_id.manifest == SAMPLE_KUBE_CONFIG &&
                workload_id.pods == Some(SAMPLE_POD_LIST.clone()) &&
                workload_id.down_options == *SAMPLE_DOWN_OPTIONS));
    }

    // [utest->swdd~podman-kube-create-continues-if-cannot-create-volume~1]
    #[tokio::test]
    async fn utest_create_workload_handle_cant_store_pods() {
        let mock_context = MockContext::new().await;

        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_config_volume(),
                SAMPLE_RUNTIME_CONFIG,
            )
            .returns(Ok(()));

        mock_context
            .play_kube(
                &*SAMPLE_GENERAL_OPTIONS,
                &*SAMPLE_PLAY_OPTIONS,
                SAMPLE_KUBE_CONFIG,
            )
            .returns(Ok(SAMPLE_POD_LIST.clone()));

        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_pods_volume(),
                r#"["pod1","pod2"]"#,
            )
            .returns(Err(SAMPLE_ERROR.into()));

        mock_context.reset_ps_cache.expect().return_const(());

        let runtime = PodmanKubeRuntime {};

        let workload_spec = generate_test_workload_spec_with_runtime_config(
            SAMPLE_AGENT.to_string(),
            SAMPLE_WORKLOAD_1.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            SAMPLE_RUNTIME_CONFIG.to_string(),
        );

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;
        assert!(matches!(workload, Ok((workload_id, _)) if
                workload_id.name == *WORKLOAD_INSTANCE_NAME &&
                workload_id.manifest == SAMPLE_KUBE_CONFIG &&
                workload_id.pods == Some(SAMPLE_POD_LIST.clone()) &&
                workload_id.down_options == *SAMPLE_DOWN_OPTIONS));
    }

    // [utest->swdd~podman-kube-state-getter-reset-cache~1]
    #[tokio::test]
    async fn utest_state_getter_resets_cache() {
        let mock_context = MockContext::new().await;

        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_config_volume(),
                SAMPLE_RUNTIME_CONFIG,
            )
            .returns(Ok(()));

        mock_context
            .play_kube(
                &*SAMPLE_GENERAL_OPTIONS,
                &*SAMPLE_PLAY_OPTIONS,
                SAMPLE_KUBE_CONFIG,
            )
            .returns(Ok(SAMPLE_POD_LIST.clone()));

        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_pods_volume(),
                r#"["pod1","pod2"]"#,
            )
            .returns(Err(SAMPLE_ERROR.into()));

        let mut seq = Sequence::new();

        mock_context
            .reset_ps_cache
            .expect()
            .once()
            .return_const(())
            .in_sequence(&mut seq);
        mock_context
            .list_states_from_pods
            .expect()
            .once()
            .with(eq(SAMPLE_POD_LIST.clone()))
            .return_const(Ok(vec![ContainerState::Running]))
            .in_sequence(&mut seq);

        let runtime = PodmanKubeRuntime {};

        let workload_spec = generate_test_workload_spec_with_runtime_config(
            SAMPLE_AGENT.to_string(),
            SAMPLE_WORKLOAD_1.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            SAMPLE_RUNTIME_CONFIG.to_string(),
        );

        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let _workload = runtime.create_workload(workload_spec, None, sender).await;

        receiver.recv().await;
    }

    #[tokio::test]
    async fn utest_create_workload_command_fails() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-create-workload-creates-config-volume~1]
        mock_context
            .store_data(
                WORKLOAD_INSTANCE_NAME.as_config_volume(),
                SAMPLE_RUNTIME_CONFIG,
            )
            .returns(Ok(()));

        mock_context
            .play_kube(
                &*SAMPLE_GENERAL_OPTIONS,
                &*SAMPLE_PLAY_OPTIONS,
                SAMPLE_KUBE_CONFIG,
            )
            .returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};

        let workload_spec = generate_test_workload_spec_with_runtime_config(
            SAMPLE_AGENT.to_string(),
            SAMPLE_WORKLOAD_1.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
            SAMPLE_RUNTIME_CONFIG.to_string(),
        );

        let (sender, _) = tokio::sync::mpsc::channel(1);
        let workload = runtime.create_workload(workload_spec, None, sender).await;

        assert!(matches!(workload, Err(RuntimeError::Create(msg)) if msg == SAMPLE_ERROR));
    }

    // [utest->swdd~podman-kube-get-workload-id-uses-volumes~1]
    #[tokio::test]
    async fn utest_get_workload_id_success() {
        let mock_context = MockContext::new().await;

        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Ok(SAMPLE_RUNTIME_CONFIG.into()));
        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_pods_volume())
            .returns(Ok(r#"["pod1","pod2"]"#.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&WORKLOAD_INSTANCE_NAME).await;

        assert!(matches!(workload, Ok(workload) if
            workload.name == *WORKLOAD_INSTANCE_NAME &&
            workload.pods == Some(SAMPLE_POD_LIST.clone()) &&
            workload.manifest == SAMPLE_KUBE_CONFIG &&
            workload.down_options == *SAMPLE_DOWN_OPTIONS
        ));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_read_pods() {
        let mock_context = MockContext::new().await;

        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Ok(SAMPLE_RUNTIME_CONFIG.into()));
        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_pods_volume())
            .returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&WORKLOAD_INSTANCE_NAME).await;

        assert!(matches!(workload, Ok(workload) if
            workload.name == *WORKLOAD_INSTANCE_NAME &&
            workload.pods.is_none() &&
            workload.manifest == SAMPLE_KUBE_CONFIG &&
            workload.down_options == *SAMPLE_DOWN_OPTIONS
        ));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_parse_pods() {
        let mock_context = MockContext::new().await;

        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Ok(SAMPLE_RUNTIME_CONFIG.into()));
        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_pods_volume())
            .returns(Ok(r#"{"#.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&WORKLOAD_INSTANCE_NAME).await;

        assert!(matches!(workload, Ok(workload) if
            workload.name == *WORKLOAD_INSTANCE_NAME &&
            workload.pods.is_none() &&
            workload.manifest == SAMPLE_KUBE_CONFIG &&
            workload.down_options == *SAMPLE_DOWN_OPTIONS
        ));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_read_config() {
        let mock_context = MockContext::new().await;

        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&WORKLOAD_INSTANCE_NAME).await;

        assert!(matches!(workload, Err(..)));
    }

    #[tokio::test]
    async fn utest_get_workload_id_could_not_parse_config() {
        let mock_context = MockContext::new().await;

        mock_context
            .read_data(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Ok("{".into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.get_workload_id(&WORKLOAD_INSTANCE_NAME).await;

        assert!(matches!(workload, Err(..)));
    }

    #[tokio::test]
    async fn utest_delete_workload_success() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-delete-workload-downs-manifest-file~1]
        mock_context
            .down_kube(&*SAMPLE_DOWN_OPTIONS, SAMPLE_KUBE_CONFIG)
            .returns(Ok(()));
        // [utest->swdd~podman-kube-delete-removes-volumes~1]
        mock_context
            .remove_volume(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Ok(()));
        // [utest->swdd~podman-kube-delete-removes-volumes~1]
        mock_context
            .remove_volume(WORKLOAD_INSTANCE_NAME.as_pods_volume())
            .returns(Ok(()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.delete_workload(&WORKLOAD_ID).await;

        assert!(matches!(workload, Ok(())));
    }

    #[tokio::test]
    async fn utest_delete_workload_handles_remove_volume_fails() {
        let mock_context = MockContext::new().await;

        mock_context
            .down_kube(&*SAMPLE_DOWN_OPTIONS, SAMPLE_KUBE_CONFIG)
            .returns(Ok(()));
        mock_context
            .remove_volume(WORKLOAD_INSTANCE_NAME.as_config_volume())
            .returns(Err(SAMPLE_ERROR.into()));
        mock_context
            .remove_volume(WORKLOAD_INSTANCE_NAME.as_pods_volume())
            .returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.delete_workload(&WORKLOAD_ID).await;

        assert!(matches!(workload, Ok(())));
    }

    #[tokio::test]
    async fn utest_delete_workload_fails() {
        let mock_context = MockContext::new().await;

        mock_context
            .down_kube(&*SAMPLE_DOWN_OPTIONS, SAMPLE_KUBE_CONFIG)
            .returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let workload = runtime.delete_workload(&WORKLOAD_ID).await;

        assert!(matches!(workload, Err(..)));
    }

    // [utest->swdd~podman-kube-state-getter-maps-state~2]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_failed() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![
                ContainerState::Starting,
                ContainerState::Exited(1),
                ContainerState::Exited(0),
                ContainerState::Paused,
                ContainerState::Running,
                ContainerState::Unknown,
                ContainerState::Stopping,
            ]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(execution_state, ExecutionState::failed("Exit code: '1'"));
    }

    // [utest->swdd~podman-kube-state-getter-maps-state~2]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_starting() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![
                ContainerState::Starting,
                ContainerState::Exited(0),
                ContainerState::Paused,
                ContainerState::Running,
                ContainerState::Unknown,
                ContainerState::Stopping,
            ]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(
            execution_state,
            ExecutionState::starting("starting container")
        );
    }

    // [utest->swdd~podman-kube-state-getter-maps-state~2]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_unknown() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![
                ContainerState::Exited(0),
                ContainerState::Paused,
                ContainerState::Running,
                ContainerState::Unknown,
            ]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(
            execution_state,
            ExecutionState::unknown("unknown container state")
        );
    }

    // [utest->swdd~podman-kube-state-getter-maps-state~2]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_unknown_from_paused() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![
                ContainerState::Exited(0),
                ContainerState::Paused,
                ContainerState::Running,
            ]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(
            execution_state,
            ExecutionState::unknown("unknown container state")
        );
    }

    // [utest->swdd~podman-kube-state-getter-maps-state~2]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_running() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![ContainerState::Exited(0), ContainerState::Running]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(execution_state, ExecutionState::running());
    }

    // [utest->swdd~podman-kube-state-getter-maps-state~2]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_succeeded() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![ContainerState::Exited(0)]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(execution_state, ExecutionState::succeeded());
    }

    // [utest->swdd~podman-kube-state-getter-removed-if-no-container~1]
    // [utest->swdd~podman-kube-state-getter-combines-states~2]
    #[tokio::test]
    async fn utest_get_state_removed() {
        let mock_context = MockContext::new().await;

        // [utest->swdd~podman-kube-state-getter-uses-container-states~1]
        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Ok(vec![]));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(execution_state, ExecutionState::lost())
    }

    #[tokio::test]
    async fn utest_get_state_unknown_as_command_fails() {
        let mock_context = MockContext::new().await;

        mock_context
            .list_states_from_pods(&*SAMPLE_POD_LIST)
            .returns(Err(SAMPLE_ERROR.into()));

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&WORKLOAD_ID).await;

        assert_eq!(
            execution_state,
            ExecutionState::unknown("Error getting state from pods.")
        );
    }

    #[tokio::test]
    async fn utest_get_state_unknown_as_pods_unknown() {
        let workload_id = PodmanKubeWorkloadId {
            pods: None,
            ..WORKLOAD_ID.clone()
        };

        let runtime = PodmanKubeRuntime {};
        let execution_state = runtime.get_state(&workload_id).await;

        assert_eq!(execution_state, ExecutionState::succeeded());
    }

    struct MockContext<'a> {
        list_volumes_by_name: podman_cli_mock::__list_volumes_by_name::Context,
        store_data: podman_cli_mock::__store_data_as_volume::Context,
        play_kube: podman_cli_mock::__play_kube::Context,
        read_data: podman_cli_mock::__read_data_from_volume::Context,
        down_kube: podman_cli_mock::__down_kube::Context,
        remove_volume: podman_cli_mock::__remove_volume::Context,
        list_states_from_pods: podman_cli_mock::__list_states_from_pods::Context,
        reset_ps_cache: podman_cli_mock::__reset_ps_cache::Context,
        _guard: tokio::sync::MutexGuard<'a, ()>, // The guard shall be dropped last
    }

    impl<'a> MockContext<'a> {
        async fn new() -> MockContext<'a> {
            Self {
                list_volumes_by_name: PodmanCli::list_volumes_by_name_context(),
                store_data: PodmanCli::store_data_as_volume_context(),
                play_kube: PodmanCli::play_kube_context(),
                read_data: PodmanCli::read_data_from_volume_context(),
                down_kube: PodmanCli::down_kube_context(),
                remove_volume: PodmanCli::remove_volume_context(),
                list_states_from_pods: PodmanCli::list_states_from_pods_context(),
                reset_ps_cache: PodmanCli::reset_ps_cache_context(),
                _guard: MOCKALL_CONTEXT_SYNC.get_lock_async().await,
            }
        }

        fn list_agent_config_volumes_returns(&self, volumes: Result<Vec<String>, String>) {
            self.list_volumes_by_name
                .expect()
                .with(eq(".agent_A.config$".to_string()))
                .once()
                .return_const(volumes);
        }

        fn store_data(
            &self,
            volume_name: String,
            data: impl Into<String>,
        ) -> ReturnsStruct<impl FnOnce(Result<(), String>) + '_> {
            let store_data = &self.store_data;
            let data = data.into();
            ReturnsStruct {
                function: |result| {
                    store_data
                        .expect()
                        .with(eq(volume_name), eq(data))
                        .once()
                        .return_const(result);
                },
            }
        }

        fn play_kube(
            &self,
            general_options: impl std::iter::IntoIterator<Item = impl ToString>,
            additional_options: impl std::iter::IntoIterator<Item = impl ToString>,
            kube_yml: impl ToString,
        ) -> ReturnsStruct<impl FnOnce(Result<Vec<String>, String>) + '_> {
            let general_options: Vec<String> =
                general_options.into_iter().map(|x| x.to_string()).collect();
            let additional_options: Vec<String> = additional_options
                .into_iter()
                .map(|x| x.to_string())
                .collect();
            let kube_yml = kube_yml.to_string().as_bytes().to_vec();
            let play_kube = &self.play_kube;
            ReturnsStruct {
                function: |result| {
                    play_kube
                        .expect()
                        .with(eq(general_options), eq(additional_options), eq(kube_yml))
                        .once()
                        .return_const(result);
                },
            }
        }

        fn read_data(
            &self,
            volume_name: impl ToString,
        ) -> ReturnsStruct<impl FnOnce(Result<String, String>) + '_> {
            let read_data = &self.read_data;
            let volume_name = volume_name.to_string();
            ReturnsStruct {
                function: |result| {
                    read_data
                        .expect()
                        .with(eq(volume_name))
                        .once()
                        .return_const(result);
                },
            }
        }

        fn down_kube(
            &self,
            additional_options: impl std::iter::IntoIterator<Item = impl ToString>,
            kube_yml: impl ToString,
        ) -> ReturnsStruct<impl FnOnce(Result<(), String>) + '_> {
            let down_kube = &self.down_kube;
            let additional_options: Vec<String> = additional_options
                .into_iter()
                .map(|x| x.to_string())
                .collect();
            let kube_yml = kube_yml.to_string().as_bytes().to_vec();
            ReturnsStruct {
                function: |result| {
                    down_kube
                        .expect()
                        .with(eq(additional_options), eq(kube_yml))
                        .once()
                        .return_const(result);
                },
            }
        }

        fn remove_volume(
            &self,
            volume_name: String,
        ) -> ReturnsStruct<impl FnOnce(Result<(), String>) + '_> {
            let remove_volume = &self.remove_volume;
            ReturnsStruct {
                function: |result| {
                    remove_volume
                        .expect()
                        .with(eq(volume_name))
                        .once()
                        .return_const(result);
                },
            }
        }

        fn list_states_from_pods(
            &self,
            pods: impl IntoIterator<Item = impl ToString>,
        ) -> ReturnsStruct<impl FnOnce(Result<Vec<ContainerState>, String>) + '_> {
            let list_states_from_pods = &self.list_states_from_pods;
            let pods: Vec<String> = pods.into_iter().map(|x| x.to_string()).collect();
            ReturnsStruct {
                function: |result| {
                    list_states_from_pods
                        .expect()
                        .with(eq(pods))
                        .once()
                        .return_once(|_| result);
                },
            }
        }
    }

    struct ReturnsStruct<F> {
        function: F,
    }

    impl<F> ReturnsStruct<F> {
        fn returns<T>(self, result: T)
        where
            F: FnOnce(T),
        {
            (self.function)(result);
        }
    }

    trait AsVolumeName {
        fn as_config_volume(&self) -> String;
        fn as_pods_volume(&self) -> String;
    }

    impl<T: Display> AsVolumeName for T {
        fn as_config_volume(&self) -> String {
            format!("{self}{}", CONFIG_VOLUME_SUFFIX)
        }
        fn as_pods_volume(&self) -> String {
            format!("{self}{}", PODS_VOLUME_SUFFIX)
        }
    }
}
