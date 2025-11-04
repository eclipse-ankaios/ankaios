// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use std::{collections::HashMap, fmt::Display, path::PathBuf, str::FromStr};

use async_trait::async_trait;

use api::ank_base::{ExecutionStateInternal, WorkloadInstanceNameInternal, WorkloadNamed};

use common::{objects::AgentName, std_extensions::UnreachableOption};

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime_connectors::{
        ReusableWorkloadState, RuntimeConnector, RuntimeError, RuntimeStateGetter, StateChecker,
        containerd::nerdctl_cli::NerdctlStartConfig, generic_log_fetcher::GenericLogFetcher,
        log_fetcher::LogFetcher, runtime_connector::LogRequestOptions,
    },
    workload_state::WorkloadStateSender,
};

#[cfg(test)]
use mockall_double::double;

// [impl->swdd~containerd-uses-nerdctl-cli~1]
#[cfg_attr(test, double)]
use crate::runtime_connectors::containerd::nerdctl_cli::NerdctlCli;

use super::containerd_runtime_config::ContainerdRuntimeConfig;

pub const CONTAINERD_RUNTIME_NAME: &str = "containerd";

#[derive(Debug, Clone)]
pub struct ContainerdRuntime {}

#[derive(Debug, Clone)]
pub struct ContainerdStateGetter {}

#[derive(Clone, Debug, PartialEq)]
pub struct ContainerdWorkloadId {
    pub id: String,
}

impl Display for ContainerdWorkloadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id.to_owned())
    }
}

impl FromStr for ContainerdWorkloadId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ContainerdWorkloadId { id: s.to_string() })
    }
}

