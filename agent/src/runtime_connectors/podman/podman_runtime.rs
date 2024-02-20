use std::path::PathBuf;

use async_trait::async_trait;

use common::{
    objects::{
        AgentName, ExecutionState, WorkloadInstanceName, WorkloadSpec,
    },
    std_extensions::UnreachableOption,
    to_server_interface::ToServerSender,
};

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime_connectors::{RuntimeConnector, RuntimeError, RuntimeStateGetter, StateChecker},
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

impl ToString for PodmanWorkloadId {
    fn to_string(&self) -> String {
        self.id.to_owned()
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
    ) -> Result<Vec<WorkloadInstanceName>, RuntimeError> {
        // [impl->swdd~podman-list-of-existing-workloads-uses-labels~1]
        let res = PodmanCli::list_workload_names_by_label("agent", agent_name.get())
            .await
            .map_err(|err| RuntimeError::List(err.to_string()))?;

        log::debug!("Found {} reusable workload(s): '{:?}'", res.len(), &res);

        Ok(res
            .iter()
            .filter_map(|x| WorkloadInstanceName::new(x))
            .collect())
    }

    // [impl->swdd~podman-create-workload-runs-workload~1]
    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: ToServerSender,
    ) -> Result<(PodmanWorkloadId, GenericPollingStateChecker), RuntimeError> {
        let workload_cfg = PodmanRuntimeConfig::try_from(&workload_spec)
            .map_err(|err| RuntimeError::Create(err.into()))?;

        let workload_id = PodmanCli::podman_run(
            workload_cfg.into(),
            workload_spec.instance_name.to_string().as_str(),
            workload_spec.agent.as_str(),
            control_interface_path,
        )
        .await
        .map_err(RuntimeError::Create)?;

        log::debug!(
            "The workload '{}' has been created with id '{}'",
            workload_spec.name,
            workload_id
        );

        let podman_workload_id = PodmanWorkloadId { id: workload_id };
        let state_checker = self
            .start_checker(&podman_workload_id, workload_spec, update_state_tx)
            .await?;

        // [impl->swdd~podman-create-workload-returns-workload-id~1]
        Ok((podman_workload_id, state_checker))
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
        update_state_tx: ToServerSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        // [impl->swdd~podman-state-getter-reset-cache~1]
        PodmanCli::reset_ps_cache().await;

        log::debug!(
            "Starting the checker for the workload '{}' with id '{}'",
            workload_spec.name,
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

    use common::{
        objects::{AgentName, ExecutionState, WorkloadInstanceName},
        test_utils::generate_test_workload_spec_with_param,
        to_server_interface::ToServer,
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
    async fn utest_get_reusable_running_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::list_workload_names_by_label_context();
        context.expect().return_const(Ok(vec![
            "container1.hash.dummy_agent".to_string(),
            "wrongcontainername".to_string(),
            "container2.hash.dummy_agent".to_string(),
        ]));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("dummy_agent");
        let res = podman_runtime
            .get_reusable_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(res.len(), 2);
        assert_eq!(
            res,
            vec![
                WorkloadInstanceName::new("container1.hash.dummy_agent").unwrap(),
                WorkloadInstanceName::new("container2.hash.dummy_agent").unwrap()
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

    // [utest->swdd~podman-create-workload-runs-workload~1]
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
        let (to_server, _from_agent) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
            .await;

        let (workload_id, _checker) = res.unwrap();

        // [utest->swdd~podman-create-workload-returns-workload-id~1]
        assert_eq!(workload_id.id, "test_id".to_string());
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
        let (to_server, mut from_agent) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
            .await;

        let (_workload_id, _checker) = res.unwrap();

        from_agent.recv().await;
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

    #[tokio::test]
    async fn utest_create_workload_run_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let context = PodmanCli::podman_run_context();
        context
            .expect()
            .return_const(Err("podman run failed".into()));

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );
        let (to_server, _from_agent) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
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

        let (to_server, _from_agent) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
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

        let workload_name = WorkloadInstanceName::new("container1.hash.dummy_agent").unwrap();

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

        let workload_name = WorkloadInstanceName::new("container1.hash.dummy_agent").unwrap();

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

        let workload_name = WorkloadInstanceName::new("container1.hash.dummy_agent").unwrap();

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
