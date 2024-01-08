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

// mod exports
pub mod workload_command_channel;
pub mod workload_control_loop;

// public api exports
pub use workload_command_channel::WorkloadCommandSender;
pub use workload_control_loop::{ControlLoopState, RestartCounter, WorkloadControlLoop};

use std::{fmt::Display, path::PathBuf};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;
use common::{
    commands::CompleteState, execution_interface::ExecutionCommand, objects::WorkloadSpec,
};

#[cfg(test)]
use mockall::automock;

#[derive(Debug, PartialEq, Eq)]
pub enum WorkloadError {
    Communication(String),
    CompleteState(String),
}

impl Display for WorkloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkloadError::Communication(msg) => {
                write!(f, "Could not send command to workload task: '{}'", msg)
            }
            WorkloadError::CompleteState(msg) => {
                write!(f, "Could not forward complete state '{}'", msg)
            }
        }
    }
}

//#[derive(Debug)]
pub enum WorkloadCommand {
    Delete,
    Update(Box<WorkloadSpec>, Option<PathBuf>),
    Restart(Box<WorkloadSpec>, Option<PathBuf>),
    Create(Box<WorkloadSpec>, Option<PathBuf>),
}

// #[derive(Debug)]
pub struct Workload {
    name: String,
    channel: WorkloadCommandSender,
    control_interface: Option<PipesChannelContext>,
}

#[cfg_attr(test, automock)]
impl Workload {
    pub fn new(
        name: String,
        channel: WorkloadCommandSender,
        control_interface: Option<PipesChannelContext>,
    ) -> Self {
        Workload {
            name,
            channel,
            control_interface,
        }
    }

    // [impl->swdd~agent-workload-obj-update-command~1]
    pub async fn update(
        &mut self,
        spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
    ) -> Result<(), WorkloadError> {
        log::info!("Updating workload '{}'.", self.name);

        if let Some(control_interface) = self.control_interface.take() {
            control_interface.abort_pipes_channel_task()
        }
        self.control_interface = control_interface;

        let control_interface_path = self
            .control_interface
            .as_ref()
            .map(|control_interface| control_interface.get_api_location());

        log::debug!("Send WorkloadCommand::Update.");
        self.channel
            .update(spec, control_interface_path)
            .await
            .map_err(|err| WorkloadError::Communication(err.to_string()))
    }

    // [impl->swdd~agent-workload-obj-delete-command~1]
    pub async fn delete(self) -> Result<(), WorkloadError> {
        log::info!("Deleting workload '{}'.", self.name);

        if let Some(control_interface) = self.control_interface {
            control_interface.abort_pipes_channel_task()
        }

        self.channel
            .delete()
            .await
            .map_err(|err| WorkloadError::Communication(err.to_string()))
    }

    // [impl->swdd~agent-forward-responses-to-control-interface-pipe~1]
    pub async fn send_complete_state(
        &mut self,
        complete_state: CompleteState,
    ) -> Result<(), WorkloadError> {
        let control_interface =
            self.control_interface
                .as_ref()
                .ok_or(WorkloadError::CompleteState(
                    "control interface not available".to_string(),
                ))?;
        control_interface
            .get_input_pipe_sender()
            .send(ExecutionCommand::CompleteState(Box::new(complete_state)))
            .await
            .map_err(|err| WorkloadError::CompleteState(err.to_string()))
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

    use std::path::PathBuf;
    use std::time::Duration;

    use common::{
        commands::CompleteState,
        execution_interface::ExecutionCommand,
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };
    use tokio::{sync::mpsc, time::timeout};

    use crate::{
        control_interface::MockPipesChannelContext,
        workload::WorkloadCommandSender,
        workload::{Workload, WorkloadCommand, WorkloadError},
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const PIPES_LOCATION: &str = "/some/path";
    const REQUEST_ID: &str = "request_id";

    const TEST_WL_COMMAND_BUFFER_SIZE: usize = 5;
    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 5;

    // [utest->swdd~agent-workload-obj-delete-command~1]
    #[tokio::test]
    async fn utest_workload_obj_delete_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();

        // drop the receiver so that the send command fails
        drop(workload_command_receiver);

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(old_control_interface_mock),
        );

        assert!(matches!(
            test_workload.delete().await,
            Err(WorkloadError::Communication(_))
        ));
    }

