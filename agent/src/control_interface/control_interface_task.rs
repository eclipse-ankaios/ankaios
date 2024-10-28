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

use std::sync::Arc;

use crate::control_interface::{to_ankaios, ToAnkaios};

#[cfg_attr(test, mockall_double::double)]
use super::authorizer::Authorizer;
#[cfg_attr(test, mockall_double::double)]
use super::reopen_file::ReopenFile;
use api::{ank_base, control_api};
use common::{
    check_version_compatibility,
    from_server_interface::{FromServer, FromServerReceiver},
    to_server_interface::{ToServer, ToServerSender},
};

use prost::Message;
use tokio::{io, select, task::JoinHandle};

const INITIAL_HELLO_MISSING_MSG: &str = "Initial Hello missing!";
const PROTOBUF_DECODE_ERROR_MSG: &str = "Could not decode protobuf data";

fn decode_to_server(protobuf_data: io::Result<Vec<u8>>) -> io::Result<control_api::ToAnkaios> {
    Ok(control_api::ToAnkaios::decode(&mut Box::new(
        protobuf_data?.as_ref(),
    ))?)
}

pub struct ControlInterfaceTask {
    output_stream: ReopenFile,
    input_stream: ReopenFile,
    input_pipe_receiver: FromServerReceiver,
    output_pipe_channel: ToServerSender,
    request_id_prefix: String,
    authorizer: Arc<Authorizer>,
}

#[cfg_attr(test, mockall::automock)]
impl ControlInterfaceTask {
    pub fn new(
        output_stream: ReopenFile,
        input_stream: ReopenFile,
        input_pipe_receiver: FromServerReceiver,
        output_pipe_channel: ToServerSender,
        request_id_prefix: String,
        authorizer: Arc<Authorizer>,
    ) -> Self {
        Self {
            output_stream,
            input_stream,
            input_pipe_receiver,
            output_pipe_channel,
            request_id_prefix,
            authorizer,
        }
    }

    // [impl->swdd~agent-closes-control-interface-on-missing-initial-hello~1]
    async fn check_initial_hello(&mut self) -> Result<(), String> {
        let to_ankaios =
            decode_to_server(self.input_stream.read_protobuf_data().await).map_err(|err| {
                log::debug!("{}: '{:?}'", PROTOBUF_DECODE_ERROR_MSG, err);
                PROTOBUF_DECODE_ERROR_MSG.to_string()
            })?;
        match to_ankaios.try_into() {
            Ok(ToAnkaios::Hello(to_ankaios::Hello { protocol_version })) => {
                check_version_compatibility(protocol_version)?
            }
            unexpected => {
                log::debug!("Expected initial Hello, received: '{unexpected:?}'.");
                return Err(INITIAL_HELLO_MISSING_MSG.into());
            }
        }
        Ok(())
    }

