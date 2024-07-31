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

use std::{mem::take, time::Duration};

use crate::filtered_complete_state::FilteredCompleteState;
use crate::{output_and_error, output_debug};
use api::ank_base;
use common::communications_client::CommunicationsClient;
use common::communications_error::CommunicationMiddlewareError;
use common::to_server_interface::ToServer;
use common::{
    commands::{CompleteStateRequest, UpdateWorkloadState},
    from_server_interface::{FromServer, FromServerReceiver},
    objects::CompleteState,
    to_server_interface::{ToServerInterface, ToServerSender},
};
use grpc::client::GRPCCommunicationsClient;
use grpc::security::TLSConfig;
#[cfg(test)]
use mockall::automock;

const BUFFER_SIZE: usize = 20;
const WAIT_TIME_MS: Duration = Duration::from_millis(3000);

pub struct ServerConnection {
    to_server: ToServerSender,
    from_server: FromServerReceiver,
    task: tokio::task::JoinHandle<()>,
    missed_from_server_messages: Vec<FromServer>,
}

#[cfg_attr(test, automock)]
impl ServerConnection {
    // [impl->swdd~server-handle-cli-communication~1]
    // [impl->swdd~cli-communication-over-middleware~1]
    // testing the function does not bring any benefit so disable the dead code warning when building for test
    #[cfg_attr(test, allow(dead_code))]
    pub fn new(
        cli_name: &str,
        server_url: String,
        tls_config: Option<TLSConfig>,
    ) -> Result<Self, CommunicationMiddlewareError> {
        let mut grpc_communications_client = GRPCCommunicationsClient::new_cli_communication(
            cli_name.to_owned(),
            server_url,
            tls_config,
        )?;

        let (to_cli, cli_receiver) = tokio::sync::mpsc::channel::<FromServer>(BUFFER_SIZE);
        let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);

        let task = tokio::spawn(async move {
            if let Err(err) = grpc_communications_client
                .run(server_receiver, to_cli.clone())
                .await
            {
                output_and_error!("{err}");
            }
        });

        Ok(Self {
            to_server,
            from_server: cli_receiver,
            task,
            missed_from_server_messages: Vec::new(),
        })
    }

    // testing the function does not bring any benefit so disable the dead code warning when building for test
    #[cfg_attr(test, allow(dead_code))]
    pub async fn shut_down(self) {
        drop(self.to_server);

        let _ = self.task.await;
    }

    pub async fn get_complete_state(
        &mut self,
        object_field_mask: &Vec<String>,
    ) -> Result<FilteredCompleteState, ServerConnectionError> {
        output_debug!(
            "get_complete_state: object_field_mask={:?} ",
            object_field_mask
        );

        let request_id = uuid::Uuid::new_v4().to_string();

        self.to_server
            .request_complete_state(
                request_id.to_owned(),
                CompleteStateRequest {
                    field_mask: object_field_mask.clone(),
                },
            )
            .await
            .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))?;

        let poll_complete_state_response = async {
            loop {
                match self.from_server.recv().await {
                    Some(FromServer::Response(ank_base::Response {
                        request_id: received_request_id,
                        response_content:
                            Some(ank_base::response::ResponseContent::CompleteState(res)),
                    })) if received_request_id == request_id => {
                        output_debug!("Received from server: {res:?} ");
                        return Ok(res.into());
                    }
                    None => return Err("Channel preliminary closed."),
                    Some(message) => {
                        // [impl->swdd~cli-stores-unexpected-message~1]
                        self.missed_from_server_messages.push(message);
                    }
                }
            }
        };
        match tokio::time::timeout(WAIT_TIME_MS, poll_complete_state_response).await {
            Ok(Ok(res)) => Ok(res),
            Ok(Err(err)) => Err(ServerConnectionError::ExecutionError(format!(
                "Failed to get complete state.\nError: {err}"
            ))),
            Err(_) => Err(ServerConnectionError::ExecutionError(format!(
                "Failed to get complete state in time (timeout={WAIT_TIME_MS:?})."
            ))),
        }
    }

    pub async fn update_state(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<ank_base::UpdateStateSuccess, ServerConnectionError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        output_debug!("Sending the new state {:?}", new_state);
        self.to_server
            .update_state(request_id.clone(), new_state, update_mask)
            .await
            .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))?;

        let poll_update_state_success = async {
            loop {
                let Some(server_message) = self.from_server.recv().await else {
                    return Err(ServerConnectionError::ExecutionError(
                        "Connection to server interrupted".into(),
                    ));
                };
                match server_message {
                    FromServer::Response(ank_base::Response {
                        request_id: received_request_id,
                        response_content:
                            Some(ank_base::response::ResponseContent::UpdateStateSuccess(
                                update_state_success,
                            )),
                    }) if received_request_id == request_id => return Ok(update_state_success),
                    // [impl->swdd~cli-requests-update-state-with-watch-error~1]
                    FromServer::Response(ank_base::Response {
                        request_id: received_request_id,
                        response_content: Some(ank_base::response::ResponseContent::Error(error)),
                    }) if received_request_id == request_id => {
                        return Err(ServerConnectionError::ExecutionError(format!(
                            "SetState failed with: '{}'",
                            error.message
                        )));
                    }
                    message => {
                        // [impl->swdd~cli-stores-unexpected-message~1]
                        self.missed_from_server_messages.push(message);
                    }
                }
            }
        };
        match tokio::time::timeout(WAIT_TIME_MS, poll_update_state_success).await {
            Ok(Ok(res)) => {
                output_debug!("Got update success: {:?}", res);
                Ok(res)
            }
            Ok(Err(err)) => {
                output_debug!("Update failed: {:?}", err);
                Err(err)
            }
            Err(_) => Err(ServerConnectionError::ExecutionError(format!(
                "Failed to get complete state in time (timeout={WAIT_TIME_MS:?})."
            ))),
        }
    }

    pub async fn read_next_update_workload_state(
        &mut self,
    ) -> Result<UpdateWorkloadState, ServerConnectionError> {
        loop {
            let server_message = self.from_server.recv().await;
            output_debug!("Got server message: {:?}", server_message);
            let Some(server_message) = server_message else {
                break Err(ServerConnectionError::ExecutionError(
                    "Connection to server interrupted".into(),
                ));
            };
            if let FromServer::UpdateWorkloadState(update_workload_state) = server_message {
                break Ok(update_workload_state);
            } else {
                // [impl->swdd~cli-stores-unexpected-message~1]
                self.missed_from_server_messages.push(server_message);
            };
        }
    }

    pub fn take_missed_from_server_messages(&mut self) -> Vec<FromServer> {
        take(&mut self.missed_from_server_messages)
    }
}

