// Copyright (c) 2024 Elektrobit Automotive GmbH
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

#[cfg_attr(test, mockall_double::double)]
use super::retry_manager::RetryManager;
use crate::BUFFER_SIZE;
use crate::control_interface::ControlInterfacePath;
use crate::runtime_connectors::{RuntimeConnector, StateChecker};
use crate::workload::workload_command_channel::{WorkloadCommandReceiver, WorkloadCommandSender};
use crate::workload_state::{WorkloadStateReceiver, WorkloadStateSender};

use api::ank_base::{WorkloadInstanceNameInternal, WorkloadNamed, WorkloadStateInternal};

use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::mpsc;

pub struct ControlLoopState<WorkloadId, StChecker>
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub workload_named: WorkloadNamed,
    pub control_interface_path: Option<ControlInterfacePath>,
    pub run_folder: PathBuf,
    pub workload_id: Option<WorkloadId>,
    pub state_checker: Option<StChecker>,
    pub to_agent_workload_state_sender: WorkloadStateSender,
    pub state_checker_workload_state_sender: WorkloadStateSender,
    pub state_checker_workload_state_receiver: WorkloadStateReceiver,
    pub runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>,
    pub command_receiver: WorkloadCommandReceiver,
    pub retry_sender: WorkloadCommandSender,
    pub retry_manager: RetryManager,
}

impl<WorkloadId, StChecker> ControlLoopState<WorkloadId, StChecker>
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn builder() -> ControlLoopStateBuilder<WorkloadId, StChecker> {
        ControlLoopStateBuilder::new()
    }

    pub fn instance_name(&self) -> &WorkloadInstanceNameInternal {
        &self.workload_named.instance_name
    }
}

pub struct ControlLoopStateBuilder<WorkloadId, StChecker>
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    workload_named: Option<WorkloadNamed>,
    workload_id: Option<WorkloadId>,
    control_interface_path: Option<ControlInterfacePath>,
    run_folder: Option<PathBuf>,
    workload_state_sender: Option<WorkloadStateSender>,
    runtime: Option<Box<dyn RuntimeConnector<WorkloadId, StChecker>>>,
    workload_command_receiver: Option<WorkloadCommandReceiver>,
    retry_sender: Option<WorkloadCommandSender>,
}

