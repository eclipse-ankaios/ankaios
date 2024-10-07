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

use std::{fmt::Display, path::PathBuf};

use async_trait::async_trait;

use common::objects::{AgentName, WorkloadInstanceName, WorkloadSpec, WorkloadState};

use crate::{runtime_connectors::StateChecker, workload_state::WorkloadStateSender};

#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    Create(String),
    Delete(String),
    List(String),
}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Create(msg) => {
                write!(f, "{}", msg)
            }
            RuntimeError::Delete(msg) => {
                write!(f, "{}", msg)
            }
            RuntimeError::List(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

// [impl->swdd~functions-required-by-runtime-connector~1]
#[async_trait]
pub trait RuntimeConnector<WorkloadId, StChecker>: Sync + Send
where
    StChecker: StateChecker<WorkloadId> + Send + Sync,
    WorkloadId: ToString + Send + Sync + 'static,
{
    fn name(&self) -> String;

    async fn get_reusable_workloads(
        &self,
        agent_name: &AgentName,
    ) -> Result<Vec<WorkloadState>, RuntimeError>;

    async fn create_workload(
        &self,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        update_state_tx: WorkloadStateSender,
    ) -> Result<(WorkloadId, StChecker), RuntimeError>;

    async fn get_workload_id(
        &self,
        instance_name: &WorkloadInstanceName,
    ) -> Result<WorkloadId, RuntimeError>;

    async fn start_checker(
        &self,
        workload_id: &WorkloadId,
        runtime_workload_config: WorkloadSpec,
        update_state_tx: WorkloadStateSender,
    ) -> Result<StChecker, RuntimeError>;

    async fn delete_workload(&self, workload_id: &WorkloadId) -> Result<(), RuntimeError>;
}

pub trait OwnableRuntime<WorkloadId, StChecker>: RuntimeConnector<WorkloadId, StChecker>
where
    StChecker: StateChecker<WorkloadId> + Send + Sync,
    WorkloadId: ToString + Send + Sync + 'static,
{
    fn to_owned(&self) -> Box<dyn RuntimeConnector<WorkloadId, StChecker>>;
}

impl<R, WorkloadId, StChecker> OwnableRuntime<WorkloadId, StChecker> for R
where
    R: RuntimeConnector<WorkloadId, StChecker> + Clone + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync,
    WorkloadId: ToString + Send + Sync + 'static,
{
    fn to_owned(&self) -> Box<dyn RuntimeConnector<WorkloadId, StChecker>> {
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
    use common::objects::{
        AgentName, ExecutionState, WorkloadInstanceName, WorkloadSpec, WorkloadState,
    };
    use tokio::sync::Mutex;

    use crate::{
        runtime_connectors::{RuntimeStateGetter, StateChecker},
        workload_state::WorkloadStateSender,
    };

    use super::{RuntimeConnector, RuntimeError};

    #[async_trait]
    impl RuntimeStateGetter<String> for StubStateChecker {
        async fn get_state(&self, _workload_id: &String) -> ExecutionState {
            ExecutionState::running()
        }
    }

    #[derive(Debug)]
    pub struct StubStateChecker {
        panic_if_not_stopped: bool,
    }

    impl StubStateChecker {
        pub fn new() -> Self {
            StubStateChecker {
                panic_if_not_stopped: false,
            }
        }

        pub fn panic_if_not_stopped(&mut self) {
            self.panic_if_not_stopped = true;
        }
    }

    #[async_trait]
    impl StateChecker<String> for StubStateChecker {
        fn start_checker(
            _workload_spec: &WorkloadSpec,
            _workload_id: String,
            _manager_interface: WorkloadStateSender,
            _state_getter: impl RuntimeStateGetter<String>,
        ) -> Self {
            log::info!("Starting the checker ;)");
            StubStateChecker::new()
        }

        async fn stop_checker(mut self) {
            log::info!("Stopping the checker ;)");
            self.panic_if_not_stopped = false;
        }
    }

    impl Drop for StubStateChecker {
        fn drop(&mut self) {
            if self.panic_if_not_stopped {
                panic!("The StubStateChecker was not stopped");
            }
        }
    }

    #[derive(Debug)]
    pub enum RuntimeCall {
        GetReusableWorkloads(AgentName, Result<Vec<WorkloadState>, RuntimeError>),
        CreateWorkload(
            WorkloadSpec,
            Option<PathBuf>,
            Result<(String, StubStateChecker), RuntimeError>,
        ),
        GetWorkloadId(WorkloadInstanceName, Result<String, RuntimeError>),
        StartChecker(
            String,
            WorkloadSpec,
            WorkloadStateSender,
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

        pub async fn assert_all_expectations(self) {
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

    pub type MockRuntimeConnector = MockBase<RuntimeCall>;

    #[async_trait]
    impl RuntimeConnector<String, StubStateChecker> for MockBase<RuntimeCall> {
        fn name(&self) -> String {
            "mock-runtime".to_string()
        }

        async fn get_reusable_workloads(
            &self,
            agent_name: &AgentName,
        ) -> Result<Vec<WorkloadState>, RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::GetReusableWorkloads(expected_agent_name, result)
                    if expected_agent_name == *agent_name =>
                {
                    return result;
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Unexpected get_reusable_running_workloads call. Expected: '{expected_call:?}'\n\nGot: {agent_name:?}");
                }
            }
        }

        async fn create_workload(
            &self,
            runtime_workload_config: WorkloadSpec,
            control_interface_path: Option<PathBuf>,
            _update_state_tx: WorkloadStateSender,
        ) -> Result<(String, StubStateChecker), RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::CreateWorkload(
                    expected_runtime_workload_config,
                    expected_control_interface_path,
                    result,
                ) if expected_runtime_workload_config == runtime_workload_config
                    && expected_control_interface_path == control_interface_path =>
                {
                    return result;
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Unexpected create_workload call. Expected: '{expected_call:?}'\n\nGot: {runtime_workload_config:?}, {control_interface_path:?}");
                }
            }
        }

        async fn get_workload_id(
            &self,
            instance_name: &WorkloadInstanceName,
        ) -> Result<String, RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::GetWorkloadId(expected_instance_name, result)
                    if expected_instance_name == *instance_name =>
                {
                    return result;
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Unexpected get_workload_id call. Expected: '{expected_call:?}' \n\nGot: {instance_name:?}");
                }
            }
        }

        async fn start_checker(
            &self,
            workload_id: &String,
            runtime_workload_config: WorkloadSpec,
            update_state_tx: WorkloadStateSender,
        ) -> Result<StubStateChecker, RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::StartChecker(
                    expected_workload_id,
                    expected_runtime_workload_config,
                    expected_update_state_tx,
                    result,
                ) if expected_workload_id == *workload_id
                    && expected_runtime_workload_config == runtime_workload_config
                    && expected_update_state_tx.same_channel(&update_state_tx) =>
                {
                    return result;
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Unexpected start_checker call. Expected: '{expected_call:?}' \n\nGot: {workload_id:?}, {runtime_workload_config:?}, {update_state_tx:?}");
                }
            }
        }

        async fn delete_workload(&self, workload_id: &String) -> Result<(), RuntimeError> {
            match self.get_expected_call().await {
                RuntimeCall::DeleteWorkload(expected_workload_id, result)
                    if expected_workload_id == *workload_id =>
                {
                    return result;
                }
                expected_call => {
                    self.unexpected_call().await;
                    panic!("Unexpected delete_workload call. Expected: '{expected_call:?}'\n\nGot: {workload_id:?}");
                }
            }
        }
    }
}
