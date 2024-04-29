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

#[cfg(test)]
use mockall::automock;

use crate::runtime_connectors::{RuntimeConnector, StateChecker};
use crate::workload::workload_command_channel::{WorkloadCommandReceiver, WorkloadCommandSender};
use crate::workload::workload_control_loop::RetryCounter;
use crate::workload_state::{WorkloadStateReceiver, WorkloadStateSender};
use crate::BUFFER_SIZE;
use common::objects::{WorkloadInstanceName, WorkloadSpec, WorkloadState};
use std::path::PathBuf;

pub struct ControlLoopState<WorkloadId, StChecker>
where
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub workload_spec: WorkloadSpec,
    pub control_interface_path: Option<PathBuf>,
    pub workload_id: Option<WorkloadId>,
    pub state_checker: Option<StChecker>,
    // sender to forward workload states to the agent manager
    pub workload_state_sender: WorkloadStateSender,
    // sender passed to the state checker
    pub state_checker_workload_state_sender: WorkloadStateSender,
    // sender to listen to the state checker workload states
    pub state_checker_workload_state_receiver: WorkloadStateReceiver,
    pub runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>,
    pub command_receiver: WorkloadCommandReceiver,
    // sender to forward retry commands to the control loop
    pub retry_sender: WorkloadCommandSender,
    pub retry_counter: RetryCounter,
}

impl<WorkloadId, StChecker> ControlLoopState<WorkloadId, StChecker>
where
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn builder() -> ControlLoopStateBuilder<WorkloadId, StChecker> {
        ControlLoopStateBuilder::new()
    }

    pub fn instance_name(&self) -> &WorkloadInstanceName {
        &self.workload_spec.instance_name
    }
}

pub struct ControlLoopStateBuilder<WorkloadId, StChecker>
where
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub workload_spec: Option<WorkloadSpec>,
    pub control_interface_path: Option<PathBuf>,
    pub workload_id: Option<WorkloadId>,
    pub state_checker: Option<StChecker>,
    pub workload_state_sender: Option<WorkloadStateSender>,
    pub runtime: Option<Box<dyn RuntimeConnector<WorkloadId, StChecker>>>,
    pub workload_command_receiver: Option<WorkloadCommandReceiver>,
    pub retry_sender: Option<WorkloadCommandSender>,
    pub retry_counter: RetryCounter,
}

#[cfg_attr(test, automock)]
impl<WorkloadId, StChecker> ControlLoopStateBuilder<WorkloadId, StChecker>
where
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn new() -> Self {
        ControlLoopStateBuilder {
            workload_spec: None,
            control_interface_path: None,
            workload_id: None,
            state_checker: None,
            workload_state_sender: None,
            runtime: None,
            workload_command_receiver: None,
            retry_sender: None,
            retry_counter: RetryCounter::new(),
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn workload_spec(mut self, workload_spec: WorkloadSpec) -> Self {
        self.workload_spec = Some(workload_spec);
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn control_interface_path(mut self, control_interface_path: Option<PathBuf>) -> Self {
        self.control_interface_path = control_interface_path;
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn workload_state_sender(mut self, update_state_tx: WorkloadStateSender) -> Self {
        self.workload_state_sender = Some(update_state_tx);
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn runtime(mut self, runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn workload_command_receiver(mut self, command_receiver: WorkloadCommandReceiver) -> Self {
        self.workload_command_receiver = Some(command_receiver);
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn retry_sender(mut self, workload_channel: WorkloadCommandSender) -> Self {
        self.retry_sender = Some(workload_channel);
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn build(self) -> Result<ControlLoopState<WorkloadId, StChecker>, String> {
        // new channel for receiving the workload states from the state checker
        let (state_checker_wl_state_sender, state_checker_wl_state_receiver) =
            tokio::sync::mpsc::channel::<WorkloadState>(BUFFER_SIZE);

        Ok(ControlLoopState {
            workload_spec: self
                .workload_spec
                .ok_or_else(|| "WorkloadSpec is not set".to_string())?,
            control_interface_path: self.control_interface_path,
            workload_id: self.workload_id,
            state_checker: self.state_checker,
            workload_state_sender: self
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
            retry_counter: self.retry_counter,
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
        runtime_connectors::test::MockRuntimeConnector,
        workload::{
            workload_command_channel::WorkloadCommandSender, workload_control_loop::RetryCounter,
        },
    };
    use common::objects::generate_test_workload_spec;

    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 20;

    #[test]
    fn utest_control_loop_state_builder_build_success() {
        let workload_spec = generate_test_workload_spec();

        let (workload_state_sender, _workload_state_receiver) =
            tokio::sync::mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let runtime = Box::new(MockRuntimeConnector::new());
        let (retry_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone()) // workload spec is moved here!
            .workload_state_sender(workload_state_sender)
            .runtime(runtime)
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(retry_sender)
            .build();

        assert!(control_loop_state.is_ok());
        let control_loop_state = control_loop_state.unwrap();
        assert_eq!(
            control_loop_state.workload_spec.instance_name,
            workload_spec.instance_name
        );
    }

    #[test]
    fn utest_contol_loop_state_builder_build_failed() {
        let workload_spec = generate_test_workload_spec();

        let (workload_state_sender, _workload_state_receiver) =
            tokio::sync::mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let runtime = Box::new(MockRuntimeConnector::new());
        let (_retry_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_state_sender(workload_state_sender)
            .runtime(runtime)
            .workload_command_receiver(workload_command_receiver)
            .build();

        assert!(control_loop_state.is_err());
    }

    #[test]
    fn utest_control_loop_state_instance_name() {
        let workload_spec = generate_test_workload_spec();
        let (workload_state_sender, _workload_state_receiver) =
            tokio::sync::mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            tokio::sync::mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let runtime = Box::new(MockRuntimeConnector::new());
        let (retry_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let control_loop_state = ControlLoopState {
            workload_spec: workload_spec.clone(),
            control_interface_path: None,
            workload_id: None,
            state_checker: None,
            workload_state_sender,
            state_checker_workload_state_sender,
            state_checker_workload_state_receiver,
            runtime,
            command_receiver: workload_command_receiver,
            retry_sender,
            retry_counter: RetryCounter::new(),
        };

        assert_eq!(
            control_loop_state.instance_name().workload_name(),
            workload_spec.instance_name.workload_name()
        );
    }
}
