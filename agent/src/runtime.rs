use std::{fmt::Display, path::PathBuf};

use async_trait::async_trait;

use common::{
    objects::{AgentName, WorkloadExecutionInstanceName, WorkloadSpec},
    state_change_interface::StateChangeSender,
};

use crate::state_checker::StateChecker;

#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    Create(String),
    Update(String),
    Delete(String),
    CompleteState(String),
    List(String),
}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Create(msg) => {
                write!(f, "Could not create workload: '{}'", msg)
            }
            RuntimeError::Update(msg) => {
                write!(f, "Could not update workload: '{}'", msg)
            }
            RuntimeError::Delete(msg) => {
                write!(f, "Could not delete workload '{}'", msg)
            }
            RuntimeError::CompleteState(msg) => {
                write!(f, "Could not forward complete state '{}'", msg)
            }
            RuntimeError::List(msg) => {
                write!(f, "Could not get a list of workloads '{}'", msg)
            }
        }
    }
}

// [impl->swdd~functions-required-by-runtime-connector~1]
#[async_trait]
pub trait Runtime<WorkloadId, StChecker>: Sync + Send
where
    StChecker: StateChecker<WorkloadId> + Send + Sync,
    WorkloadId: Send + Sync + 'static,
{
    fn name(&self) -> String;

    async fn get_reusable_running_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError>;

    async fn create_workload(
        &self,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: StateChangeSender,
    ) -> Result<(WorkloadId, StChecker), RuntimeError>;

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
    ) -> Result<WorkloadId, RuntimeError>;

    async fn start_checker(
        &self,
        workload_id: &WorkloadId,
        runtime_workload_config: WorkloadSpec,
        update_state_tx: StateChangeSender,
    ) -> Result<StChecker, RuntimeError>;

    async fn delete_workload(&self, workload_id: &WorkloadId) -> Result<(), RuntimeError>;
}

pub trait OwnableRuntime<WorkloadId, StChecker>: Runtime<WorkloadId, StChecker>
where
    StChecker: StateChecker<WorkloadId> + Send + Sync,
    WorkloadId: Send + Sync + 'static,
{
    fn to_owned(&self) -> Box<dyn Runtime<WorkloadId, StChecker>>;
}

