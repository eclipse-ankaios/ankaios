// Copyright (c) 2023 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Display, path::PathBuf, str::FromStr};

use async_trait::async_trait;

use common::{
    objects::{AgentName, ExecutionState, WorkloadInstanceName, WorkloadSpec},
    std_extensions::UnreachableOption,
};

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime_connectors::{
        podman_cli::PodmanStartConfig, ReusableWorkloadState, RuntimeConnector, RuntimeError,
        RuntimeStateGetter, StateChecker,
    },
    workload_state::WorkloadStateSender,
};

#[cfg(test)]
use mockall_double::double;

// [impl->swdd~podman-uses-podman-cli~1]
#[cfg_attr(test, double)]
use crate::runtime_connectors::podman_cli::PodmanCli;

use super::podman_runtime_config::PodmanRuntimeConfig;

pub const PODMAN_RUNTIME_NAME: &str = "podman";

#[derive(Debug, Clone)]
pub struct PodmanRuntime {}

#[derive(Debug, Clone)]
pub struct PodmanStateGetter {}

#[derive(Clone, Debug, PartialEq)]
pub struct PodmanWorkloadId {
    pub id: String,
}

impl Display for PodmanWorkloadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id.to_owned())
    }
}

impl FromStr for PodmanWorkloadId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(PodmanWorkloadId { id: s.to_string() })
    }
}

#[async_trait]
// [impl->swdd~podman-implements-runtime-state-getter~1]
impl RuntimeStateGetter<PodmanWorkloadId> for PodmanStateGetter {
    async fn get_state(&self, workload_id: &PodmanWorkloadId) -> ExecutionState {
        log::trace!("Getting the state for the workload '{}'", workload_id.id);

        // [impl->swdd~podman-state-getter-returns-unknown-state~1]
        // [impl->swdd~podman-state-getter-uses-podmancli~1]
        // [impl->swdd~podman-state-getter-returns-lost-state~1]
        let exec_state = match PodmanCli::list_states_by_id(workload_id.id.as_str()).await {
            Ok(state) => {
                if let Some(state) = state {
                    state
                } else {
                    ExecutionState::lost()
                }
            }
            Err(err) => {
                log::warn!(
                    "Could not get state of workload '{}': '{}'. Returning unknown.",
                    workload_id.id,
                    err
                );
                ExecutionState::unknown("Error getting state from Podman.")
            }
        };

        log::trace!(
            "Returning the state '{}' for the workload '{}'",
            exec_state,
            workload_id.id
        );
        exec_state
    }
}

impl PodmanRuntime {
    async fn workload_instance_names_to_workload_states(
        &self,
        workload_instance_names: &Vec<WorkloadInstanceName>,
    ) -> Result<Vec<ReusableWorkloadState>, RuntimeError> {
        let mut workload_states = Vec::<ReusableWorkloadState>::default();
        for instance_name in workload_instance_names {
            let workload_id = &self.get_workload_id(instance_name).await?.id;
            match PodmanCli::list_states_by_id(workload_id).await {
                Ok(Some(execution_state)) => workload_states.push(ReusableWorkloadState::new(
                    instance_name.clone(),
                    execution_state,
                    Some(workload_id.to_string()),
                )),
                Ok(None) => {
                    return Err(RuntimeError::List(format!(
                        "Could not get execution state for workload '{}'",
                        instance_name
                    )))
                }
                Err(err) => return Err(RuntimeError::List(err)),
            }
        }
        Ok(workload_states)
    }
}

#[async_trait]
// [impl->swdd~podman-implements-runtime-connector~1]
impl RuntimeConnector<PodmanWorkloadId, GenericPollingStateChecker> for PodmanRuntime {
    // [impl->swdd~podman-name-returns-podman~1]
    fn name(&self) -> String {
        PODMAN_RUNTIME_NAME.to_string()
    }

    async fn get_reusable_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<ReusableWorkloadState>, RuntimeError> {
        // [impl->swdd~podman-list-of-existing-workloads-uses-labels~1]
        let res = PodmanCli::list_workload_names_by_label("agent", agent_name.get())
            .await
            .map_err(|err| RuntimeError::List(err.to_string()))?;

        log::debug!("Found {} reusable workload(s): '{:?}'", res.len(), &res);

        let workload_instance_names: Vec<WorkloadInstanceName> = res
            .iter()
            .filter_map(|x| x.as_str().try_into().ok())
            .collect();

        self.workload_instance_names_to_workload_states(&workload_instance_names)
            .await
    }

