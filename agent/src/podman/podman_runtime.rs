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
    list_all_workloads_by_label, list_running_workloads_by_label, list_states_by_id, run_workload,
};

#[cfg(test)]
use self::tests::{
    list_all_workloads_by_label, list_running_workloads_by_label, list_states_by_id, run_workload,
};

use super::podman_runtime_config::PodmanRuntimeConfigCli;

#[derive(Debug, Clone)]
pub struct PodmanRuntime {}

#[derive(Debug)]
pub struct PodmanConfig {}

#[derive(Clone, Debug)]
pub struct PodmanWorkloadId {
    pub id: String,
}

struct PodmanStateChecker {}

#[async_trait]
impl RuntimeStateChecker<PodmanWorkloadId> for PodmanStateChecker {
    // This seems to be rather get_state() and not check_state()
    async fn check_state(&self, workload_id: &PodmanWorkloadId) -> ExecutionState {
        log::trace!("Checking the state for the workload '{}'", workload_id.id);

        let mut exec_state = ExecutionState::ExecUnknown;
        if let Ok(states) = list_states_by_id(workload_id.id.as_str()).await {
            if let Some(state) = states.first() {
                exec_state = state.clone();
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
        let res = list_running_workloads_by_label("agent", agent_name.get())
            .await
            .map_err(|err| RuntimeError::Update(err.to_string()))?;

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
        let res =
            list_all_workloads_by_label("name", instance_name.to_string().as_str(), r"{{.ID}}")
                .await
                .map_err(|err| RuntimeError::Update(err.to_string()))?;

        if let Some(id) = res.get(0) {
            log::debug!("Found a workload: '{}'", id);
            Ok(PodmanWorkloadId { id: id.to_string() })
        } else {
            log::warn!(
                "get_workload_id returned unexpected number of workloads {:?}",
                res
            );
            Err(RuntimeError::Update(
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
        log::debug!(
            "Starting the checker for the workload '{}' with id={}",
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
        todo!()
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
        podman::podman_runtime_config::PodmanRuntimeConfigCli, runtime::Runtime,
        test_helper::MOCKALL_CONTEXT_SYNC,
    };

    use super::PodmanRuntime;

    const BUFFER_SIZE: usize = 20;

    mockall::lazy_static! {
        pub static ref FAKE_LIST_RUNNING_WORKLOADS_RESULTS: Mutex<std::collections::VecDeque<Result<Vec<String>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_running_workloads_by_label(
        _key: &str,
        _value: &str,
    ) -> Result<Vec<String>, String> {
        FAKE_LIST_RUNNING_WORKLOADS_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    mockall::lazy_static! {
        pub static ref FAKE_LIST_ALL_WORKLOADS_RESULTS: Mutex<std::collections::VecDeque<Result<Vec<String>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_all_workloads_by_label(
        _key: &str,
        _val: &str,
        _result_format: &str,
    ) -> Result<Vec<String>, String> {
        FAKE_LIST_ALL_WORKLOADS_RESULTS
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    pub async fn run_workload(
        _workload_cfg: PodmanRuntimeConfigCli,
        _workload_name: &str,
        _agent: &str,
        _control_interface_location: Option<PathBuf>,
    ) -> Result<String, String> {
        Ok("my_id".to_string())
    }

    pub async fn list_states_by_id(_workload_id: &str) -> Result<Vec<ExecutionState>, String> {
        Ok(Vec::new())
    }

    #[tokio::test]
    async fn get_reusable_running_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_RUNNING_WORKLOADS_RESULTS
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
    async fn get_reusable_running_workloads_empty_list() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_RUNNING_WORKLOADS_RESULTS
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
    async fn get_reusable_running_workloads_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_RUNNING_WORKLOADS_RESULTS
            .lock()
            .unwrap()
            .push_back(Err("Simulated error".to_string()));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("dummy_agent");

        assert!(podman_runtime
            .get_reusable_running_workloads(&agent_name)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn create_container_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let workload_spec = generate_test_workload_spec_cli();
        let (to_server, _from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let podman_runtime = PodmanRuntime {};
        let _res = podman_runtime
            .create_workload(workload_spec, Some(PathBuf::from("run_folder")), to_server)
            .await;

        // TODO: cover whole function
    }

    // TODO: tests of get_workload_id (success, error)
}
