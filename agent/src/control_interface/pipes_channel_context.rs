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

use common::objects::WorkloadInstanceName;

#[cfg(test)]
use mockall::automock;

use super::authorizer::Authorizer;
#[cfg_attr(test, mockall_double::double)]
use super::input_output::InputOutput;
#[cfg_attr(test, mockall_double::double)]
use super::pipes_channel_task::PipesChannelTask;
#[cfg_attr(test, mockall_double::double)]
use super::reopen_file::ReopenFile;
#[cfg_attr(test, mockall_double::double)]
use super::FromServerChannels;
use common::{from_server_interface::FromServerSender, to_server_interface::ToServerSender};
use std::{
    fmt::{self, Display},
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::task::JoinHandle;

#[derive(Debug)]
pub enum PipesChannelContextError {
    CouldNotCreateFifo(String),
}

impl Display for PipesChannelContextError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PipesChannelContextError::CouldNotCreateFifo(msg) => {
                write!(f, "{msg:?}")
            }
        }
    }
}

// [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
pub struct PipesChannelContext {
    pipes: InputOutput,
    input_pipe_sender: FromServerSender,
    task_handle: JoinHandle<()>,
    authorizer: Arc<Authorizer>,
}

#[cfg_attr(test, automock)]
impl PipesChannelContext {
    pub fn new(
        run_directory: &Path,
        execution_instance_name: &WorkloadInstanceName,
        output_pipe_channel: ToServerSender,
        authorizer: Authorizer,
    ) -> Result<Self, PipesChannelContextError> {
        // [impl->swdd~agent-control-interface-pipes-path-naming~1]
        match InputOutput::new(execution_instance_name.pipes_folder_name(run_directory)) {
            Ok(pipes) => {
                let input_stream = ReopenFile::open(pipes.get_output().get_path());
                let output_stream = ReopenFile::create(pipes.get_input().get_path());
                let request_id_prefix = [execution_instance_name.workload_name(), ""].join("@");
                let input_pipe_channels = FromServerChannels::new(1024);

                let authorizer = Arc::new(authorizer);

                Ok(PipesChannelContext {
                    pipes,
                    input_pipe_sender: input_pipe_channels.get_sender(),
                    task_handle: PipesChannelTask::new(
                        output_stream,
                        input_stream,
                        input_pipe_channels.move_receiver(),
                        output_pipe_channel,
                        request_id_prefix,
                        authorizer.clone(),
                    )
                    .run_task(),
                    authorizer,
                })
            }
            Err(e) => Err(PipesChannelContextError::CouldNotCreateFifo(e.to_string())),
        }
    }

    #[allow(dead_code)]
    // Used in the tests below for now
    pub fn get_authorizer(&self) -> &Authorizer {
        &self.authorizer
    }

    #[allow(dead_code)]
    // Used in the tests below for now
    pub fn get_api_location(&self) -> PathBuf {
        self.pipes.get_location()
    }
    pub fn get_input_pipe_sender(&self) -> FromServerSender {
        self.input_pipe_sender.clone()
    }

    pub fn abort_pipes_channel_task(&self) {
        self.task_handle.abort();
    }
}

impl Drop for PipesChannelContext {
    fn drop(&mut self) {
        self.abort_pipes_channel_task()
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
    use std::path::Path;

    use common::from_server_interface::FromServer;
    use tokio::sync::mpsc;

    const CONFIG: &str = "config";

    use crate::control_interface::{
        generate_test_input_output_mock, generate_test_pipes_channel_task_mock, Authorizer,
        MockFromServerChannels, MockReopenFile, PipesChannelContext,
    };
    use common::objects::WorkloadInstanceName;

    // [utest->swdd~agent-create-control-interface-pipes-per-workload~1]
    // [utest->swdd~agent-control-interface-pipes-path-naming~1]
    #[tokio::test]
    async fn utest_pipes_channel_context_get_api_location_returns_valid_location() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let reopen_file_mock_open = MockReopenFile::open_context();
        reopen_file_mock_open
            .expect()
            .returning(|_| MockReopenFile::default());
        let reopen_file_mock_create = MockReopenFile::create_context();
        reopen_file_mock_create
            .expect()
            .returning(|_| MockReopenFile::default());

        let _input_output_mock = generate_test_input_output_mock();

        let ex_com_ch_mock_context = MockFromServerChannels::new_context();
        let (sender, receiver) = mpsc::channel(1);
        ex_com_ch_mock_context.expect().return_once(move |_| {
            let mut mock = MockFromServerChannels::default();
            mock.expect_get_sender().return_const(sender);
            mock.expect_move_receiver().return_once(|| receiver);
            mock
        });

        let _pipes_channel_task_mock = generate_test_pipes_channel_task_mock();

        let pipes_channel_context = PipesChannelContext::new(
            Path::new("api_pipes_location"),
            &WorkloadInstanceName::builder()
                .workload_name("workload_name_1")
                .config(&String::from(CONFIG))
                .build(),
            mpsc::channel(1).0,
            Authorizer::default(),
        )
        .unwrap();

        assert_eq!(
            pipes_channel_context
                .get_api_location()
                .as_os_str()
                .to_string_lossy(),
            "api_pipes_location/workload_name_1.b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910"
        );
    }

    // [utest->swdd~agent-create-control-interface-pipes-per-workload~1]
    #[tokio::test]
    async fn utest_get_input_pipe_sender_returns_valid_sender() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let reopen_file_mock_open = MockReopenFile::open_context();
        reopen_file_mock_open
            .expect()
            .returning(|_| MockReopenFile::default());
        let reopen_file_mock_create = MockReopenFile::create_context();
        reopen_file_mock_create
            .expect()
            .returning(|_| MockReopenFile::default());

        let _input_output_mock = generate_test_input_output_mock();

        let ex_com_ch_mock_context = MockFromServerChannels::new_context();
        let (sender, mut receiver) = mpsc::channel(1024);
        ex_com_ch_mock_context.expect().return_once(move |_| {
            let mut mock = MockFromServerChannels::default();
            mock.expect_get_sender().return_const(sender);
            mock.expect_move_receiver()
                .return_once(|| mpsc::channel(1).1); //return fake receiver
            mock
        });

        let _pipes_channel_task_mock = generate_test_pipes_channel_task_mock();

        let pipes_channel_context = PipesChannelContext::new(
            Path::new("api_pipes_location"),
            &WorkloadInstanceName::builder()
                .agent_name("workload_name_1")
                .config(&String::from(CONFIG))
                .build(),
            mpsc::channel(1).0,
            Authorizer::default(),
        )
        .unwrap();

        let _ = pipes_channel_context
            .get_input_pipe_sender()
            .send(FromServer::UpdateWorkload(
                common::commands::UpdateWorkload {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                },
            ))
            .await;

        assert_eq!(
            Some(FromServer::UpdateWorkload(
                common::commands::UpdateWorkload {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                }
            )),
            receiver.recv().await
        );

        pipes_channel_context.abort_pipes_channel_task();
    }
}