    // [impl->swdd~podman-create-workload-runs-workload~2]
    // [impl->swdd~podman-create-workload-starts-existing-workload~1]
    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        reusable_workload_id: Option<PodmanWorkloadId>,
        control_interface_path: Option<PathBuf>,
        update_state_tx: WorkloadStateSender,
    ) -> Result<(PodmanWorkloadId, GenericPollingStateChecker), RuntimeError> {
        let workload_cfg = PodmanRuntimeConfig::try_from(&workload_spec)
            .map_err(|err| RuntimeError::Create(err.into()))?;

        let cli_result = match reusable_workload_id {
            Some(workload_id) => {
                let start_config = PodmanStartConfig {
                    general_options: workload_cfg.general_options,
                    container_id: workload_id.id,
                };
                PodmanCli::podman_start(start_config, &workload_spec.instance_name.to_string())
                    .await
            }
            None => {
                PodmanCli::podman_run(
                    workload_cfg.into(),
                    &workload_spec.instance_name.to_string(),
                    workload_spec.instance_name.agent_name(),
                    control_interface_path,
                )
                .await
            }
        };

        match cli_result {
            Ok(workload_id) => {
                log::debug!(
                    "The workload '{}' has been created with internal id '{}'",
                    workload_spec.instance_name,
                    workload_id
                );

                let podman_workload_id = PodmanWorkloadId { id: workload_id };
                let state_checker = self
                    .start_checker(&podman_workload_id, workload_spec, update_state_tx)
                    .await?;

                // [impl->swdd~podman-create-workload-returns-workload-id~1]
                Ok((podman_workload_id, state_checker))
            }
            Err(err) => {
                // [impl->swdd~podman-create-workload-deletes-failed-container~1]
                log::debug!("Creating/starting container failed, cleaning up. Error: '{err}'");
                match PodmanCli::remove_workloads_by_id(&workload_spec.instance_name.to_string())
                    .await
                {
                    Ok(()) => log::debug!("The broken container has been deleted successfully"),
                    Err(e) => log::warn!(
                        "Failed container cleanup after failed create. Error: '{}'",
                        e
                    ),
                }

                // No matter if we have deleted the broken container or not, we have to report that the "workload create" failed.
                Err(RuntimeError::Create(err))
            }
        }
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadInstanceName,
    ) -> Result<PodmanWorkloadId, RuntimeError> {
        // [impl->swdd~podman-get-workload-id-uses-label~1]
        let res = PodmanCli::list_workload_ids_by_label("name", instance_name.to_string().as_str())
            .await
            .map_err(|err| RuntimeError::List(err.to_string()))?;

        if 1 == res.len() {
            let id = res.first().unwrap_or_unreachable();
            log::debug!("Found an id for workload '{}': '{}'", instance_name, id);
            Ok(PodmanWorkloadId { id: id.to_string() })
        } else {
            log::warn!(
                "get_workload_id returned unexpected number of workloads {:?}",
                res
            );
            Err(RuntimeError::List(
                "Unexpected number of workloads".to_string(),
            ))
        }
    }

    // [impl->swdd~podman-start-checker-starts-podman-state-checker~1]
    async fn start_checker(
        &self,
        workload_id: &PodmanWorkloadId,
        workload_spec: WorkloadSpec,
        update_state_tx: WorkloadStateSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        // [impl->swdd~podman-state-getter-reset-cache~1]
        PodmanCli::reset_ps_cache().await;

        log::debug!(
            "Starting the checker for the workload '{}' with internal id '{}'",
            workload_spec.instance_name,
            workload_id.id
        );
        let checker = GenericPollingStateChecker::start_checker(
            &workload_spec,
            workload_id.clone(),
            update_state_tx,
            PodmanStateGetter {},
        );
        Ok(checker)
    }

    // [impl->swdd~podman-delete-workload-stops-and-removes-workload~1]
    async fn delete_workload(&self, workload_id: &PodmanWorkloadId) -> Result<(), RuntimeError> {
        log::debug!("Deleting workload with id '{}'", workload_id.id);
        PodmanCli::remove_workloads_by_id(&workload_id.id)
            .await
            .map_err(|err| RuntimeError::Delete(err.to_string()))
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
    use std::path::PathBuf;
    use std::str::FromStr;

    use common::objects::{
        generate_test_workload_spec_with_param, AgentName, ExecutionState, WorkloadInstanceName,
    };
    use mockall::Sequence;

    use super::PodmanCli;
    use super::PodmanRuntime;
    use super::{PodmanStateGetter, PodmanWorkloadId, PODMAN_RUNTIME_NAME};
    use crate::runtime_connectors::{RuntimeConnector, RuntimeError, RuntimeStateGetter};
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    const BUFFER_SIZE: usize = 20;

    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";

    // [utest->swdd~podman-name-returns-podman~1]
    #[test]
    fn utest_name_podman() {
        let podman_runtime = PodmanRuntime {};
        assert_eq!(podman_runtime.name(), "podman".to_string());
    }

    // [utest->swdd~podman-list-of-existing-workloads-uses-labels~1]
    #[tokio::test]
    async fn utest_get_reusable_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_workload_names_by_label_context =
            PodmanCli::list_workload_names_by_label_context();
        list_workload_names_by_label_context
            .expect()
            .return_const(Ok(vec![
                "container1.hash.dummy_agent".to_string(),
                "wrongcontainername".to_string(),
                "container2.hash.dummy_agent".to_string(),
            ]));

        let list_workload_ids_by_label_context = PodmanCli::list_workload_ids_by_label_context();
        list_workload_ids_by_label_context
            .expect()
            .return_const(Ok(vec!["container1.hash.dummy_agent".to_string()]));

        let list_states_by_id_context = PodmanCli::list_states_by_id_context();
        list_states_by_id_context
            .expect()
            .return_const(Ok(Some(ExecutionState::initial())));

        let list_states_by_id_context = PodmanCli::list_states_by_id_context();
        list_states_by_id_context
            .expect()
            .return_const(Ok(Some(ExecutionState::initial())));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("dummy_agent");
        let res = podman_runtime
            .get_reusable_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(
            res.iter()
                .map(|x| x.workload_state.instance_name.clone())
                .collect::<Vec<WorkloadInstanceName>>(),
            vec![
                "container1.hash.dummy_agent".try_into().unwrap(),
                "container2.hash.dummy_agent".try_into().unwrap()
            ]
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_empty_list() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_workload_names_by_label_context();
        context.expect().return_const(Ok(Vec::new()));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("different_agent");
        let res = podman_runtime
            .get_reusable_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(res.len(), 0);
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_workload_names_by_label_context();
        context
            .expect()
            .return_const(Err("Simulated error".to_string()));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("dummy_agent");

        assert_eq!(
            podman_runtime.get_reusable_workloads(&agent_name).await,
            Err(crate::runtime_connectors::RuntimeError::List(
                "Simulated error".into()
            ))
        );
    }

    // [utest->swdd~podman-create-workload-runs-workload~2]
    #[tokio::test]
    async fn utest_create_workload_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = PodmanCli::podman_run_context();
        run_context.expect().return_const(Ok("test_id".into()));

        let resest_cache_context = PodmanCli::reset_ps_cache_context();
        resest_cache_context.expect().return_const(());

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(
                workload_spec,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
            )
            .await;

        let (workload_id, _checker) = res.unwrap();

        // [utest->swdd~podman-create-workload-returns-workload-id~1]
        assert_eq!(workload_id.id, "test_id".to_string());
    }

    // [utest->swdd~podman-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_create_workload_with_existing_workload_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let reusable_workload_id = "test_id";

        let start_context = PodmanCli::podman_start_context();
        start_context
            .expect()
            .returning(|start_config, _| Ok(start_config.container_id));

        let resest_cache_context = PodmanCli::reset_ps_cache_context();
        resest_cache_context.expect().return_const(());

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(
                workload_spec,
                Some(PodmanWorkloadId::from_str(reusable_workload_id).unwrap()),
                Some(PathBuf::from("run_folder")),
                state_change_tx,
            )
            .await;

        let (workload_id, _checker) = res.unwrap();

        // [utest->swdd~podman-create-workload-returns-workload-id~1]
        assert_eq!(workload_id.id, reusable_workload_id);
    }

    // [utest->swdd~podman-state-getter-reset-cache~1]
    #[tokio::test]
    async fn utest_state_getter_resets_cache() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = PodmanCli::podman_run_context();
        run_context.expect().return_const(Ok("test_id".into()));

        let mut seq = Sequence::new();

        let resest_cache_context = PodmanCli::reset_ps_cache_context();
        resest_cache_context
            .expect()
            .once()
            .return_const(())
            .in_sequence(&mut seq);

        let list_states_context = PodmanCli::list_states_by_id_context();
        list_states_context
            .expect()
            .once()
            .return_const(Ok(Some(ExecutionState::running())))
            .in_sequence(&mut seq);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        let (state_change_tx, mut state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(
                workload_spec,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
            )
            .await;

        let (_workload_id, _checker) = res.unwrap();

        state_change_rx.recv().await;
    }

    // [utest->swdd~podman-state-getter-uses-podmancli~1]
    #[tokio::test]
    async fn utest_state_getter_uses_podman_cli() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_context = PodmanCli::list_states_by_id_context();
        list_states_context
            .expect()
            .return_const(Ok(Some(ExecutionState::running())));

        let state_getter = PodmanStateGetter {};
        let execution_state = state_getter
            .get_state(&PodmanWorkloadId {
                id: "test_workload_id".into(),
            })
            .await;

        assert_eq!(execution_state, ExecutionState::running());
    }

    // [utest->swdd~podman-create-workload-deletes-failed-container~1]
    #[tokio::test]
    async fn utest_create_workload_run_failed_cleanup_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = PodmanCli::podman_run_context();
        run_context
            .expect()
            .return_const(Err("podman run failed".into()));

        // Workload creation fails, but deleting the broken container succeeded
        let delete_context = PodmanCli::remove_workloads_by_id_context();
        delete_context.expect().return_const(Ok(()));

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(
                workload_spec,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
            )
            .await;

        assert!(res.is_err_and(|x| { x == RuntimeError::Create("podman run failed".into()) }))
    }

    #[tokio::test]
    async fn utest_create_workload_run_failed_cleanup_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = PodmanCli::podman_run_context();
        run_context
            .expect()
            .return_const(Err("podman run failed".into()));

        // Workload creation fails, deleting the broken container failed
        let delete_context = PodmanCli::remove_workloads_by_id_context();
        delete_context
            .expect()
            .return_const(Err("simulated error".into()));

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(
                workload_spec,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
            )
            .await;

        assert!(res.is_err_and(|x| { x == RuntimeError::Create("podman run failed".into()) }))
    }

    #[tokio::test]
    async fn utest_create_workload_parsing_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        workload_spec.runtime_config = "broken runtime config".to_string();

        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(
                workload_spec,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
            )
            .await;

        assert!(res.is_err());
    }

    // [utest->swdd~podman-get-workload-id-uses-label~1]
    #[tokio::test]
    async fn utest_get_workload_id_workload_found() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_workload_ids_by_label_context();
        context
            .expect()
            .return_const(Ok(vec!["test_workload_id".to_string()]));

        let workload_name = "container1.hash.dummy_agent".try_into().unwrap();

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.get_workload_id(&workload_name).await;

        assert_eq!(
            res,
            Ok(PodmanWorkloadId {
                id: "test_workload_id".into()
            })
        )
    }

    #[tokio::test]
    async fn utest_get_workload_id_no_workload_found() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_workload_ids_by_label_context();
        context.expect().return_const(Ok(Vec::new()));

        let workload_name = "container1.hash.dummy_agent".try_into().unwrap();

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.get_workload_id(&workload_name).await;

        assert_eq!(
            res,
            Err(RuntimeError::List(
                "Unexpected number of workloads".to_owned()
            ))
        )
    }

    #[tokio::test]
    async fn utest_get_workload_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_workload_ids_by_label_context();
        context.expect().return_const(Err("simulated error".into()));

        let workload_name = "container1.hash.dummy_agent".try_into().unwrap();

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.get_workload_id(&workload_name).await;

        assert_eq!(res, Err(RuntimeError::List("simulated error".to_owned())))
    }

    // [utest->podman-state-getter-uses-podmancli~1]
    #[tokio::test]
    async fn utest_get_state_returns_state() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_states_by_id_context();
        context
            .expect()
            .return_const(Ok(Some(ExecutionState::running())));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };
        let checker = PodmanStateGetter {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionState::running());
    }

    // [utest->swdd~podman-state-getter-returns-lost-state~1]
    #[tokio::test]
    async fn utest_get_state_returns_lost_on_missing_state() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_states_by_id_context();
        context.expect().return_const(Ok(None));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };
        let checker = PodmanStateGetter {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionState::lost())
    }

    // [utest->swdd~podman-state-getter-returns-unknown-state~1]
    #[tokio::test]
    async fn utest_get_state_returns_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_states_by_id_context();
        context.expect().return_const(Err("simulated error".into()));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };
        let checker = PodmanStateGetter {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(
            res,
            ExecutionState::unknown("Error getting state from Podman.")
        );
    }

    // [utest->swdd~podman-delete-workload-stops-and-removes-workload~1]
    #[tokio::test]
    async fn utest_delete_workload_succeeds() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::remove_workloads_by_id_context();
        context.expect().return_const(Ok(()));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.delete_workload(&workload_id).await;
        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    async fn utest_delete_workload_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::remove_workloads_by_id_context();
        context.expect().return_const(Err("simulated error".into()));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.delete_workload(&workload_id).await;
        assert_eq!(res, Err(RuntimeError::Delete("simulated error".into())));
    }
}