#[derive(Debug)]
pub enum ServerConnectionError {
    ExecutionError(String),
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
    use std::collections::HashMap;

    use super::ank_base::{self, UpdateStateSuccess};
    use common::{
        commands::{CompleteStateRequest, RequestContent, UpdateStateRequest, UpdateWorkloadState},
        from_server_interface::FromServer,
        objects::{
            CompleteState, ExecutionState, State, StoredWorkloadSpec, WorkloadInstanceName,
            WorkloadState,
        },
        test_utils,
        to_server_interface::ToServer,
    };
    use tokio::sync::mpsc::Receiver;

    use super::ServerConnection;

    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const AGENT_A: &str = "agent_A";
    const RUNTIME: &str = "runtime";
    const REQUEST: &str = "request";
    const OTHER_REQUEST: &str = "other_request";
    const FIELD_MASK: &str = "field_mask";
    const ID: &str = "id";

    #[derive(Default)]
    struct CommunicationSimulator {
        actions: Vec<CommunicationSimulatorAction>,
    }

    struct CorrectCommuncationChecker {
        join_handle: tokio::task::JoinHandle<()>,
        is_ready: tokio::sync::oneshot::Receiver<Receiver<ToServer>>,
    }

    #[derive(Clone)]
    enum CommunicationSimulatorAction {
        WillSendMessage(FromServer),
        WillSendResponse(String, ank_base::response::ResponseContent),
        ExpectReceiveRequest(String, RequestContent),
    }