impl<WorkloadId, StChecker> ControlLoopStateBuilder<WorkloadId, StChecker>
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn new() -> Self {
        ControlLoopStateBuilder {
            workload_named: None,
            workload_id: None,
            control_interface_path: None,
            run_folder: None,
            workload_state_sender: None,
            runtime: None,
            workload_command_receiver: None,
            retry_sender: None,
        }
    }

    pub fn workload_named(mut self, workload_named: WorkloadNamed) -> Self {
        self.workload_named = Some(workload_named);
        self
    }

    pub fn workload_id(mut self, workload_id: Option<WorkloadId>) -> Self {
        self.workload_id = workload_id;
        self
    }

    pub fn control_interface_path(
        mut self,
        control_interface_path: Option<ControlInterfacePath>,
    ) -> Self {
        self.control_interface_path = control_interface_path;
        self
    }

    pub fn run_folder(mut self, run_folder: PathBuf) -> Self {
        self.run_folder = Some(run_folder);
        self
    }

    pub fn workload_state_sender(mut self, update_state_tx: WorkloadStateSender) -> Self {
        self.workload_state_sender = Some(update_state_tx);
        self
    }

    pub fn runtime(mut self, runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn workload_command_receiver(mut self, command_receiver: WorkloadCommandReceiver) -> Self {
        self.workload_command_receiver = Some(command_receiver);
        self
    }

    pub fn retry_sender(mut self, workload_channel: WorkloadCommandSender) -> Self {
        self.retry_sender = Some(workload_channel);
        self
    }

    pub fn build(self) -> Result<ControlLoopState<WorkloadId, StChecker>, String> {
        // new channel for receiving the workload states from the state checker
        let (state_checker_wl_state_sender, state_checker_wl_state_receiver) =
            mpsc::channel::<WorkloadStateInternal>(BUFFER_SIZE);

        Ok(ControlLoopState {
            workload_named: self
                .workload_named
                .ok_or_else(|| "WorkloadNamed is not set".to_string())?,
            control_interface_path: self.control_interface_path,
            run_folder: self
                .run_folder
                .ok_or_else(|| "RunFolder is not set".to_string())?,
            workload_id: self.workload_id,
            state_checker: None,
            to_agent_workload_state_sender: self
                .workload_state_sender
                .ok_or_else(|| "WorkloadStateSender is not set".to_string())?,
            state_checker_workload_state_sender: state_checker_wl_state_sender,
            state_checker_workload_state_receiver: state_checker_wl_state_receiver,
            runtime: self
                .runtime
                .ok_or_else(|| "RuntimeConnector is not set".to_string())?,
            command_receiver: self
                .workload_command_receiver
                .ok_or_else(|| "WorkloadCommandReceiver is not set".to_string())?,
            retry_sender: self
                .retry_sender
                .ok_or_else(|| "WorkloadCommandSender is not set".to_string())?,
            retry_manager: Default::default(),
        })
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
    use super::ControlLoopState;
    use crate::{
        control_interface::ControlInterfacePath,
        runtime_connectors::test::{MockRuntimeConnector, StubStateChecker},
        workload::workload_command_channel::WorkloadCommandSender,
        workload_state::WorkloadStateSenderInterface,
    };

    use api::ank_base::{ExecutionStateInternal, WorkloadNamed};
    use api::test_utils::{
        generate_test_workload, generate_test_workload_state_with_workload_named,
    };

    use tokio::{sync::mpsc, time};

    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 20;

    #[tokio::test]
    async fn utest_control_loop_state_builder_build_success() {
        let control_interface_path = Some(ControlInterfacePath::new("/some/path".into()));
        let workload_named: WorkloadNamed = generate_test_workload();

        let (workload_state_sender, mut workload_state_receiver) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let runtime = Box::new(MockRuntimeConnector::new());
        let (retry_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload_named.clone())
            .control_interface_path(control_interface_path.clone())
            .run_folder("/some/path".into())
            .workload_state_sender(workload_state_sender)
            .runtime(runtime)
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(retry_sender)
            .build();

        assert!(control_loop_state.is_ok());
        let mut control_loop_state = control_loop_state.unwrap();
        assert_eq!(
            control_loop_state.workload_named.instance_name,
            workload_named.instance_name
        );
        assert_eq!(
            control_loop_state.control_interface_path,
            control_interface_path
        );

        assert!(control_loop_state.workload_id.is_none());
        assert!(control_loop_state.state_checker.is_none());

        // workload state for testing the channel between state checker and workload control loop
        let state_checker_wl_state = generate_test_workload_state_with_workload_named(
            &workload_named,
            ExecutionStateInternal::running(),
        );

        control_loop_state
            .state_checker_workload_state_sender
            .report_workload_execution_state(
                &state_checker_wl_state.instance_name,
                state_checker_wl_state.execution_state.clone(),
            )
            .await;

        assert_eq!(
            time::timeout(
                time::Duration::from_millis(100),
                control_loop_state
                    .state_checker_workload_state_receiver
                    .recv()
            )
            .await,
            Ok(Some(state_checker_wl_state))
        );

        // workload state for testing the channel between workload control loop and agent manager
        let forwarded_wl_state_to_agent = generate_test_workload_state_with_workload_named(
            &workload_named,
            ExecutionStateInternal::succeeded(),
        );

        control_loop_state
            .to_agent_workload_state_sender
            .report_workload_execution_state(
                &forwarded_wl_state_to_agent.instance_name,
                forwarded_wl_state_to_agent.execution_state.clone(),
            )
            .await;

        assert_eq!(
            time::timeout(
                time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await,
            Ok(Some(forwarded_wl_state_to_agent))
        );
    }

    #[test]
    fn utest_control_loop_state_builder_build_failed() {
        let control_loop_state = ControlLoopState::<String, StubStateChecker>::builder().build();
        assert!(control_loop_state.is_err());
    }

    #[test]
    fn utest_control_loop_state_instance_name() {
        let workload_named: api::ank_base::WorkloadNamed = generate_test_workload();
        let (workload_state_sender, _workload_state_receiver) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let runtime = Box::new(MockRuntimeConnector::new());
        let (retry_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let control_loop_state = ControlLoopState {
            workload_named: workload_named.clone(),
            control_interface_path: None,
            run_folder: "/some/path".into(),
            workload_id: None,
            state_checker: None,
            to_agent_workload_state_sender: workload_state_sender,
            state_checker_workload_state_sender,
            state_checker_workload_state_receiver,
            runtime,
            command_receiver: workload_command_receiver,
            retry_sender,
            retry_manager: Default::default(),
        };

        assert_eq!(
            *control_loop_state.instance_name(),
            workload_named.instance_name
        );
    }
}
