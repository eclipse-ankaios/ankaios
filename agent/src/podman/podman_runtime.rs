use std::path::PathBuf;

use async_trait::async_trait;

use common::{
    objects::{
        AgentName, ExecutionState, WorkloadExecutionInstanceName, WorkloadInstanceName,
        WorkloadSpec,
    },
    state_change_interface::StateChangeSender,
};

use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime::{Runtime, RuntimeError},
    state_checker::{RuntimeStateChecker, StateChecker},
};

#[cfg(not(test))]
use crate::podman::podman_cli::{
    list_states_by_id, list_workload_ids_by_label, list_workload_names_by_label,
    remove_workloads_by_id, run_workload,
};

#[cfg(test)]
use self::tests::{
    list_states_by_id, list_workload_ids_by_label, list_workload_names_by_label,
    remove_workloads_by_id, run_workload,
};

use super::podman_runtime_config::PodmanRuntimeConfigCli;

#[derive(Debug, Clone)]
pub struct PodmanRuntime {}

#[derive(Debug)]
pub struct PodmanConfig {}

#[derive(Clone, Debug, PartialEq)]
pub struct PodmanWorkloadId {
    pub id: String,
}

struct PodmanStateChecker {}

#[async_trait]
impl RuntimeStateChecker<PodmanWorkloadId> for PodmanStateChecker {
    async fn get_state(&self, workload_id: &PodmanWorkloadId) -> ExecutionState {
        log::trace!("Getting the state for the workload '{}'", workload_id.id);

        let mut exec_state = ExecutionState::ExecUnknown;
        if let Ok(mut states) = list_states_by_id(workload_id.id.as_str()).await {
            match states.len() {
                1 => exec_state = states.swap_remove(0),
                0 => exec_state = ExecutionState::ExecRemoved, // we know that container was removed
                _ => log::error!("Too many matches for the container Id '{:?}'", workload_id),
            }
        }
        log::trace!("Returning the state {}", exec_state);
        exec_state
    }
}