    pub async fn run(mut self) {
        // [impl->swdd~agent-closes-control-interface-on-missing-initial-hello~1]
        if let Err(message) = self.check_initial_hello().await {
            log::warn!("{message}");
            let _ = self.send_connection_closed(message).await;
            return;
        }

        loop {
            select! {
                // [impl->swdd~agent-ensures-control-interface-output-pipe-read~1]
                from_server = self.input_pipe_receiver.recv() => {
                    if let Some(FromServer::Response(response)) = from_server {
                        let _ = self.forward_from_server(response).await;
                    } else {
                        log::warn!("The server is sending unrequested messages to a workload: '{:?}'", from_server);
                    }
                }
                // [impl->swdd~agent-listens-for-requests-from-pipe~1]
                // [impl->swdd~agent-forward-request-from-control-interface-pipe-to-server~1]
                to_ankaios_binary = self.input_stream.read_protobuf_data() => {
                    if let Ok(to_ankaios) = decode_to_server(to_ankaios_binary) {
                        // [impl->swdd~agent-converts-control-interface-message-to-ankaios-object~1]
                        match to_ankaios.try_into() {
                            Ok(ToAnkaios::Request(mut request)) => {
                                if self.authorizer.authorize(&request) {
                                    log::debug!("Allowing request '{:?}' from authorizer '{:?}'", request, self.authorizer);
                                    request.prefix_request_id(&self.request_id_prefix);
                                    let _ = self.output_pipe_channel.send(ToServer::Request(request)).await;
                                } else {
                                    log::debug!("Denying request '{:?}' from authorizer '{:?}'", request, self.authorizer);
                                    let error = ank_base::Response {
                                        request_id: request.request_id,
                                        response_content: Some(ank_base::response::ResponseContent::Error(ank_base::Error {
                                            message: "Access denied".into(),
                                        })),
                                    };
                                    let _ = self.forward_from_server(error).await;
                                };
                            },
                            Ok(ToAnkaios::Hello(to_ankaios::Hello{protocol_version})) => {
                                log::warn!("Received yet another Hello with protocol version '{protocol_version}'");
                                if let Err(message) = check_version_compatibility(protocol_version) {
                                    log::warn!("{message}");
                                    let _ = self.send_connection_closed(message).await;
                                    return;
                                }
                            }
                            Err(error) => {
                                log::warn!("Could not convert protobuf in internal data structure: '{}'", error);
                            }
                        }
                    } else {
                        log::warn!("Could not decode to Ankaios data.");
                        // Beware! There be dragons! This part is needed to test the workloop of the control interface.
                        // There is no other (proper) possibility to get out of the loop as mockall does not work properly with tasks.
                        #[cfg(test)]
                        return;
                    }
                }
            }
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn run_task(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    async fn send_connection_closed(&mut self, reason: String) -> io::Result<()> {
        use control_api::from_ankaios::FromAnkaiosEnum;
        let message = control_api::FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::ConnectionClosed(
                control_api::ConnectionClosed { reason },
            )),
        };

        // [impl->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
        let binary = message.encode_length_delimited_to_vec();
        self.output_stream.write_all(&binary).await?;

        Ok(())
    }

    async fn forward_from_server(&mut self, response: ank_base::Response) -> io::Result<()> {
        use control_api::from_ankaios::FromAnkaiosEnum;
        let message = control_api::FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(response))),
        };

        // [impl->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
        let binary = message.encode_length_delimited_to_vec();
        self.output_stream.write_all(&binary).await?;

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
pub fn generate_test_control_interface_task_mock() -> __mock_MockControlInterfaceTask::__new::Context
{
    let control_interface_task_mock = MockControlInterfaceTask::new_context();
    control_interface_task_mock
        .expect()
        .return_once(|_, _, _, _, _, _| {
            let mut control_interface_task_mock = MockControlInterfaceTask::default();
            control_interface_task_mock
                .expect_run_task()
                .return_once(|| tokio::spawn(async {}));
            control_interface_task_mock
        });
    control_interface_task_mock
}

#[cfg(test)]
mod tests {
    use std::{io::Error, sync::Arc};

    use common::{commands, to_server_interface::ToServer};
    use mockall::{predicate, Sequence};
    use tokio::sync::mpsc;

    use api::{ank_base, control_api};
    use prost::Message;

    use super::ControlInterfaceTask;

    use crate::control_interface::{
        authorizer::MockAuthorizer, control_interface_task::INITIAL_HELLO_MISSING_MSG,
        reopen_file::MockReopenFile,
    };

    const REQUEST_ID: &str = "req_id";

    fn prepare_workload_hello_binary_message(version: impl Into<String>) -> Vec<u8> {
        let workload_hello = control_api::ToAnkaios {
            to_ankaios_enum: Some(control_api::to_ankaios::ToAnkaiosEnum::Hello(
                control_api::Hello {
                    protocol_version: version.into(),
                },
            )),
        };

        workload_hello.encode_to_vec()
    }

    fn prepare_request_complete_state_binary_message(field_mask: impl Into<String>) -> Vec<u8> {
        let ank_request = ank_base::Request {
            request_id: REQUEST_ID.into(),
            request_content: Some(ank_base::request::RequestContent::CompleteStateRequest(
                ank_base::CompleteStateRequest {
                    field_mask: vec![field_mask.into()],
                },
            )),
        };
        let test_output_request = control_api::ToAnkaios {
            to_ankaios_enum: Some(control_api::to_ankaios::ToAnkaiosEnum::Request(ank_request)),
        };

        test_output_request.encode_to_vec()
    }

