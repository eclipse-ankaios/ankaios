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

#[cfg_attr(test, mockall_double::double)]
use super::ReopenFile;
use api::proto;
use common::{
    from_server_interface::{FromServer, FromServerReceiver},
    to_server_interface::{ToServer, ToServerSender},
};

use prost::Message;
use tokio::{io, select, task::JoinHandle};

fn decode_to_server(protobuf_data: io::Result<Box<[u8]>>) -> io::Result<proto::ToServer> {
    Ok(proto::ToServer::decode(&mut Box::new(
        protobuf_data?.as_ref(),
    ))?)
}

pub struct PipesChannelTask {
    output_stream: ReopenFile,
    input_stream: ReopenFile,
    input_pipe_receiver: FromServerReceiver,
    output_pipe_channel: ToServerSender,
    request_id_prefix: String,
}

#[cfg_attr(test, mockall::automock)]
impl PipesChannelTask {
    pub fn new(
        output_stream: ReopenFile,
        input_stream: ReopenFile,
        input_pipe_receiver: FromServerReceiver,
        output_pipe_channel: ToServerSender,
        request_id_prefix: String,
    ) -> Self {
        Self {
            output_stream,
            input_stream,
            input_pipe_receiver,
            output_pipe_channel,
            request_id_prefix,
        }
    }
    pub async fn run(mut self) {
        loop {
            select! {
                // [impl->swdd~agent-ensures-control-interface-output-pipe-read~1]
                from_server = self.input_pipe_receiver.recv() => {
                    if let Some(from_server) = from_server {
                        let _ = self.forward_from_server(from_server).await;
                    }
                }
                // [impl->swdd~agent-listens-for-requests-from-pipe~1]
                // [impl->swdd~agent-forward-request-from-control-interface-pipe-to-server~1]
                to_server_binary = self.input_stream.read_protobuf_data() => {
                    if let Ok(to_server) = decode_to_server(to_server_binary) {
                        match to_server.try_into() {
                            Ok(ToServer::Request(mut request)) => {
                                request.prefix_request_id(&self.request_id_prefix);
                                let _ = self.output_pipe_channel.send(ToServer::Request(request)).await;
                            }
                            Ok(to_server_message) => {
                                let _ = self.output_pipe_channel.send(to_server_message).await;
                            }
                            Err(error) => {
                                log::warn!("Could not convert protobuf in internal data structure: {}", error)
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn run_task(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    async fn forward_from_server(&mut self, command: FromServer) -> io::Result<()> {
        if let Ok(proto) = proto::FromServer::try_from(command) {
            // [impl->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
            let binary = proto.encode_length_delimited_to_vec();
            self.output_stream.write_all(&binary).await?;
        }
        Ok(())
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
pub fn generate_test_pipes_channel_task_mock() -> __mock_MockPipesChannelTask::__new::Context {
    let pipes_channel_task_mock_context = MockPipesChannelTask::new_context();
    pipes_channel_task_mock_context
        .expect()
        .return_once(|_, _, _, _, _| {
            let mut pipes_channel_task_mock = MockPipesChannelTask::default();
            pipes_channel_task_mock
                .expect_run_task()
                .return_once(|| tokio::spawn(async {}));
            pipes_channel_task_mock
        });
    pipes_channel_task_mock_context
}

#[cfg(test)]
mod tests {
    use common::commands;
    use mockall::predicate;
    use tokio::sync::mpsc;

    use super::*;

    use crate::control_interface::MockReopenFile;

    #[tokio::test]
    async fn utest_pipes_channel_task_forward_from_server() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_command = FromServer::Response(commands::Response {
            request_id: "req_id".to_owned(),
            response_content: commands::ResponseContent::CompleteState(Default::default()),
        });

        let test_command_binary = proto::FromServer::try_from(test_command.clone())
            .unwrap()
            .encode_length_delimited_to_vec();

        // [utest->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
        let mut output_stream_mock = MockReopenFile::default();
        output_stream_mock
            .expect_write_all()
            .with(predicate::eq(test_command_binary))
            .return_once(|_| Ok(()));

        let input_stream_mock = MockReopenFile::default();
        let (_, input_pipe_receiver) = mpsc::channel(1);
        let (output_pipe_sender, _) = mpsc::channel(1);
        let request_id_prefix = String::from("prefix@");

        let mut pipes_channel_task = PipesChannelTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix,
        );

        assert!(pipes_channel_task
            .forward_from_server(test_command)
            .await
            .is_ok());
    }

    // [utest->swdd~agent-listens-for-requests-from-pipe~1]
    // [utest->swdd~agent-forward-request-from-control-interface-pipe-to-server~1]
    // [utest->swdd~agent-ensures-control-interface-output-pipe-read~1]
    #[tokio::test]
    async fn utest_pipes_channel_task_run_task() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_output_request = proto::ToServer {
            to_server_enum: Some(proto::to_server::ToServerEnum::Request(proto::Request {
                request_id: "req_id".to_owned(),
                request_content: Some(proto::request::RequestContent::CompleteStateRequest(
                    proto::CompleteStateRequest { field_mask: vec![] },
                )),
            })),
        };

        let test_output_request_binary = test_output_request.encode_to_vec();

        let mut input_stream_mock = MockReopenFile::default();
        let mut x = [0; 12];
        x.clone_from_slice(&test_output_request_binary[..]);
        input_stream_mock
            .expect_read_protobuf_data()
            .returning(move || Ok(Box::new(x)));

        let test_input_command = FromServer::Response(commands::Response {
            request_id: "req_id".to_owned(),
            response_content: commands::ResponseContent::CompleteState(Default::default()),
        });

        let test_input_command_binary = proto::FromServer::try_from(test_input_command.clone())
            .unwrap()
            .encode_length_delimited_to_vec();

        let mut output_stream_mock = MockReopenFile::default();
        output_stream_mock
            .expect_write_all()
            .with(predicate::eq(test_input_command_binary.clone()))
            .return_once(|_| Ok(()));

        let (input_pipe_sender, input_pipe_receiver) = mpsc::channel(1);
        let (output_pipe_sender, mut output_pipe_receiver) = mpsc::channel(1);
        let request_id_prefix = String::from("prefix@");

        let pipes_channel_task = PipesChannelTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix,
        );

        let handle = pipes_channel_task.run_task();

        assert!(input_pipe_sender.send(test_input_command).await.is_ok());
        assert_eq!(
            Some(ToServer::Request(commands::Request {
                request_id: "prefix@req_id".to_owned(),
                request_content: commands::RequestContent::CompleteStateRequest(
                    commands::CompleteStateRequest { field_mask: vec![] }
                )
            })),
            output_pipe_receiver.recv().await
        );

        handle.abort();
    }
}