    impl CommunicationSimulator {
        fn create_server_connection(self) -> (CorrectCommuncationChecker, ServerConnection) {
            let (from_server, cli_receiver) = tokio::sync::mpsc::channel::<FromServer>(1);
            let (to_server, mut server_receiver) = tokio::sync::mpsc::channel::<ToServer>(1);

            let (is_ready_sender, is_ready) = tokio::sync::oneshot::channel();

            let join_handle = tokio::spawn(async move {
                let mut request_ids = HashMap::<String, String>::new();
                for action in self.actions {
                    match action {
                        CommunicationSimulatorAction::WillSendMessage(message) => {
                            from_server.send(message).await.unwrap()
                        }
                        CommunicationSimulatorAction::WillSendResponse(request_name, response) => {
                            let request_id = request_ids.get(&request_name).unwrap();
                            from_server
                                .send(FromServer::Response(ank_base::Response {
                                    request_id: request_id.to_owned(),
                                    response_content: Some(response),
                                }))
                                .await
                                .unwrap();
                        }
                        CommunicationSimulatorAction::ExpectReceiveRequest(
                            request_name,
                            expected_request,
                        ) => {
                            let actual_message = server_receiver.recv().await.unwrap();
                            let common::to_server_interface::ToServer::Request(actual_request) =
                                actual_message
                            else {
                                panic!("Expected a request")
                            };
                            request_ids.insert(request_name, actual_request.request_id);
                            assert_eq!(actual_request.request_content, expected_request);
                        }
                    }
                }
                is_ready_sender.send(server_receiver).unwrap();
            });

            (
                CorrectCommuncationChecker {
                    join_handle,
                    is_ready,
                },
                ServerConnection {
                    to_server,
                    from_server: cli_receiver,
                    task: tokio::spawn(async {}),
                    missed_from_server_messages: Vec::new(),
                },
            )
        }

        pub fn will_send_message(&mut self, message: FromServer) {
            self.actions
                .push(CommunicationSimulatorAction::WillSendMessage(message));
        }

        pub fn will_send_response(
            &mut self,
            request_name: &str,
            response: ank_base::response::ResponseContent,
        ) {
            self.actions
                .push(CommunicationSimulatorAction::WillSendResponse(
                    request_name.to_string(),
                    response,
                ));
        }

        pub fn expect_receive_request(&mut self, request_name: &str, request: RequestContent) {
            self.actions
                .push(CommunicationSimulatorAction::ExpectReceiveRequest(
                    request_name.to_string(),
                    request,
                ));
        }
    }

    impl CorrectCommuncationChecker {
        fn check_communication(mut self) {
            let Ok(mut to_server) = self.is_ready.try_recv() else {
                panic!("Not all messages have been sent or received");
            };
            self.join_handle.abort();
            if let Ok(message) = to_server.try_recv() {
                panic!("Received unexpected message: {:#?}", message);
            }
        }
    }

    impl Drop for CorrectCommuncationChecker {
        fn drop(&mut self) {
            self.join_handle.abort();
        }
    }

    fn complete_state(workload_name: &str) -> CompleteState {
        CompleteState {
            desired_state: State {
                workloads: [(
                    workload_name.into(),
                    StoredWorkloadSpec {
                        agent: AGENT_A.into(),
                        runtime: RUNTIME.into(),
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn instance_name(workload_name: &str) -> WorkloadInstanceName {
        format!("{workload_name}.{ID}.{AGENT_A}")
            .try_into()
            .unwrap()
    }

    #[tokio::test]
    async fn utest_get_complete_state() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![FIELD_MASK.into()],
            }),
        );
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::CompleteState(
                test_utils::generate_test_proto_complete_state(&[(
                    WORKLOAD_NAME_1,
                    ank_base::Workload {
                        agent: Some(AGENT_A.to_string()),
                        runtime: Some(RUNTIME.to_string()),
                        tags: Some(ank_base::Tags { tags: vec![] }),
                        dependencies: Some(ank_base::Dependencies {
                            dependencies: HashMap::new(),
                        }),
                        restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                        runtime_config: Some("".to_string()),
                        control_interface_access: None,
                    },
                )]),
            ),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&vec![FIELD_MASK.into()])
            .await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            (test_utils::generate_test_proto_complete_state(&[(
                WORKLOAD_NAME_1,
                ank_base::Workload {
                    agent: Some(AGENT_A.to_string()),
                    runtime: Some(RUNTIME.to_string()),
                    tags: Some(ank_base::Tags { tags: vec![] }),
                    dependencies: Some(ank_base::Dependencies {
                        dependencies: HashMap::new()
                    }),
                    restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                    runtime_config: Some("".to_string()),
                    control_interface_access: None
                },
            )])
            .into())
        );
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_get_complete_state_fails_at_request() {
        let sim = CommunicationSimulator::default();
        let (_, mut server_connection) = sim.create_server_connection();
        // sending the GetCompleteState request to the server, shall already fail
        let (to_server, _) = tokio::sync::mpsc::channel(1);
        server_connection.to_server = to_server;

        let result = server_connection
            .get_complete_state(&vec![FIELD_MASK.into()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_get_complete_state_fails_no_response() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![FIELD_MASK.into()],
            }),
        );
        let (_checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&vec![FIELD_MASK.into()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_get_complete_state_fails_response_timeout() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![FIELD_MASK.into()],
            }),
        );
        let (checker, mut server_connection) = sim.create_server_connection();
        let (_to_client, from_server) = tokio::sync::mpsc::channel(1);
        server_connection.from_server = from_server;