    #[tokio::test]
    async fn utest_control_interface_task_forward_from_server() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let response = ank_base::Response {
            request_id: REQUEST_ID.into(),
            response_content: Some(ank_base::response::ResponseContent::CompleteState(
                Default::default(),
            )),
        };

        let test_command_binary = control_api::FromAnkaios {
            from_ankaios_enum: Some(control_api::from_ankaios::FromAnkaiosEnum::Response(
                Box::new(response.clone()),
            )),
        }
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

        let mut control_interface_task = ControlInterfaceTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix,
            Arc::new(MockAuthorizer::default()),
        );

        assert!(control_interface_task
            .forward_from_server(response)
            .await
            .is_ok());
    }

    // [utest->swdd~agent-listens-for-requests-from-pipe~1]
    // [utest->swdd~agent-ensures-control-interface-output-pipe-read~1]
    #[tokio::test]
    async fn utest_control_interface_task_run_task_access_denied() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_output_request = control_api::ToAnkaios {
            to_ankaios_enum: Some(control_api::to_ankaios::ToAnkaiosEnum::Request(
                ank_base::Request {
                    request_id: REQUEST_ID.into(),
                    request_content: Some(ank_base::request::RequestContent::CompleteStateRequest(
                        ank_base::CompleteStateRequest { field_mask: vec![] },
                    )),
                },
            )),
        };

        let test_output_request_binary = test_output_request.encode_to_vec();

        let mut mockall_seq = Sequence::new();

        let mut input_stream_mock = MockReopenFile::default();

        let workload_hello_binary = prepare_workload_hello_binary_message(common::ANKAIOS_VERSION);
        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .return_once(move || Ok(workload_hello_binary));

        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .return_once(move || Ok(test_output_request_binary));

        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .returning(move || Err(Error::new(std::io::ErrorKind::Other, "error")));

        let error = ank_base::Response {
            request_id: REQUEST_ID.into(),
            response_content: Some(ank_base::response::ResponseContent::Error(
                ank_base::Error {
                    message: "Access denied".into(),
                },
            )),
        };

        let test_input_command_binary = control_api::FromAnkaios {
            from_ankaios_enum: Some(control_api::from_ankaios::FromAnkaiosEnum::Response(
                Box::new(error.clone()),
            )),
        }
        .encode_length_delimited_to_vec();

        let mut output_stream_mock = MockReopenFile::default();
        output_stream_mock
            .expect_write_all()
            .with(predicate::eq(test_input_command_binary.clone()))
            .once()
            .returning(|_| Ok(()));

        let (_input_pipe_sender, input_pipe_receiver) = mpsc::channel(1);
        let (output_pipe_sender, mut output_pipe_receiver) = mpsc::channel(1);
        let request_id_prefix = String::from("prefix@");

        let mut authorizer = MockAuthorizer::default();
        authorizer.expect_authorize().once().return_const(false);

        let control_interface_task = ControlInterfaceTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix,
            Arc::new(authorizer),
        );

        control_interface_task.run().await;
        assert!(output_pipe_receiver.recv().await.is_none());
    }

    // [utest->swdd~agent-listens-for-requests-from-pipe~1]
    // [utest->swdd~agent-ensures-control-interface-output-pipe-read~1]
    // [utest->swdd~agent-forward-request-from-control-interface-pipe-to-server~1]
    // [utest->swdd~agent-closes-control-interface-on-missing-initial-hello~1]
    #[tokio::test]
    async fn utest_control_interface_task_run_task_access_allowed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let ank_request = ank_base::Request {
            request_id: REQUEST_ID.into(),
            request_content: Some(ank_base::request::RequestContent::CompleteStateRequest(
                ank_base::CompleteStateRequest {
                    field_mask: vec!["desiredState.workloads.nginx".to_string()],
                },
            )),
        };
        let test_output_request = control_api::ToAnkaios {
            to_ankaios_enum: Some(control_api::to_ankaios::ToAnkaiosEnum::Request(
                ank_request.clone(),
            )),
        };

        let test_output_request_binary = test_output_request.encode_to_vec();

        let mut mockall_seq = Sequence::new();

        let mut input_stream_mock = MockReopenFile::default();

        let workload_hello_binary = prepare_workload_hello_binary_message(common::ANKAIOS_VERSION);
        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .return_once(move || Ok(workload_hello_binary));

        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .return_once(move || Ok(test_output_request_binary));

        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .returning(move || Err(Error::new(std::io::ErrorKind::Other, "error")));

        let output_stream_mock = MockReopenFile::default();

        let (_input_pipe_sender, input_pipe_receiver) = mpsc::channel(1);
        let (output_pipe_sender, mut output_pipe_receiver) = mpsc::channel(1);
        let request_id_prefix = "prefix@";

        let mut authorizer = MockAuthorizer::default();
        authorizer.expect_authorize().once().return_const(true);

        let control_interface_task = ControlInterfaceTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix.to_owned(),
            Arc::new(authorizer),
        );

        control_interface_task.run().await;

        let mut expected_request: commands::Request = ank_request.try_into().unwrap();
        expected_request.prefix_request_id(request_id_prefix);
        assert_eq!(
            output_pipe_receiver.recv().await,
            Some(ToServer::Request(expected_request))
        );
    }

    // [utest->swdd~agent-closes-control-interface-on-missing-initial-hello~1]
    #[tokio::test]
    async fn utest_control_interface_task_run_task_no_hello() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_output_request_binary = prepare_request_complete_state_binary_message("");

        let mut mockall_seq = Sequence::new();
        let mut input_stream_mock = MockReopenFile::default();
        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .return_once(move || Ok(test_output_request_binary));

        let test_input_command_binary = control_api::FromAnkaios {
            from_ankaios_enum: Some(
                control_api::from_ankaios::FromAnkaiosEnum::ConnectionClosed(
                    control_api::ConnectionClosed {
                        reason: INITIAL_HELLO_MISSING_MSG.into(),
                    },
                ),
            ),
        }
        .encode_length_delimited_to_vec();

        let mut output_stream_mock = MockReopenFile::default();
        output_stream_mock
            .expect_write_all()
            .with(predicate::eq(test_input_command_binary))
            .once()
            .returning(|_| Ok(()));

        let (_input_pipe_sender, input_pipe_receiver) = mpsc::channel(1);
        let (output_pipe_sender, mut output_pipe_receiver) = mpsc::channel(1);
        let request_id_prefix = "prefix@";

        let authorizer = MockAuthorizer::default();

        let control_interface_task = ControlInterfaceTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix.to_owned(),
            Arc::new(authorizer),
        );

        control_interface_task.run().await;
        assert!(output_pipe_receiver.recv().await.is_none());
    }

    // [utest->swdd~agent-closes-control-interface-on-missing-initial-hello~1]
    #[tokio::test]
    async fn utest_control_interface_task_run_task_hello_unsupported_version() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mockall_seq = Sequence::new();
        let mut input_stream_mock = MockReopenFile::default();

        let unsupported_version = "1999.1.0";
        let workload_hello_binary = prepare_workload_hello_binary_message(unsupported_version);
        input_stream_mock
            .expect_read_protobuf_data()
            .once()
            .in_sequence(&mut mockall_seq)
            .return_once(move || Ok(workload_hello_binary));

        let test_input_command_binary = control_api::FromAnkaios {
            from_ankaios_enum: Some(
                control_api::from_ankaios::FromAnkaiosEnum::ConnectionClosed(
                    control_api::ConnectionClosed {
                        reason: format!("Unsupported protocol version '{unsupported_version}'. Currently supported '{}'", common::ANKAIOS_VERSION),
                    },
                ),
            ),
        }
        .encode_length_delimited_to_vec();

        let mut output_stream_mock = MockReopenFile::default();
        output_stream_mock
            .expect_write_all()
            .with(predicate::eq(test_input_command_binary))
            .once()
            .returning(|_| Ok(()));

        let (_input_pipe_sender, input_pipe_receiver) = mpsc::channel(1);
        let (output_pipe_sender, mut output_pipe_receiver) = mpsc::channel(1);
        let request_id_prefix = "prefix@";

        let authorizer = MockAuthorizer::default();

        let control_interface_task = ControlInterfaceTask::new(
            output_stream_mock,
            input_stream_mock,
            input_pipe_receiver,
            output_pipe_sender,
            request_id_prefix.to_owned(),
            Arc::new(authorizer),
        );

        control_interface_task.run().await;
        assert!(output_pipe_receiver.recv().await.is_none());
    }
}