    // [utest->swdd~agent-workload-obj-update-command~1]
    #[tokio::test]
    async fn utest_workload_obj_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let mut new_control_interface_mock = MockPipesChannelContext::default();
        new_control_interface_mock
            .expect_get_api_location()
            .once()
            .return_const(PIPES_LOCATION);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(old_control_interface_mock),
        );

        test_workload
            .update(workload_spec.clone(), Some(new_control_interface_mock))
            .await
            .unwrap();

        let expected_workload_spec = Box::new(workload_spec);
        let expected_pipes_path_buf = PathBuf::from(PIPES_LOCATION);

        assert!(matches!(
            timeout(Duration::from_millis(200), workload_command_receiver.recv()).await,
            Ok(Some(WorkloadCommand::Update(
                boxed_workload_spec,
                Some(pipes_path_buf)
            )))
        if expected_workload_spec == boxed_workload_spec && expected_pipes_path_buf == pipes_path_buf));
    }

    // [utest->swdd~agent-workload-obj-update-command~1]
    #[tokio::test]
    async fn utest_workload_obj_update_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();

        // drop the receiver so that the send command fails
        drop(workload_command_receiver);

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let mut new_control_interface_mock = MockPipesChannelContext::default();
        new_control_interface_mock
            .expect_get_api_location()
            .once()
            .return_const(PIPES_LOCATION);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(old_control_interface_mock),
        );

        assert!(matches!(
            test_workload
                .update(workload_spec.clone(), Some(new_control_interface_mock))
                .await,
            Err(WorkloadError::Communication(_))
        ));
    }

    // [utest->swdd~agent-workload-obj-delete-command~1]
    #[tokio::test]
    async fn utest_workload_obj_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        let mut old_control_interface_mock = MockPipesChannelContext::default();
        old_control_interface_mock
            .expect_abort_pipes_channel_task()
            .once()
            .return_const(());

        let test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(old_control_interface_mock),
        );

        test_workload.delete().await.unwrap();

        assert!(matches!(
            timeout(Duration::from_millis(200), workload_command_receiver.recv()).await,
            Ok(Some(WorkloadCommand::Delete))
        ));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, _) = WorkloadCommandSender::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_input_pipe_sender()
            .once()
            .return_const(state_change_tx);

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(control_interface_mock),
        );
        let complete_state = generate_test_complete_state(
            format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
            vec![generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_1_NAME.to_string(),
                RUNTIME_NAME.to_string(),
            )],
        );

        test_workload
            .send_complete_state(complete_state.clone())
            .await
            .unwrap();

        let expected_complete_state_box = Box::new(complete_state);

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(ExecutionCommand::CompleteState(complete_state_box)))
        if expected_complete_state_box == complete_state_box));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_pipes_communication_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, _) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        drop(state_change_rx);

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_input_pipe_sender()
            .once()
            .return_const(state_change_tx);

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(control_interface_mock),
        );
        let complete_state = CompleteState::default();

        assert!(matches!(
            test_workload.send_complete_state(complete_state).await,
            Err(WorkloadError::CompleteState(_))
        ));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_no_control_interface() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, _) = WorkloadCommandSender::new();

        let mut test_workload =
            Workload::new(WORKLOAD_1_NAME.to_string(), workload_command_sender, None);
        let complete_state = CompleteState::default();

        assert!(matches!(
            test_workload.send_complete_state(complete_state).await,
            Err(WorkloadError::CompleteState(_))
        ));
    }
}