#[async_trait]
impl Runtime<PodmanWorkloadId, GenericPollingStateChecker> for PodmanRuntime {
    fn name(&self) -> String {
        "podman".to_string()
    }

    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError> {
        let res = list_workload_names_by_label("agent", agent_name.get())
            .await
            .map_err(|err| RuntimeError::List(err.to_string()))?;

        log::debug!(
            "get_reusable_running_workloads found {} workload(s): '{:?}'",
            res.len(),
            &res
        );

        Ok(res
            .iter()
            .filter_map(|x| WorkloadExecutionInstanceName::new(x))
            .collect())
    }

    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: StateChangeSender,
    ) -> Result<(PodmanWorkloadId, GenericPollingStateChecker), RuntimeError> {
        let workload_cfg = PodmanRuntimeConfigCli::try_from(&workload_spec)
            .map_err(|err| RuntimeError::Create(err.into()))?;

        let workload_id = run_workload(
            workload_cfg,
            workload_spec.instance_name().to_string().as_str(),
            workload_spec.agent.as_str(),
            control_interface_path,
        )
        .await
        .map_err(RuntimeError::Create)?;

        log::info!(
            "The workload '{}' has been created with id '{}'",
            workload_spec.name,
            workload_id
        );

        let podman_workload_id = PodmanWorkloadId { id: workload_id };
        let state_checker = self
            .start_checker(&podman_workload_id, workload_spec, update_state_tx)
            .await?;

        Ok((podman_workload_id, state_checker))
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<PodmanWorkloadId, RuntimeError> {
        let res = list_workload_ids_by_label("name", instance_name.to_string().as_str())
            .await
            .map_err(|err| RuntimeError::List(err.to_string()))?;

        if let Some(id) = res.first() {
            log::debug!("Found a workload: '{}'", id);
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

    async fn start_checker(
        &self,
        workload_id: &PodmanWorkloadId,
        workload_spec: WorkloadSpec,
        update_state_tx: StateChangeSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        log::info!(
            "Starting the checker for the workload '{}' with id '{}'",
            workload_spec.name,
            workload_id.id
        );
        let podman_state_checker = PodmanStateChecker {};
        let checker = GenericPollingStateChecker::start_checker(
            &workload_spec,
            workload_id.clone(),
            update_state_tx,
            podman_state_checker,
        );
        Ok(checker)
    }

    async fn delete_workload(&self, workload_id: &PodmanWorkloadId) -> Result<(), RuntimeError> {
        log::info!("Deleting workload with id '{}'", workload_id.id);
        remove_workloads_by_id(&workload_id.id)
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Mutex};

    use common::{
        objects::{AgentName, ExecutionState, WorkloadExecutionInstanceName},
        state_change_interface::StateChangeCommand,
        test_utils::generate_test_workload_spec_cli,
    };

    use crate::{
        podman::{
            podman_runtime::PodmanStateChecker, podman_runtime_config::PodmanRuntimeConfigCli,
            PodmanWorkloadId,
        },
        runtime::{Runtime, RuntimeError},
        state_checker::RuntimeStateChecker,
        test_helper::MOCKALL_CONTEXT_SYNC,
    };

    use super::PodmanRuntime;

    const BUFFER_SIZE: usize = 20;

    mockall::lazy_static! {
        pub static ref FAKE_LIST_WORKLOAD_IDS_RESULTS: Mutex<std::collections::VecDeque<Result<Vec<String>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_workload_ids_by_label(_key: &str, _val: &str) -> Result<Vec<String>, String> {
        FAKE_LIST_WORKLOAD_IDS_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    mockall::lazy_static! {
        pub static ref FAKE_LIST_WORKLOAD_NAMES_RESULTS: Mutex<std::collections::VecDeque<Result<Vec<String>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_workload_names_by_label(
        _key: &str,
        _value: &str,
    ) -> Result<Vec<String>, String> {
        FAKE_LIST_WORKLOAD_NAMES_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    mockall::lazy_static! {
        pub static ref FAKE_RUN_WORKLOAD_RESULTS: Mutex<std::collections::VecDeque<Result<String, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn run_workload(
        _workload_cfg: PodmanRuntimeConfigCli,
        _workload_name: &str,
        _agent: &str,
        _control_interface_location: Option<PathBuf>,
    ) -> Result<String, String> {
        FAKE_RUN_WORKLOAD_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    mockall::lazy_static! {
        pub static ref FAKE_LIST_STATES_RESULTS: Mutex<std::collections::VecDeque<Result<Vec<ExecutionState>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_states_by_id(_workload_id: &str) -> Result<Vec<ExecutionState>, String> {
        FAKE_LIST_STATES_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    mockall::lazy_static! {
        pub static ref FAKE_DELETE_WORKLOAD_RESULTS: Mutex<std::collections::VecDeque<Result<(), String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn remove_workloads_by_id(_workload_id: &str) -> Result<(), String> {
        FAKE_DELETE_WORKLOAD_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    #[test]
    fn utest_name_podman() {
        let podman_runtime = PodmanRuntime {};
        assert_eq!(podman_runtime.name(), "podman".to_string());
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_WORKLOAD_NAMES_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(vec![
                "container1.hash.dummy_agent".to_string(),
                "wrongcontainername".to_string(),
                "container2.hash.dummy_agent".to_string(),
            ]));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("dummy_agent");
        let res = podman_runtime
            .get_reusable_running_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(res.len(), 2);
        assert_eq!(
            res,
            vec![
                WorkloadExecutionInstanceName::new("container1.hash.dummy_agent").unwrap(),
                WorkloadExecutionInstanceName::new("container2.hash.dummy_agent").unwrap()
            ]
        );
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_empty_list() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_WORKLOAD_NAMES_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(Vec::new()));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("different_agent");
        let res = podman_runtime
            .get_reusable_running_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(res.len(), 0);
    }

    #[tokio::test]
    async fn utest_get_reusable_running_workloads_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_WORKLOAD_NAMES_RESULTS
            .lock()
            .unwrap()
            .push_back(Err("Simulated error".to_string()));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("dummy_agent");

        assert_eq!(
            podman_runtime
                .get_reusable_running_workloads(&agent_name)
                .await,
            Err(crate::runtime::RuntimeError::List("Simulated error".into()))
        );
    }

    #[tokio::test]
    async fn utest_create_workload_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_RUN_WORKLOAD_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok("test_id".into()));

        let workload_spec = generate_test_workload_spec_cli();
        let (to_server, _from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
            .await;

        let (workload_id, _checker) = res.unwrap();

        assert_eq!(workload_id.id, "test_id".to_string());
    }

    #[tokio::test]
    async fn utest_create_workload_run_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_RUN_WORKLOAD_RESULTS
            .lock()
            .unwrap()
            .push_back(Err("podman run failed".into()));

        let workload_spec = generate_test_workload_spec_cli();
        let (to_server, _from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
            .await;

        assert!(res.is_err_and(|x| { x == RuntimeError::Create("podman run failed".into()) }))
    }

    #[tokio::test]
    async fn utest_create_workload_parsing_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut workload_spec = generate_test_workload_spec_cli();
        workload_spec.runtime_config = "broken runtime config".to_string();

        let (to_server, _from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
            .await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn utest_get_workload_id_workload_found() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_WORKLOAD_IDS_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(vec!["test_workload_id".to_string()]));

        let workload_name =
            WorkloadExecutionInstanceName::new("container1.hash.dummy_agent").unwrap();

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

        FAKE_LIST_WORKLOAD_IDS_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(Vec::new()));

        let workload_name =
            WorkloadExecutionInstanceName::new("container1.hash.dummy_agent").unwrap();

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

        FAKE_LIST_WORKLOAD_IDS_RESULTS
            .lock()
            .unwrap()
            .push_back(Err("simulated error".into()));

        let workload_name =
            WorkloadExecutionInstanceName::new("container1.hash.dummy_agent").unwrap();

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.get_workload_id(&workload_name).await;

        assert_eq!(res, Err(RuntimeError::List("simulated error".to_owned())))
    }

    #[tokio::test]
    async fn utest_get_state_returns_state() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_STATES_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(vec![ExecutionState::ExecRunning]));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };
        let checker: &dyn RuntimeStateChecker<PodmanWorkloadId> = &PodmanStateChecker {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionState::ExecRunning);
    }

    #[tokio::test]
    async fn utest_get_state_returns_empty_list() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_STATES_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(Vec::new()));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };
        let checker: &dyn RuntimeStateChecker<PodmanWorkloadId> = &PodmanStateChecker {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionState::ExecUnknown);
    }

    #[tokio::test]
    async fn utest_get_state_returns_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_STATES_RESULTS
            .lock()
            .unwrap()
            .push_back(Err("simulated error".into()));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };
        let checker: &dyn RuntimeStateChecker<PodmanWorkloadId> = &PodmanStateChecker {};
        let res = checker.get_state(&workload_id).await;
        assert_eq!(res, ExecutionState::ExecUnknown);
    }

    #[tokio::test]
    async fn utest_delete_workload_succeeds() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_DELETE_WORKLOAD_RESULTS
            .lock()
            .unwrap()
            .push_back(Ok(()));

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

        FAKE_DELETE_WORKLOAD_RESULTS
            .lock()
            .unwrap()
            .push_back(Err("simulated error".into()));

        let workload_id = PodmanWorkloadId {
            id: "test_id".into(),
        };

        let podman_runtime = PodmanRuntime {};
        let res = podman_runtime.delete_workload(&workload_id).await;
        assert_eq!(res, Err(RuntimeError::Delete("simulated error".into())));
    }
}
