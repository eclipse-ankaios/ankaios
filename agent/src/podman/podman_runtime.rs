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
use crate::podman::podman_cli::list_workloads;

#[cfg(test)]
use self::tests::list_workloads;

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
impl RuntimeStateChecker for PodmanStateChecker {
    async fn check_state(&self, instance_name: &WorkloadExecutionInstanceName) -> ExecutionState {
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
        let agent_name_str = agent_name.get();
        log::debug!(
            "Calling get_reusable_running_workloads in '{}' for '{}'",
            self.name(),
            agent_name_str
        );
        let filter_expression = format!(r#"name=^\w+\.\w+\.{agent_name_str}"#);
        let res = list_workloads(filter_expression.as_str())
            .await
            .map_err(|err| RuntimeError::Update(err.to_string()))?;

        let ret = res
            .iter()
            .filter_map(|x| WorkloadExecutionInstanceName::new(x))
            .collect();
        Ok(ret)
    }

    async fn create_workload(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: StateChangeSender,
    ) -> Result<(PodmanWorkloadId, GenericPollingStateChecker), RuntimeError> {
        log::debug!("Calling create_workload in '{}'", self.name());
        Ok((
            PodmanWorkloadId {
                id: "my id".to_string(),
            },
            GenericPollingStateChecker {
                task_handle: tokio::spawn(async {}),
            },
        ))
    }

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<PodmanWorkloadId, RuntimeError> {
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
    use std::sync::Mutex;

    use common::objects::{AgentName, WorkloadExecutionInstanceName, WorkloadInstanceName};

    use crate::runtime::Runtime;

    use super::PodmanRuntime;

    mockall::lazy_static! {
        pub static ref FAKE_READ_TO_STRING_MOCK_RESULT_LIST: Mutex<std::collections::VecDeque<Result<Vec<String>, String>>> =
        Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn list_workloads(_regex: &str) -> Result<Vec<String>, String> {
        FAKE_READ_TO_STRING_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    #[tokio::test]
    async fn get_reusable_running_workloads_success() {
        env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

        FAKE_READ_TO_STRING_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(vec![
                "test.container1.name".to_string(),
                "wrongcontainername".to_string(),
                "test.container2.name".to_string(),
            ]));

        let podman_runtime = PodmanRuntime {};
        let agent_name = AgentName::from("agent_A");
        let res = podman_runtime
            .get_reusable_running_workloads(&agent_name)
            .await
            .unwrap();

        assert_eq!(res.len(), 2);
        assert_eq!(
            res,
            vec![
                WorkloadExecutionInstanceName::new("test.container1.name").unwrap(),
                WorkloadExecutionInstanceName::new("test.container2.name").unwrap()
            ]
        );
    }

    // TODO a test the podman returns an error.
}
