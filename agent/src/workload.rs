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
pub mod control_loop_state;
pub mod workload_command_channel;
pub mod workload_control_loop;

// public api exports
pub use control_loop_state::ControlLoopState;
pub use workload_command_channel::WorkloadCommandSender;
#[cfg(test)]
pub use workload_control_loop::MockWorkloadControlLoop;

use std::{fmt::Display, path::PathBuf};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;
#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContextInfo;

use api::ank_base;

use common::{
    from_server_interface::FromServer,
    objects::{WorkloadInstanceName, WorkloadSpec},
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

#[derive(Debug, PartialEq)]
pub enum WorkloadCommand {
    Delete,
    Update(Option<Box<WorkloadSpec>>, Option<PathBuf>),
    Retry(Box<WorkloadInstanceName>),
    Create,
    Resume,
}

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
    // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
    fn exchange_control_interface(
        &mut self,
        pipes_channel_context_info: Option<PipesChannelContextInfo>,
    ) {
        if let Some(control_interface) = self.control_interface.take() {
            control_interface.abort_pipes_channel_task()
        }
        self.control_interface = match pipes_channel_context_info {
            Some(info) => info.create_control_interface(),
            None => None,
        };
    }

    fn is_control_interface_changed(
        &self,
        pipes_channel_context_info: &Option<PipesChannelContextInfo>,
    ) -> bool {
        match (&self.control_interface, pipes_channel_context_info) {
            (None, None) => false,
            (Some(_current), None) => true,
            (None, Some(_new)) => true,
            (Some(current), Some(new_context)) => !new_context.has_same_configuration(current),
        }
    }

    fn update_control_interface(
        &mut self,
        pipes_channel_context_info: Option<PipesChannelContextInfo>,
    ) {
        if self.is_control_interface_changed(&pipes_channel_context_info) {
            self.exchange_control_interface(pipes_channel_context_info);
        }
    }

    // [impl->swdd~agent-workload-obj-update-command~1]
    pub async fn update(
        &mut self,
        spec: Option<WorkloadSpec>,
        pipes_channel_context_info: Option<PipesChannelContextInfo>,
    ) -> Result<(), WorkloadError> {
        log::info!("Updating workload '{}'.", self.name);

        self.update_control_interface(pipes_channel_context_info);

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
    pub async fn forward_response(
        &mut self,
        response: ank_base::Response,
    ) -> Result<(), WorkloadError> {
        let control_interface =
            self.control_interface
                .as_ref()
                .ok_or(WorkloadError::CompleteState(
                    "control interface not available".to_string(),
                ))?;
        control_interface
            .get_input_pipe_sender()
            .send(FromServer::Response(response))
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

    use super::ank_base::{self, response::ResponseContent, Response};
    use common::{
        from_server_interface::FromServer,
        objects::{generate_test_workload_spec_with_param, CompleteState},
        test_utils::generate_test_complete_state,
    };
    use tokio::{sync::mpsc, time::timeout};

    use crate::{
        control_interface::MockPipesChannelContext,
        control_interface::MockPipesChannelContextInfo,
        workload::{Workload, WorkloadCommand, WorkloadCommandSender, WorkloadError},
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

    #[test]
    fn utest_is_control_interface_changed_set_from_none_to_new_returns_true() {
        let (workload_command_sender, _) = WorkloadCommandSender::new();
        let test_workload_with_control_interface = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender.clone(),
            None,
        );
        assert!(test_workload_with_control_interface
            .is_control_interface_changed(&Some(MockPipesChannelContextInfo::default())));
    }

    #[test]
    fn utest_is_control_interface_changed_set_from_existing_to_none_returns_true() {
        let (workload_command_sender, _) = WorkloadCommandSender::new();

        let test_workload_with_control_interface = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender.clone(),
            Some(MockPipesChannelContext::default()),
        );

        assert!(test_workload_with_control_interface.is_control_interface_changed(&None));
    }

    #[test]
    fn utest_is_control_interface_changed_set_from_none_to_none_returns_false() {
        let (workload_command_sender, _) = WorkloadCommandSender::new();

        let test_workload_with_control_interface = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender.clone(),
            None,
        );

        assert!(!test_workload_with_control_interface.is_control_interface_changed(&None));
    }

    #[test]
    fn utest_is_control_interface_changed_returns_true() {
        let (workload_command_sender, _) = WorkloadCommandSender::new();

        let mut pipes_channel_context_info_mock = MockPipesChannelContextInfo::default();
        pipes_channel_context_info_mock
            .expect_has_same_configuration()
            .once()
            .return_const(false);

        let test_workload_with_control_interface = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender.clone(),
            Some(MockPipesChannelContext::default()),
        );

        assert!(test_workload_with_control_interface
            .is_control_interface_changed(&Some(pipes_channel_context_info_mock)));
    }

    #[test]
    fn utest_is_control_interface_changed_returns_false() {
        let (workload_command_sender, _) = WorkloadCommandSender::new();

        let mut pipes_channel_context_info_mock = MockPipesChannelContextInfo::default();
        pipes_channel_context_info_mock
            .expect_has_same_configuration()
            .once()
            .return_const(true);

        let test_workload_with_control_interface = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender.clone(),
            Some(MockPipesChannelContext::default()),
        );

        assert!(!test_workload_with_control_interface
            .is_control_interface_changed(&Some(pipes_channel_context_info_mock)));
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

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_control_interface_mock = MockPipesChannelContext::default();
        new_control_interface_mock
            .expect_get_api_location()
            .once()
            .return_const(PIPES_LOCATION);

        let mut new_control_interface_info_mock = MockPipesChannelContextInfo::default();
        new_control_interface_info_mock
            .expect_has_same_configuration()
            .once()
            .return_const(false);
        new_control_interface_info_mock
            .expect_create_control_interface()
            .once()
            .return_once(|| Some(new_control_interface_mock));

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(old_control_interface_mock),
        );

        test_workload
            .update(
                Some(workload_spec.clone()),
                Some(new_control_interface_info_mock),
            )
            .await
            .unwrap();

        let expected_workload_spec = Box::new(workload_spec);
        let expected_pipes_path_buf = PathBuf::from(PIPES_LOCATION);

        assert_eq!(
            Ok(Some(WorkloadCommand::Update(
                Some(expected_workload_spec),
                Some(expected_pipes_path_buf)
            ))),
            timeout(Duration::from_millis(200), workload_command_receiver.recv()).await
        );
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

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_control_interface_mock = MockPipesChannelContextInfo::default();
        new_control_interface_mock
            .expect_has_same_configuration()
            .once()
            .return_const(false);
        new_control_interface_mock
            .expect_create_control_interface()
            .once()
            .return_once(|| None);

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(old_control_interface_mock),
        );

        assert!(matches!(
            test_workload
                .update(
                    Some(workload_spec.clone()),
                    Some(new_control_interface_mock)
                )
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
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_input_pipe_sender()
            .once()
            .return_const(to_server_tx);

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(control_interface_mock),
        );
        let complete_state =
            generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_1_NAME.to_string(),
                RUNTIME_NAME.to_string(),
            )]);

        test_workload
            .forward_response(ank_base::Response {
                request_id: REQUEST_ID.to_owned(),
                response_content: Some(ank_base::response::ResponseContent::CompleteState(
                    complete_state.clone().into(),
                )),
            })
            .await
            .unwrap();

        let expected_complete_state = complete_state;

        assert!(matches!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(FromServer::Response(Response{request_id: _, response_content: Some(ResponseContent::CompleteState(complete_state))})))
        if ank_base::CompleteState::from(expected_complete_state) == complete_state));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_workload_obj_send_complete_state_pipes_communication_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, _) = WorkloadCommandSender::new();
        let (to_server_tx, to_server_rx) = mpsc::channel(TEST_WL_COMMAND_BUFFER_SIZE);

        drop(to_server_rx);

        let mut control_interface_mock = MockPipesChannelContext::default();
        control_interface_mock
            .expect_get_input_pipe_sender()
            .once()
            .return_const(to_server_tx);

        let mut test_workload = Workload::new(
            WORKLOAD_1_NAME.to_string(),
            workload_command_sender,
            Some(control_interface_mock),
        );
        let complete_state = CompleteState::default();

        assert!(matches!(
            test_workload
                .forward_response(ank_base::Response {
                    request_id: REQUEST_ID.to_owned(),
                    response_content: Some(ank_base::response::ResponseContent::CompleteState(
                        complete_state.clone().into(),
                    )),
                })
                .await,
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
            test_workload
                .forward_response(ank_base::Response {
                    request_id: REQUEST_ID.to_owned(),
                    response_content: Some(ank_base::response::ResponseContent::CompleteState(
                        complete_state.clone().into(),
                    )),
                })
                .await,
            Err(WorkloadError::CompleteState(_))
        ));
    }
}