#[async_trait]
// [impl->swdd~containerd-implements-runtime-state-getter~1]
impl RuntimeStateGetter<ContainerdWorkloadId> for ContainerdStateGetter {
    async fn get_state(&self, workload_id: &ContainerdWorkloadId) -> ExecutionStateInternal {
        log::trace!("Getting the state for the workload '{}'", workload_id.id);

        // [impl->swdd~containerd-state-getter-returns-unknown-state~1]
        // [impl->swdd~containerd-state-getter-uses-nerdctlcli~1]
        // [impl->swdd~containerd-state-getter-returns-lost-state~1]
        let exec_state = match NerdctlCli::list_states_by_id(workload_id.id.as_str()).await {
            Ok(state) => {
                if let Some(state) = state {
                    state
                } else {
                    ExecutionStateInternal::lost()
                }
            }
            Err(err) => {
                log::warn!(
                    "Could not get state of workload '{}': '{}'. Returning unknown.",
                    workload_id.id,
                    err
                );
                ExecutionStateInternal::unknown("Error getting state from Nerdctl.")
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

impl ContainerdRuntime {
    async fn sample_workload_states(
        &self,
        workload_instance_names: &Vec<WorkloadInstanceNameInternal>,
    ) -> Result<Vec<ReusableWorkloadState>, RuntimeError> {
        let mut workload_states = Vec::<ReusableWorkloadState>::default();
        for instance_name in workload_instance_names {
            let workload_id = &self.get_workload_id(instance_name).await?.id;
            match NerdctlCli::list_states_by_id(workload_id).await {
                Ok(Some(execution_state)) => workload_states.push(ReusableWorkloadState::new(
                    instance_name.clone(),
                    execution_state,
                    Some(workload_id.to_string()),
                )),
                Ok(None) => {
                    return Err(RuntimeError::List(format!(
                        "Could not get execution state for workload '{instance_name}'"
                    )));
                }
                Err(err) => return Err(RuntimeError::List(err)),
            }
        }
        Ok(workload_states)
    }
}

#[async_trait]
// [impl->swdd~containerd-implements-runtime-connector~1]
impl RuntimeConnector<ContainerdWorkloadId, GenericPollingStateChecker> for ContainerdRuntime {
    // [impl->swdd~containerd-name-returns-containerd~1]
    fn name(&self) -> String {
        CONTAINERD_RUNTIME_NAME.to_string()
    }

    async fn get_reusable_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<ReusableWorkloadState>, RuntimeError> {
        // [impl->swdd~containerd-list-of-existing-workloads-uses-labels~1]
        let res = NerdctlCli::list_workload_names_by_label("agent", agent_name.get())
            .await
            .map_err(|err| RuntimeError::List(err.to_string()))?;

        log::debug!("Found {} reusable workload(s): '{:?}'", res.len(), &res);

        let workload_instance_names: Vec<WorkloadInstanceNameInternal> = res
            .iter()
            .filter_map(|x| x.as_str().try_into().ok())
            .collect();

        self.sample_workload_states(&workload_instance_names).await
    }

    // [impl->swdd~containerd-create-workload-runs-workload~1]
    // [impl->swdd~containerd-create-workload-starts-existing-workload~1]
    async fn create_workload(
        &self,
        workload_named: WorkloadNamed,
        reusable_workload_id: Option<ContainerdWorkloadId>,
        control_interface_path: Option<PathBuf>,
        update_state_tx: WorkloadStateSender,
        workload_file_path_mappings: HashMap<PathBuf, PathBuf>,
    ) -> Result<(ContainerdWorkloadId, GenericPollingStateChecker), RuntimeError> {
        let workload_cfg = ContainerdRuntimeConfig::try_from(&workload_named.workload)
            .map_err(RuntimeError::Unsupported)?;

        let cli_result = match reusable_workload_id {
            Some(workload_id) => {
                let start_config = NerdctlStartConfig {
                    general_options: workload_cfg.general_options,
                    container_id: workload_id.id,
                };
                NerdctlCli::nerdctl_start(start_config, &workload_named.instance_name.to_string())
                    .await
            }
            None => {
                NerdctlCli::nerdctl_run(
                    workload_cfg.into(),
                    &workload_named.instance_name.to_string(),
                    workload_named.instance_name.agent_name(),
                    control_interface_path,
                    workload_file_path_mappings,
                )
                .await
            }
        };

        match cli_result {
            Ok(workload_id) => {
                log::debug!(
                    "The workload '{}' has been created with internal id '{}'",
                    workload_named.instance_name,
                    workload_id
                );

                let nerdctl_workload_id = ContainerdWorkloadId { id: workload_id };
                let state_checker = self
                    .start_checker(&nerdctl_workload_id, workload_named, update_state_tx)
                    .await?;

                // [impl->swdd~containerd-create-workload-returns-workload-id~1]
                Ok((nerdctl_workload_id, state_checker))
            }
            Err(err) => {
                // [impl->swdd~containerd-create-workload-deletes-failed-container~1]
                log::debug!("Creating/starting container failed, cleaning up. Error: '{err}'");
                match NerdctlCli::remove_workloads_by_id(&workload_named.instance_name.to_string())
                    .await
                {
                    Ok(()) => log::debug!("The broken container has been deleted successfully"),
                    Err(e) => {
                        log::warn!("Failed container cleanup after failed create. Error: '{e}'")
                    }
                }

                // No matter if we have deleted the broken container or not, we have to report that the "workload create" failed.
                Err(RuntimeError::Create(err))
            }
        }
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadInstanceNameInternal,
    ) -> Result<ContainerdWorkloadId, RuntimeError> {
        // [impl->swdd~containerd-get-workload-id-uses-label~1]
        let res =
            NerdctlCli::list_container_ids_by_label("name", instance_name.to_string().as_str())
                .await
                .map_err(|err| RuntimeError::List(err.to_string()))?;

        const LENGTH_FOR_VALID_ID: usize = 1;

        if LENGTH_FOR_VALID_ID == res.len() {
            let id = res.first().unwrap_or_unreachable();
            log::debug!("Found an id for workload '{instance_name}': '{id}'");
            Ok(ContainerdWorkloadId { id: id.to_string() })
        } else {
            log::warn!("get_workload_id returned unexpected number of workloads {res:?}");
            Err(RuntimeError::List(
                "Unexpected number of workloads".to_string(),
            ))
        }
    }

    // [impl->swdd~containerd-start-checker-starts-containerd-state-checker~1]
    async fn start_checker(
        &self,
        workload_id: &ContainerdWorkloadId,
        workload_named: WorkloadNamed,
        update_state_tx: WorkloadStateSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        // [impl->swdd~containerd-state-getter-reset-cache~1]
        NerdctlCli::reset_ps_cache().await;

        log::debug!(
            "Starting the checker for the workload '{}' with internal id '{}'",
            workload_named.instance_name,
            workload_id.id
        );
        let checker = GenericPollingStateChecker::start_checker(
            &workload_named,
            workload_id.clone(),
            update_state_tx,
            ContainerdStateGetter {},
        );
        Ok(checker)
    }

    fn get_log_fetcher(
        &self,
        workload_id: ContainerdWorkloadId,
        options: &LogRequestOptions,
    ) -> Result<Box<dyn LogFetcher + Send>, RuntimeError> {
        let nerdctl_log_fetcher =
            super::containerd_log_fetcher::ContainerdLogFetcher::new(&workload_id, options);
        let log_fetcher = GenericLogFetcher::new(nerdctl_log_fetcher);
        Ok(Box::new(log_fetcher))
    }

    // [impl->swdd~containerd-delete-workload-stops-and-removes-workload~1]
    async fn delete_workload(
        &self,
        workload_id: &ContainerdWorkloadId,
    ) -> Result<(), RuntimeError> {
        log::debug!("Deleting workload with id '{}'", workload_id.id);
        NerdctlCli::remove_workloads_by_id(&workload_id.id)
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

// [utest->swdd~agent-functions-required-by-runtime-connector~1]
#[cfg(test)]
mod tests {
    use super::ContainerdRuntime;
    use super::NerdctlCli;
    use super::{ContainerdStateGetter, ContainerdWorkloadId};
    use crate::runtime_connectors::LogRequestOptions;
    use crate::runtime_connectors::containerd::containerd_runtime::CONTAINERD_RUNTIME_NAME;
    use crate::runtime_connectors::{RuntimeConnector, RuntimeError, RuntimeStateGetter};
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    use api::ank_base::{ExecutionStateInternal, WorkloadInstanceNameInternal, WorkloadNamed};
    use api::test_utils::generate_test_workload_with_param;
    use common::objects::AgentName;

    use mockall::Sequence;
    use std::path::PathBuf;
    use std::str::FromStr;

    const BUFFER_SIZE: usize = 20;

    const AGENT_NAME: &str = "agent_x";
    // const WORKLOAD_1_NAME: &str = "workload1";

    // [utest->swdd~containerd-name-returns-containerd~1]
    #[test]
    fn utest_name_containerd() {
        let containerd_runtime = ContainerdRuntime {};
        assert_eq!(containerd_runtime.name(), "containerd".to_string());
    }

    // [utest->swdd~containerd-list-of-existing-workloads-uses-labels~1]
    #[tokio::test]
    async fn utest_get_reusable_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_workload_names_by_label_context =
            NerdctlCli::list_workload_names_by_label_context();
        list_workload_names_by_label_context
            .expect()
            .return_const(Ok(vec![
                "container1.hash.dummy_agent".to_string(),
                "wrongcontainername".to_string(),
                "container2.hash.dummy_agent".to_string(),
            ]));

        let list_container_ids_by_label_context = NerdctlCli::list_container_ids_by_label_context();
        list_container_ids_by_label_context
            .expect()
            .return_const(Ok(vec!["container1.hash.dummy_agent".to_string()]));

        let list_states_by_id_context = NerdctlCli::list_states_by_id_context();
        list_states_by_id_context
            .expect()
            .return_const(Ok(Some(ExecutionStateInternal::initial())));

        let list_states_by_id_context = NerdctlCli::list_states_by_id_context();
        list_states_by_id_context
            .expect()
            .return_const(Ok(Some(ExecutionStateInternal::initial())));

        let containerd_runtime = ContainerdRuntime {};
        let agent_name = AgentName::from("dummy_agent");
        let res = containerd_runtime
            .get_reusable_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(
            res.iter()
                .map(|x| x.workload_state.instance_name.clone())
                .collect::<Vec<WorkloadInstanceNameInternal>>(),
            vec![
                "container1.hash.dummy_agent".try_into().unwrap(),
                "container2.hash.dummy_agent".try_into().unwrap()
            ]
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_empty_list() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_workload_names_by_label_context();
        context.expect().return_const(Ok(Vec::new()));

        let containerd_runtime = ContainerdRuntime {};
        let agent_name = AgentName::from("different_agent");
        let res = containerd_runtime
            .get_reusable_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(res.len(), 0);
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_workload_names_by_label_context();
        context
            .expect()
            .return_const(Err("Simulated error".to_string()));

        let containerd_runtime = ContainerdRuntime {};
        let agent_name = AgentName::from("dummy_agent");

        assert_eq!(
            containerd_runtime.get_reusable_workloads(&agent_name).await,
            Err(crate::runtime_connectors::RuntimeError::List(
                "Simulated error".into()
            ))
        );
    }

    // [utest->swdd~containerd-create-workload-runs-workload~1]
    #[tokio::test]
    async fn utest_create_workload_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = NerdctlCli::nerdctl_run_context();
        run_context.expect().return_const(Ok("test_id".into()));

        let reset_cache_context = NerdctlCli::reset_ps_cache_context();
        reset_cache_context.expect().return_const(());

        let workload_named: WorkloadNamed =
            generate_test_workload_with_param(AGENT_NAME, CONTAINERD_RUNTIME_NAME);
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime
            .create_workload(
                workload_named,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
                Default::default(),
            )
            .await;

        let (workload_id, _checker) = res.unwrap();

        // [utest->swdd~containerd-create-workload-returns-workload-id~1]
        assert_eq!(workload_id.id, "test_id".to_string());
    }

    // [utest->swdd~containerd-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_create_workload_with_existing_workload_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let reusable_workload_id = "test_id";

        let start_context = NerdctlCli::nerdctl_start_context();
        start_context
            .expect()
            .returning(|start_config, _| Ok(start_config.container_id));

        let reset_cache_context = NerdctlCli::reset_ps_cache_context();
        reset_cache_context.expect().return_const(());

        let workload_named = generate_test_workload_with_param(AGENT_NAME, CONTAINERD_RUNTIME_NAME);
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime
            .create_workload(
                workload_named,
                Some(ContainerdWorkloadId::from_str(reusable_workload_id).unwrap()),
                Some(PathBuf::from("run_folder")),
                state_change_tx,
                Default::default(),
            )
            .await;

        let (workload_id, _checker) = res.unwrap();

        // [utest->swdd~containerd-create-workload-returns-workload-id~1]
        assert_eq!(workload_id.id, reusable_workload_id);
    }

    // [utest->swdd~containerd-state-getter-reset-cache~1]
    #[tokio::test]
    async fn utest_state_getter_resets_cache() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = NerdctlCli::nerdctl_run_context();
        run_context.expect().return_const(Ok("test_id".into()));

        let mut seq = Sequence::new();

        let reset_cache_context = NerdctlCli::reset_ps_cache_context();
        reset_cache_context
            .expect()
            .once()
            .return_const(())
            .in_sequence(&mut seq);

        let list_states_context = NerdctlCli::list_states_by_id_context();
        list_states_context
            .expect()
            .once()
            .return_const(Ok(Some(ExecutionStateInternal::running())))
            .in_sequence(&mut seq);

        let workload_named = generate_test_workload_with_param(AGENT_NAME, CONTAINERD_RUNTIME_NAME);
        let (state_change_tx, mut state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime
            .create_workload(
                workload_named,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
                Default::default(),
            )
            .await;

        let (_workload_id, _checker) = res.unwrap();

        state_change_rx.recv().await;
    }

    // [utest->swdd~containerd-state-getter-uses-nerdctlcli~1]
    #[tokio::test]
    async fn utest_state_getter_uses_nerdctl_cli() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let list_states_context = NerdctlCli::list_states_by_id_context();
        list_states_context
            .expect()
            .return_const(Ok(Some(ExecutionStateInternal::running())));

        let state_getter = ContainerdStateGetter {};
        let execution_state = state_getter
            .get_state(&ContainerdWorkloadId {
                id: "test_workload_id".into(),
            })
            .await;

        assert_eq!(execution_state, ExecutionStateInternal::running());
    }

    // [utest->swdd~containerd-create-workload-deletes-failed-container~1]
    #[tokio::test]
    async fn utest_create_workload_run_failed_cleanup_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = NerdctlCli::nerdctl_run_context();
        run_context
            .expect()
            .return_const(Err("nerdctl run failed".into()));

        // Workload creation fails, but deleting the broken container succeeded
        let delete_context = NerdctlCli::remove_workloads_by_id_context();
        delete_context.expect().return_const(Ok(()));

        let workload_named = generate_test_workload_with_param(AGENT_NAME, CONTAINERD_RUNTIME_NAME);
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime
            .create_workload(
                workload_named,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
                Default::default(),
            )
            .await;

        assert!(res.is_err_and(|x| { x == RuntimeError::Create("nerdctl run failed".into()) }))
    }

    // [utest->swdd~containerd-create-workload-deletes-failed-container~1]
    #[tokio::test]
    async fn utest_create_workload_run_failed_cleanup_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let run_context = NerdctlCli::nerdctl_run_context();
        run_context
            .expect()
            .return_const(Err("nerdctl run failed".into()));

        // Workload creation fails, deleting the broken container failed
        let delete_context = NerdctlCli::remove_workloads_by_id_context();
        delete_context
            .expect()
            .return_const(Err("simulated error".into()));

        let workload_named = generate_test_workload_with_param(AGENT_NAME, CONTAINERD_RUNTIME_NAME);
        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime
            .create_workload(
                workload_named,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
                Default::default(),
            )
            .await;

        assert!(res.is_err_and(|x| { x == RuntimeError::Create("nerdctl run failed".into()) }))
    }

    #[tokio::test]
    async fn utest_create_workload_parsing_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut workload_named: WorkloadNamed =
            generate_test_workload_with_param(AGENT_NAME, CONTAINERD_RUNTIME_NAME);
        workload_named.workload.runtime_config = "broken runtime config".to_string();

        let (state_change_tx, _state_change_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime
            .create_workload(
                workload_named,
                None,
                Some(PathBuf::from("run_folder")),
                state_change_tx,
                Default::default(),
            )
            .await;

        assert!(res.is_err());
    }

    // [utest->swdd~containerd-get-workload-id-uses-label~1]
    #[tokio::test]
    async fn utest_get_workload_id_workload_found() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_container_ids_by_label_context();
        context
            .expect()
            .return_const(Ok(vec!["test_workload_id".to_string()]));

        let workload_name = "container1.hash.dummy_agent".try_into().unwrap();

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime.get_workload_id(&workload_name).await;

        assert_eq!(
            res,
            Ok(ContainerdWorkloadId {
                id: "test_workload_id".into()
            })
        )
    }