impl<R, WorkloadId, StChecker> OwnableRuntime<WorkloadId, StChecker> for R
where
    R: Runtime<WorkloadId, StChecker> + Clone + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync,
    WorkloadId: Send + Sync + 'static,
{
    fn to_owned(&self) -> Box<dyn Runtime<WorkloadId, StChecker>> {
        Box::new(self.clone())
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
pub mod test {
    use std::{collections::VecDeque, path::PathBuf, sync::Arc};

    use async_trait::async_trait;
    use common::{
        objects::{AgentName, ExecutionState, WorkloadExecutionInstanceName, WorkloadSpec},
        state_change_interface::StateChangeSender,
    };
    use tokio::sync::Mutex;

    use crate::state_checker::{RuntimeStateChecker, StateChecker};

    use super::{Runtime, RuntimeError};

    #[derive(Debug)]
    struct StubRuntimeStateChecker {}

    #[async_trait]
    impl RuntimeStateChecker<String> for StubStateChecker {
        async fn get_state(&self, _workload_id: &String) -> ExecutionState {
            ExecutionState::ExecRunning
        }
    }

    #[derive(Debug)]
    pub struct StubStateChecker {}

    #[async_trait]
    impl StateChecker<String> for StubStateChecker {
        fn start_checker(
            _workload_spec: &WorkloadSpec,
            _workload_id: String,
            _manager_interface: StateChangeSender,
            _state_checker: impl RuntimeStateChecker<String>,
        ) -> Self {
            log::info!("Starting the checker ;)");
            StubStateChecker {}
        }

        async fn stop_checker(self) {
            log::info!("Stopping the checker ;)");
        }
    }

    #[derive(Debug)]
    pub enum RuntimeCall {
        GetReusableWorkloads(
            AgentName,
            Result<Vec<WorkloadExecutionInstanceName>, RuntimeError>,
        ),
        CreateWorkload(
            WorkloadSpec,
            Option<PathBuf>,
            StateChangeSender,
            Result<(String, StubStateChecker), RuntimeError>,
        ),
        GetWorkloadId(WorkloadExecutionInstanceName, Result<String, RuntimeError>),
        StartChecker(
            String,
            WorkloadSpec,
            StateChangeSender,
            Result<StubStateChecker, RuntimeError>,
        ),
        DeleteWorkload(String, Result<(), RuntimeError>),
    }

    #[derive(Debug)]
    struct CallChecker<CallType>
    where
        CallType: std::fmt::Debug,
    {
        pub expected_calls: VecDeque<CallType>,
        pub unexpected_call_count: i8,
    }

    impl<CallType> CallChecker<CallType>
    where
        CallType: std::fmt::Debug,
    {
        pub fn new() -> Self {
            CallChecker {
                expected_calls: VecDeque::new(),
                unexpected_call_count: 0,
            }
        }
    }

    #[derive(Debug)]
    pub struct MockBase<CallType>
    where
        CallType: std::fmt::Debug,
    {
        call_checker: Arc<Mutex<CallChecker<CallType>>>,
    }

    impl<CallType> MockBase<CallType>
    where
        CallType: std::fmt::Debug,
    {
        pub fn new() -> Self {
            MockBase {
                call_checker: Arc::new(Mutex::new(CallChecker::new())),
            }
        }

        pub async fn expect(&mut self, calls: Vec<CallType>) {
            self.call_checker
                .lock()
                .await
                .expected_calls
                .append(&mut VecDeque::from(calls));
        }

        async fn get_expected_call(&self) -> CallType {
            let mut call_checker = self.call_checker.lock().await;
            match call_checker.expected_calls.pop_front() {
                Some(call) => call,
                None => {
                    call_checker.unexpected_call_count += 1;
                    panic!("No more calls expected");
                }
            }
        }

        pub async fn unexpected_call(&self) {
            self.call_checker.lock().await.unexpected_call_count += 1;
        }

        pub async fn assert_all_expectations(&self) {
            let call_checker = self.call_checker.lock().await;

            assert!(
                call_checker.expected_calls.is_empty(),
                "Not all expected calls were done: {:?}",
                call_checker
            );
            assert!(
                0 == call_checker.unexpected_call_count,
                "Received an unexpected amount of calls: '{:?}'",
                call_checker.unexpected_call_count
            );
        }
    }

    // This had to be implemented manually.
    // The auto derived Clone does not understand that CallType doesn't need to be Clone
    impl<CallType> Clone for MockBase<CallType>
    where
        CallType: std::fmt::Debug,
    {
        fn clone(&self) -> Self {
            Self {
                call_checker: self.call_checker.clone(),
            }
        }
    }

    pub type MockRuntime = MockBase<RuntimeCall>;

    #[async_trait]
    impl Runtime<String, StubStateChecker> for MockBase<RuntimeCall> {
        fn name(&self) -> String {
            "mock-runtime".to_string()
        }

        async fn get_reusable_running_workloads(
            &self,
            agent_name: &AgentName,
        ) -> Result<Vec<WorkloadExecutionInstanceName>, RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::GetReusableWorkloads(expected_agent_name, result) => {
                    if expected_agent_name == *agent_name {
                        return result;
                    } else {
                        self.unexpected_call().await;
                        panic!(                            "Expected arguments '{expected_agent_name:?}' do not match the received: '{agent_name:?}'"
                        )
                    }
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Received an unexpected get_reusable_running_workloads call. Expected: '{expected_call:?}'")
                }
            }
        }

        async fn create_workload(
            &self,
            runtime_workload_config: WorkloadSpec,
            control_interface_path: Option<PathBuf>,
            update_state_tx: StateChangeSender,
        ) -> Result<(String, StubStateChecker), RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::CreateWorkload(
                    expected_runtime_workload_config,
                    expected_control_interface_path,
                    expected_update_state_tx,
                    result,
                ) => {
                    if expected_runtime_workload_config == runtime_workload_config
                        && expected_control_interface_path == control_interface_path
                        && expected_update_state_tx.same_channel(&update_state_tx)
                    {
                        return result;
                    } else {
                        self.unexpected_call().await;
                        panic!(
                            "Expected arguments '{expected_runtime_workload_config:?}', '{expected_control_interface_path:?}' or channel do not match the received: '{runtime_workload_config:?}', '{control_interface_path:?}' or channel."
                        );
                    }
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Received an unexpected create_workload call. Expected: '{expected_call:?}'");
                }
            }
        }

        async fn get_workload_id(
            &self,
            instance_name: &WorkloadExecutionInstanceName,
        ) -> Result<String, RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::GetWorkloadId(expected_instance_name, result) => {
                    if expected_instance_name == *instance_name {
                        return result;
                    } else {
                        self.unexpected_call().await;
                        panic!(    "Expected arguments '{expected_instance_name:?}' do not match the received: '{instance_name:?}'"
                        );
                    }
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!(
                    "Received an unexpected get_workload_id call. Expected: '{expected_call:?}'"
                )
                }
            }
        }

        async fn start_checker(
            &self,
            workload_id: &String,
            runtime_workload_config: WorkloadSpec,
            update_state_tx: StateChangeSender,
        ) -> Result<StubStateChecker, RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::StartChecker(
                    expected_workload_id,
                    expected_runtime_workload_config,
                    expected_update_state_tx,
                    result,
                ) => {
                    if expected_workload_id == *workload_id
                        && expected_runtime_workload_config == runtime_workload_config
                        && expected_update_state_tx.same_channel(&update_state_tx)
                    {
                        return result;
                    } else {
                        self.unexpected_call().await;
                        panic!(
                            "Expected arguments '{expected_workload_id:?}', '{expected_runtime_workload_config:?}' or channel do not match the received: '{workload_id:?}', '{runtime_workload_config:?}' or channel."
                        )
                    }
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!(
                        "Received an unexpected start_checker call. Expected: '{expected_call:?}'"
                    )
                }
            }
        }

        async fn delete_workload(&self, workload_id: &String) -> Result<(), RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::DeleteWorkload(expected_workload_id, result) => {
                    if expected_workload_id == *workload_id {
                        return result;
                    } else {
                        self.unexpected_call().await;
                        panic!(
                            "Expected arguments '{expected_workload_id:?}' do not match the received: '{workload_id:?}'"
                        )
                    }
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!(
                    "Received an unexpected delete_workload call. Expected: '{expected_call:?}'"
                )
                }
            }
        }
    }
}
