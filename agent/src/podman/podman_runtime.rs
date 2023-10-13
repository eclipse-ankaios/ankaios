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
use crate::podman::podman_cli::{list_workloads, run_workload};

#[cfg(test)]
use self::tests::{list_workloads, run_workload};

use super::podman_runtime_config::PodmanRuntimeConfigCli;

#[derive(Debug, Clone)]
pub struct PodmanRuntime {}

#[derive(Debug)]
pub struct PodmanConfig {}

#[derive(Clone, Debug)]
pub struct PodmanWorkloadId {
    pub id: String,
}

struct PodmanStateChecker {
    runtime: PodmanRuntime,
}

#[async_trait]
impl RuntimeStateChecker<PodmanWorkloadId> for PodmanStateChecker {
    async fn check_state(&self, instance_name: &PodmanWorkloadId) -> ExecutionState {
        todo!()
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
        let filter_expression = format!(r"name=^\w+\.\w+\.{agent_name}");
        let res = list_workloads(filter_expression.as_str())
            .await
            .map_err(|err| RuntimeError::Update(err.to_string()))?;

        log::debug!(
            "get_reusable_running_workloads found {} workloads: {:?}",
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

        let id = run_workload(
            workload_cfg,
            workload_spec.instance_name().to_string(),
            control_interface_path,
        )
        .await
        .map_err(RuntimeError::Create)?;

        // TODO: store the container id, create a checker (PodmanStateChecker ) and call start_checker

        Ok((
            PodmanWorkloadId { id },
            GenericPollingStateChecker {
                task_handle: tokio::spawn(async {}),
            },
        ))
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<PodmanWorkloadId, RuntimeError> {
        // TODO: Save the arc mutex with mapping between workload id and the workload execution name.
        todo!()
    }

    async fn start_checker(
        &self,
        workload_id: &PodmanWorkloadId,
        workload_spec: WorkloadSpec,
        update_state_tx: StateChangeSender,
    ) -> Result<GenericPollingStateChecker, RuntimeError> {
        todo!()
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
        objects::{AgentName, WorkloadExecutionInstanceName},
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
        pub static ref FAKE_LIST_CONTAINER_MOCK_RESULT_LIST: Mutex<std::collections::VecDeque<Result<Vec<String>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_workloads(_regex: &str) -> Result<Vec<String>, String> {
        FAKE_LIST_CONTAINER_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    pub async fn run_workload(
        _workload_cfg: PodmanRuntimeConfigCli,
        _workload_name: String,
        _control_interface_location: Option<PathBuf>,
    ) -> Result<String, String> {
        Ok("my_id".to_string())
    }

    #[tokio::test]
    async fn get_reusable_running_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        FAKE_LIST_CONTAINER_MOCK_RESULT_LIST
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

        FAKE_LIST_CONTAINER_MOCK_RESULT_LIST
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

        FAKE_LIST_CONTAINER_MOCK_RESULT_LIST
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
}