    #[tokio::test]
    async fn utest_get_workload_id_no_workload_found() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_container_ids_by_label_context();
        context.expect().return_const(Ok(Vec::new()));

        let workload_name = "container1.hash.dummy_agent".try_into().unwrap();

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime.get_workload_id(&workload_name).await;

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

        let context = NerdctlCli::list_container_ids_by_label_context();
        context.expect().return_const(Err("simulated error".into()));

        let workload_name = "container1.hash.dummy_agent".try_into().unwrap();

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime.get_workload_id(&workload_name).await;

        assert_eq!(res, Err(RuntimeError::List("simulated error".to_owned())))
    }

    // [utest->nerdctl-state-getter-uses-nerdctlcli~1]
    #[tokio::test]
    async fn utest_get_state_returns_state() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_states_by_id_context();
        context
            .expect()
            .return_const(Ok(Some(ExecutionStateInternal::running())));

        let workload_id = ContainerdWorkloadId {
            id: "test_id".into(),
        };
        let checker = ContainerdStateGetter {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionStateInternal::running());
    }

    // [utest->swdd~containerd-state-getter-returns-lost-state~1]
    #[tokio::test]
    async fn utest_get_state_returns_lost_on_missing_state() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_states_by_id_context();
        context.expect().return_const(Ok(None));

        let workload_id = ContainerdWorkloadId {
            id: "test_id".into(),
        };
        let checker = ContainerdStateGetter {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionStateInternal::lost())
    }

    // [utest->swdd~containerd-state-getter-returns-unknown-state~1]
    #[tokio::test]
    async fn utest_get_state_returns_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::list_states_by_id_context();
        context.expect().return_const(Err("simulated error".into()));

        let workload_id = ContainerdWorkloadId {
            id: "test_id".into(),
        };
        let checker = ContainerdStateGetter {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(
            res,
            ExecutionStateInternal::unknown("Error getting state from Nerdctl.")
        );
    }

    // [utest->swdd~containerd-delete-workload-stops-and-removes-workload~1]
    #[tokio::test]
    async fn utest_delete_workload_succeeds() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::remove_workloads_by_id_context();
        context.expect().return_const(Ok(()));

        let workload_id = ContainerdWorkloadId {
            id: "test_id".into(),
        };

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime.delete_workload(&workload_id).await;
        assert_eq!(res, Ok(()));
    }

    // [utest->swdd~containerd-delete-workload-stops-and-removes-workload~1]
    #[tokio::test]
    async fn utest_delete_workload_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = NerdctlCli::remove_workloads_by_id_context();
        context.expect().return_const(Err("simulated error".into()));

        let workload_id = ContainerdWorkloadId {
            id: "test_id".into(),
        };

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime.delete_workload(&workload_id).await;
        assert_eq!(res, Err(RuntimeError::Delete("simulated error".into())));
    }

    #[tokio::test]
    async fn utest_get_log_fetcher() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let workload_id = ContainerdWorkloadId {
            id: "test_id".into(),
        };

        let log_request = LogRequestOptions {
            follow: false,
            since: None,
            until: None,
            tail: None,
        };

        let containerd_runtime = ContainerdRuntime {};
        let res = containerd_runtime.get_log_fetcher(workload_id, &log_request);
        assert!(res.is_ok());
    }
}