        let result = server_connection
            .get_complete_state(&vec![FIELD_MASK.into()])
            .await;
        assert!(result.is_err());
        checker.check_communication();
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_get_complete_state_other_response_in_between() {
        let other_response = FromServer::Response(ank_base::Response {
            request_id: OTHER_REQUEST.into(),
            response_content: Some(ank_base::response::ResponseContent::CompleteState(
                test_utils::generate_test_proto_complete_state(&[(
                    WORKLOAD_NAME_2,
                    ank_base::Workload {
                        agent: Some(AGENT_A.to_string()),
                        runtime: Some(RUNTIME.to_string()),
                        tags: Some(ank_base::Tags { tags: vec![] }),
                        dependencies: Some(ank_base::Dependencies {
                            dependencies: HashMap::new(),
                        }),
                        restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                        runtime_config: Some("".to_string()),
                        control_interface_access: None,
                    },
                )]),
            )),
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![FIELD_MASK.into()],
            }),
        );
        sim.will_send_message(other_response.clone());
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::CompleteState(
                test_utils::generate_test_proto_complete_state(&[(
                    WORKLOAD_NAME_1,
                    ank_base::Workload {
                        agent: Some(AGENT_A.to_string()),
                        runtime: Some(RUNTIME.to_string()),
                        tags: Some(ank_base::Tags { tags: vec![] }),
                        dependencies: Some(ank_base::Dependencies {
                            dependencies: HashMap::new(),
                        }),
                        restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                        runtime_config: Some("".to_string()),
                        control_interface_access: None,
                    },
                )]),
            ),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&vec![FIELD_MASK.into()])
            .await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            (test_utils::generate_test_proto_complete_state(&[(
                WORKLOAD_NAME_1,
                ank_base::Workload {
                    agent: Some(AGENT_A.to_string()),
                    runtime: Some(RUNTIME.to_string()),
                    tags: Some(ank_base::Tags { tags: vec![] }),
                    dependencies: Some(ank_base::Dependencies {
                        dependencies: HashMap::new()
                    }),
                    restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                    runtime_config: Some("".to_string()),
                    control_interface_access: None
                },
            )])
            .into())
        );
        assert_eq!(
            server_connection.take_missed_from_server_messages(),
            vec![other_response]
        );
        checker.check_communication();
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_get_complete_state_other_message_in_between() {
        let other_message = FromServer::UpdateWorkloadState(UpdateWorkloadState {
            workload_states: vec![],
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![FIELD_MASK.into()],
            }),
        );
        sim.will_send_message(other_message.clone());
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::CompleteState(
                test_utils::generate_test_proto_complete_state(&[(
                    WORKLOAD_NAME_1,
                    ank_base::Workload {
                        agent: Some(AGENT_A.to_string()),
                        runtime: Some(RUNTIME.to_string()),
                        tags: Some(ank_base::Tags { tags: vec![] }),
                        dependencies: Some(ank_base::Dependencies {
                            dependencies: HashMap::new(),
                        }),
                        restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                        runtime_config: Some("".to_string()),
                        control_interface_access: None,
                    },
                )]),
            ),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&vec![FIELD_MASK.into()])
            .await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            (test_utils::generate_test_proto_complete_state(&[(
                WORKLOAD_NAME_1,
                ank_base::Workload {
                    agent: Some(AGENT_A.to_string()),
                    runtime: Some(RUNTIME.to_string()),
                    tags: Some(ank_base::Tags { tags: vec![] }),
                    dependencies: Some(ank_base::Dependencies {
                        dependencies: HashMap::new()
                    }),
                    restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                    runtime_config: Some("".to_string()),
                    control_interface_access: None
                },
            )])
            .into())
        );
        assert_eq!(
            server_connection.take_missed_from_server_messages(),
            vec![other_message]
        );
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_update_state() {
        let update_state_success = UpdateStateSuccess {
            added_workloads: vec![WORKLOAD_NAME_1.into()],
            deleted_workloads: vec![],
        };

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: complete_state(WORKLOAD_NAME_1),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::UpdateStateSuccess(update_state_success.clone()),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), update_state_success);
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_update_state_fails_at_request() {
        let sim = CommunicationSimulator::default();
        let (_, mut server_connection) = sim.create_server_connection();
        // sending the GetCompleteState request to the server, shall already fail
        let (to_server, _) = tokio::sync::mpsc::channel(1);
        server_connection.to_server = to_server;

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_update_state_fails_no_response() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: complete_state(WORKLOAD_NAME_1),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );

        let (_, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_update_state_fails_error_response() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: complete_state(WORKLOAD_NAME_1),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::Error(ank_base::Error { message: "".into() }),
        );

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_err());
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_update_state_fails_response_timeout() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: complete_state(WORKLOAD_NAME_1),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );

        let (checker, mut server_connection) = sim.create_server_connection();
        let (_to_client, from_server) = tokio::sync::mpsc::channel(1);
        server_connection.from_server = from_server;

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_err());
        checker.check_communication();
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_update_state_other_response_in_between() {
        let update_state_success = UpdateStateSuccess {
            added_workloads: vec![WORKLOAD_NAME_1.into()],
            deleted_workloads: vec![],
        };
        let other_response = FromServer::Response(ank_base::Response {
            request_id: OTHER_REQUEST.into(),
            response_content: Some(ank_base::response::ResponseContent::CompleteState(
                test_utils::generate_test_proto_complete_state(&[(
                    WORKLOAD_NAME_2,
                    ank_base::Workload {
                        agent: Some(AGENT_A.to_string()),
                        runtime: Some(RUNTIME.to_string()),
                        tags: Some(ank_base::Tags { tags: vec![] }),
                        dependencies: Some(ank_base::Dependencies {
                            dependencies: HashMap::new(),
                        }),
                        restart_policy: Some(ank_base::RestartPolicy::Never as i32),
                        runtime_config: Some("".to_string()),
                        control_interface_access: None,
                    },
                )]),
            )),
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: complete_state(WORKLOAD_NAME_1),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_message(other_response.clone());
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::UpdateStateSuccess(update_state_success.clone()),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), update_state_success);
        assert_eq!(
            server_connection.take_missed_from_server_messages(),
            vec![other_response]
        );
        checker.check_communication();
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_update_state_other_message_in_between() {
        let update_state_success = UpdateStateSuccess {
            added_workloads: vec![WORKLOAD_NAME_1.into()],
            deleted_workloads: vec![],
        };
        let other_message = FromServer::UpdateWorkloadState(UpdateWorkloadState {
            workload_states: vec![],
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            REQUEST,
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: complete_state(WORKLOAD_NAME_1),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_message(other_message.clone());
        sim.will_send_response(
            REQUEST,
            ank_base::response::ResponseContent::UpdateStateSuccess(update_state_success.clone()),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(complete_state(WORKLOAD_NAME_1), vec![FIELD_MASK.into()])
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), update_state_success);
        assert_eq!(
            server_connection.take_missed_from_server_messages(),
            vec![other_message]
        );
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_read_next_update_workload_state() {
        let update_workload_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                instance_name: instance_name(WORKLOAD_NAME_1),
                execution_state: ExecutionState::running(),
            }],
        };

        let mut sim = CommunicationSimulator::default();
        sim.will_send_message(FromServer::UpdateWorkloadState(
            update_workload_state.clone(),
        ));
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection.read_next_update_workload_state().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), update_workload_state);

        checker.check_communication();
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_read_next_update_workload_state_other_message_in_between() {
        let other_message = FromServer::Response(ank_base::Response {
            request_id: REQUEST.into(),
            response_content: Some(ank_base::response::ResponseContent::Error(
                ank_base::Error { message: "".into() },
            )),
        });
        let update_workload_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                instance_name: instance_name(WORKLOAD_NAME_1),
                execution_state: ExecutionState::running(),
            }],
        };

        let mut sim = CommunicationSimulator::default();
        sim.will_send_message(other_message.clone());
        sim.will_send_message(FromServer::UpdateWorkloadState(
            update_workload_state.clone(),
        ));
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection.read_next_update_workload_state().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), update_workload_state);
        assert_eq!(
            server_connection.take_missed_from_server_messages(),
            vec![other_message]
        );
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_read_next_update_workload_state_fails_no_response() {
        let sim = CommunicationSimulator::default();

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection.read_next_update_workload_state().await;
        assert!(result.is_err());

        checker.check_communication();
    }
}
