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

use crate::cli::LogsArgs;
#[cfg_attr(test, mockall_double::double)]
use crate::cli_signals::SignalHandler;
use crate::{output_and_error, output_debug};

use ankaios_api::ank_base::{
    CompleteState, CompleteStateRequestSpec, CompleteStateResponse, CompleteStateSpec, LogEntry,
    LogsRequestAccepted, LogsRequestSpec, Response, ResponseContent, UpdateStateSuccess,
    WorkloadInstanceName, WorkloadInstanceNameSpec,
};
use common::{
    commands::UpdateWorkloadState,
    communications_client::CommunicationsClient,
    communications_error::CommunicationMiddlewareError,
    from_server_interface::{FromServer, FromServerReceiver},
    to_server_interface::{ToServer, ToServerInterface, ToServerSender},
};
use grpc::{client::GRPCCommunicationsClient, security::TLSConfig};
use std::{collections::BTreeSet, mem::take, time::Duration};
use tokio::{sync::mpsc, task::JoinHandle, time::timeout};
use uuid::Uuid;

#[cfg(not(test))]
use {common::std_extensions::IllegalStateResult, std::io::Write};

#[cfg(test)]
use mockall::automock;

const BUFFER_SIZE: usize = 20;
const WAIT_TIME_MS: Duration = Duration::from_millis(3000);

pub struct ServerConnection {
    to_server: ToServerSender,
    from_server: FromServerReceiver,
    task: JoinHandle<()>,
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

        let (to_cli, cli_receiver) = mpsc::channel::<FromServer>(BUFFER_SIZE);
        let (to_server, server_receiver) = mpsc::channel::<ToServer>(BUFFER_SIZE);

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
        object_field_mask: &[String],
    ) -> Result<CompleteState, ServerConnectionError> {
        output_debug!(
            "get_complete_state: object_field_mask={:?} ",
            object_field_mask
        );

        let request_id = Uuid::new_v4().to_string();

        self.to_server
            .request_complete_state(
                request_id.to_owned(),
                CompleteStateRequestSpec {
                    field_mask: object_field_mask.to_vec(),
                    subscribe_for_events: false,
                },
            )
            .await
            .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))?;

        let poll_complete_state_response = async {
            loop {
                match self.from_server.recv().await {
                    Some(FromServer::Response(Response {
                        request_id: received_request_id,
                        response_content: Some(ResponseContent::CompleteStateResponse(res)),
                    })) if received_request_id == request_id => {
                        output_debug!("Received from server: {res:?} ");
                        return Ok(res.complete_state.unwrap_or_default());
                    }
                    None => return Err("Channel preliminary closed."),
                    Some(message) => {
                        // [impl->swdd~cli-stores-unexpected-message~1]
                        self.missed_from_server_messages.push(message);
                    }
                }
            }
        };
        match timeout(WAIT_TIME_MS, poll_complete_state_response).await {
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
        new_state: CompleteStateSpec,
        update_mask: Vec<String>,
    ) -> Result<UpdateStateSuccess, ServerConnectionError> {
        let request_id = Uuid::new_v4().to_string();
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
                    FromServer::Response(Response {
                        request_id: received_request_id,
                        response_content:
                            Some(ResponseContent::UpdateStateSuccess(update_state_success)),
                    }) if received_request_id == request_id => return Ok(update_state_success),
                    // [impl->swdd~cli-requests-update-state-with-watch-error~1]
                    FromServer::Response(Response {
                        request_id: received_request_id,
                        response_content: Some(ResponseContent::Error(error)),
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
        match timeout(WAIT_TIME_MS, poll_update_state_success).await {
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

    // [impl->swdd~cli-streams-logs-from-the-server~1]
    pub async fn stream_logs(
        &mut self,
        instance_names: BTreeSet<WorkloadInstanceNameSpec>,
        args: LogsArgs,
    ) -> Result<(), ServerConnectionError> {
        let request_id = Uuid::new_v4().to_string();

        let output_workload_names = args.output_names;

        self.send_logs_request_for_workloads(
            &request_id,
            instance_names.clone().into_iter().collect(),
            args,
        )
        .await?;

        let logs_request_accepted_response =
            self.get_logs_accepted_response(request_id.clone()).await?;

        self.compare_requested_with_accepted_workloads(
            &instance_names,
            logs_request_accepted_response.workload_names,
        )?;

        let output_logs_fn = select_log_format_function(&instance_names, output_workload_names);

        self.listen_for_workload_logs(request_id, instance_names, output_logs_fn)
            .await
    }

    async fn send_logs_request_for_workloads(
        &mut self,
        request_id: &str,
        workload_instance_names: Vec<WorkloadInstanceNameSpec>,
        args: LogsArgs,
    ) -> Result<(), ServerConnectionError> {
        let logs_request = LogsRequestSpec {
            workload_names: workload_instance_names,
            follow: args.follow,
            tail: args.tail,
            since: args.since,
            until: args.until,
        };

        self.to_server
            .logs_request(request_id.to_string(), logs_request.into())
            .await
            .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))
    }

    async fn get_logs_accepted_response(
        &mut self,
        request_id: String,
    ) -> Result<LogsRequestAccepted, ServerConnectionError> {
        timeout(
            WAIT_TIME_MS,
            self.poll_logs_request_accepted_response(request_id),
        )
        .await
        .unwrap_or_else(|_| {
            Err(ServerConnectionError::ExecutionError(format!(
                "Failed to get LogsRequestAccepted response in time (timeout={WAIT_TIME_MS:?})."
            )))
        })
    }

    async fn poll_logs_request_accepted_response(
        &mut self,
        request_id: String,
    ) -> Result<LogsRequestAccepted, ServerConnectionError> {
        loop {
            match self.from_server.recv().await {
                Some(FromServer::Response(Response {
                    request_id: incoming_request_id,
                    response_content:
                        Some(ResponseContent::LogsRequestAccepted(
                            logs_request_accepted_response,
                        )),
                    })) if request_id == incoming_request_id => {
                        output_debug!(
                            "LogsRequest accepted of request id '{}' for the following workload instance names: {:?}",
                            request_id,
                            logs_request_accepted_response.workload_names
                        );
                        break Ok(logs_request_accepted_response);
                }
                Some(FromServer::Response(Response {
                    request_id: incoming_request_id,
                    response_content:
                        Some(ResponseContent::Error(error)),
                })) if request_id == incoming_request_id => {
                    break Err(ServerConnectionError::ExecutionError(format!(
                        "Server replied with error: '{}'",
                        error.message
                    )));
                }
                Some(unexpected_message) => {
                    output_debug!("Ignore received unexpected message while waiting for LogsRequestAccepted response: {unexpected_message:?}");
                    /* The unexpected message is not added to the queue of missed messages,
                    because the current intend is to receive logs. There is no need to wait for
                    additional messages like UpdateWorkloadState messages. */
                },
                None => break Err(ServerConnectionError::ExecutionError(
                    "Connection to server interrupted while waiting for LogsRequestAccepted response."
                        .to_string(),
                )),
            }
        }
    }

    fn compare_requested_with_accepted_workloads(
        &self,
        requested_workloads: &BTreeSet<WorkloadInstanceNameSpec>,
        accepted_workloads: Vec<WorkloadInstanceName>,
    ) -> Result<(), ServerConnectionError> {
        for instance_name in requested_workloads {
            let instance_name = instance_name.to_owned().into();
            if !accepted_workloads.contains(&instance_name) {
                return Err(ServerConnectionError::ExecutionError(format!(
                    "Workload '{}' is not accepted by the server to receive logs from.",
                    instance_name.workload_name,
                )));
            }
        }

        Ok(())
    }

    async fn listen_for_workload_logs(
        &mut self,
        request_id: String,
        mut instance_names: BTreeSet<WorkloadInstanceNameSpec>,
        output_log_format_function: fn(Vec<LogEntry>),
    ) -> Result<(), ServerConnectionError> {
        loop {
            tokio::select! {
                // [impl->swdd~cli-sends-logs-cancel-request-upon-termination~1]
                _ = SignalHandler::wait_for_signals() => {
                    self.to_server
                        .logs_cancel_request(request_id).await
                        .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))?;

                    output_debug!("LogsCancelRequest sent after receiving signal to stop.");
                    break Ok(());
                }
                // [impl->swdd~cli-handles-log-responses-from-server~1]
                server_message = self.from_server.recv() => {
                    let server_message = server_message
                        .ok_or(
                            ServerConnectionError::ExecutionError("Error streaming workload logs: channel preliminary closed.".to_string()
                    ))?;

                    match handle_server_log_response(&request_id, server_message)? {
                        LogStreamingState::Output(log_entries) => {
                            output_log_format_function(log_entries.log_entries);
                        }
                        LogStreamingState::Continue => continue,
                        // [impl->swdd~cli-stops-log-output-for-specific-workloads~1]
                        LogStreamingState::StopForWorkload(instance_name) => {
                            instance_names.remove(&instance_name);

                            if instance_names.is_empty() {
                                // log streaming is finished for all requested instances
                                output_debug!("All requested workload instances have been processed. Stopping log streaming.");
                                break Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    // [impl->swdd~cli-subscribes-for-events~1]
    pub async fn subscribe_and_listen_for_events(
        &mut self,
        field_mask: Vec<String>,
        detailed: bool,
    ) -> Result<EventSubscription, ServerConnectionError> {
        output_debug!(
            "Subscribing for events from server with field mask: {:?}",
            field_mask
        );
        let request_id = uuid::Uuid::new_v4().to_string();
        let events_request = CompleteStateRequestSpec {
            field_mask,
            subscribe_for_events: true,
        };

        self.to_server
            .request_complete_state(request_id.clone(), events_request)
            .await
            .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))?;

        Ok(EventSubscription {
            request_id,
            initial_response_received: false,
            output_initial_response: detailed,
        })
    }

    // [impl->swdd~cli-receives-events~1]
    // [impl->swdd~cli-handles-event-subscription-errors~1]
    pub async fn receive_next_event(
        &mut self,
        subscription: &mut EventSubscription,
    ) -> Result<Option<CompleteStateResponse>, ServerConnectionError> {
        output_debug!("Listening for events from server...");
        loop {
            tokio::select! {
                _ = SignalHandler::wait_for_signals() => {
                    output_debug!("Received signal to stop event listening.");
                    return Ok(None);
                }
                server_message = self.from_server.recv() => {
                    match server_message {
                        Some(FromServer::Response(Response {
                            request_id: received_request_id,
                            response_content:
                                Some(ResponseContent::CompleteStateResponse(res)),
                        })) if received_request_id == subscription.request_id => {
                            if !subscription.initial_response_received {
                                output_debug!("Received initial state response, subscription active");
                                subscription.initial_response_received = true;

                                if subscription.output_initial_response {
                                    return Ok(Some(*res));
                                }
                            } else {
                                output_debug!("Received event from server: {res:?}");
                                return Ok(Some(*res));
                            }
                        }
                        Some(FromServer::Response(Response {
                            request_id: received_request_id,
                            response_content:
                                Some(ResponseContent::Error(error)),
                        })) if received_request_id == subscription.request_id => {
                            return Err(ServerConnectionError::ExecutionError(format!(
                                "Event subscription failed: '{}'",
                                error.message
                            )));
                        }
                        None => {
                            return Err(ServerConnectionError::ExecutionError(
                                "Connection to server interrupted".into(),
                            ));
                        }
                        Some(message) => {
                            output_debug!("Received unexpected message: {:?}", message);
                        }
                    }
                }
            }
        }
    }
}

pub struct EventSubscription {
    pub(crate) request_id: String,
    pub(crate) initial_response_received: bool,
    pub(crate) output_initial_response: bool,
}

// [impl->swdd~cli-handles-log-responses-from-server~1]
fn handle_server_log_response(
    request_id: &String,
    server_message: FromServer,
) -> Result<LogStreamingState, ServerConnectionError> {
    match server_message {
        FromServer::Response(Response {
            request_id: received_request_id,
            response_content: Some(ResponseContent::LogEntriesResponse(logs_response)),
        }) if &received_request_id == request_id => Ok(LogStreamingState::Output(logs_response)),
        FromServer::Response(Response {
            request_id: received_request_id,
            response_content: Some(ResponseContent::LogsStopResponse(logs_stop_response)),
        }) if &received_request_id == request_id => {
            let workload_instance_name =
                logs_stop_response
                    .workload_name
                    .ok_or(ServerConnectionError::ExecutionError(
                        "Received invalid LogsStopResponse without workload name".to_string(),
                    ))?;

            output_debug!(
                "Received stop message for workload instance: {:?}",
                workload_instance_name
            );
            Ok(LogStreamingState::StopForWorkload(
                workload_instance_name.try_into().unwrap(),
            ))
        }

        FromServer::Response(Response {
            request_id: received_request_id,
            response_content: Some(ResponseContent::Error(error)),
        }) if &received_request_id == request_id => Err(ServerConnectionError::ExecutionError(
            format!("Error streaming logs: '{}'", error.message),
        )),
        // ignore all other messages sent by the server while streaming logs
        unexpected_response => {
            output_debug!(
                "Received an unexpected response from the server: {:?}",
                unexpected_response
            );
            Ok(LogStreamingState::Continue)
        }
    }
}

// [impl->swdd~cli-outputs-logs-in-specific-format~1]
fn select_log_format_function(
    instance_names: &BTreeSet<WorkloadInstanceNameSpec>,
    force_output_names: bool,
) -> fn(Vec<LogEntry>) {
    if is_output_with_workload_names(instance_names, force_output_names) {
        output_logs_with_workload_names
    } else {
        output_logs_without_workload_names
    }
}

fn is_output_with_workload_names(
    instance_names: &BTreeSet<WorkloadInstanceNameSpec>,
    force_output_names: bool,
) -> bool {
    instance_names.len() > 1 || force_output_names
}

enum LogStreamingState {
    StopForWorkload(WorkloadInstanceNameSpec),
    Continue,
    Output(ankaios_api::ank_base::LogEntriesResponse),
}

#[derive(Debug, PartialEq)]
pub enum ServerConnectionError {
    ExecutionError(String),
}

// [impl->swdd~cli-outputs-logs-in-specific-format~1]
fn output_logs_with_workload_names(log_entries: Vec<LogEntry>) {
    log_entries.iter().for_each(|log_entry| {
        let workload_instance_name = log_entry.workload_name.as_ref().unwrap_or_else(|| {
            crate::output_and_error!(
                "Failed to output log: workload name is not available inside log entry."
            )
        });

        let workload_name = workload_instance_name.workload_name.as_str();
        let formatted_log = format!("{} {}\n", workload_name, log_entry.message);
        print_log(&formatted_log);
    });
}

// [impl->swdd~cli-outputs-logs-in-specific-format~1]
fn output_logs_without_workload_names(log_entries: Vec<LogEntry>) {
    log_entries.iter().for_each(|log_entry| {
        print_log(&format!("{}\n", log_entry.message));
    });
}

#[cfg(not(test))]
fn print_log(log_line: &str) {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    stdout.write(log_line.as_bytes()).unwrap_or_illegal_state();
}

#[cfg(test)]
fn print_log(log_line: &str) {
    TEST_LOG_OUTPUT_DATA.push(log_line.into());
}

#[cfg(test)]
use {mockall::lazy_static, std::sync::Mutex};

#[cfg(test)]
pub struct SynchronizedTestLogData(Mutex<Vec<String>>);

#[cfg(test)]
impl SynchronizedTestLogData {
    pub fn new() -> Self {
        SynchronizedTestLogData(Mutex::new(Vec::new()))
    }

    pub fn push(&self, log_entry: String) {
        let mut data = self.0.lock().unwrap();
        data.push(log_entry);
    }

    pub fn take(&self) -> Vec<String> {
        let mut data = self.0.lock().unwrap();
        std::mem::take(&mut *data)
    }
}

#[cfg(test)]
lazy_static! {
    pub static ref TEST_LOG_OUTPUT_DATA: SynchronizedTestLogData = SynchronizedTestLogData::new();
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
    use super::ServerConnection;
    use crate::{
        cli::LogsArgs,
        cli_commands::server_connection::{
            ServerConnectionError, TEST_LOG_OUTPUT_DATA, select_log_format_function,
        },
        cli_signals::MockSignalHandler,
        test_helper::MOCKALL_CONTEXT_SYNC,
    };

    use ankaios_api::ank_base::{
        CompleteStateRequestSpec, CompleteStateResponse, CompleteStateSpec, ConfigMappings,
        Dependencies, Error, ExecutionStateSpec, LogEntriesResponse, LogEntry,
        LogsCancelRequestSpec, LogsRequestAccepted, LogsRequestSpec, LogsStopResponse,
        RequestContentSpec, Response, ResponseContent, RestartPolicy, StateSpec, Tags,
        UpdateStateRequestSpec, UpdateStateSuccess, Workload, WorkloadInstanceNameSpec,
        WorkloadMapSpec, WorkloadSpec, WorkloadStateSpec,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_complete_state_response, generate_test_files,
        generate_test_proto_complete_state, generate_test_workload,
    };
    use common::{
        commands::UpdateWorkloadState, from_server_interface::FromServer,
        to_server_interface::ToServer,
    };

    use std::collections::{BTreeSet, HashMap};
    use tokio::sync::{
        mpsc::{self, Receiver},
        oneshot,
    };

    const OTHER_REQUEST: &str = "other_request";
    const FIELD_MASK: &str = "field_mask";

    #[derive(Default)]
    struct CommunicationSimulator {
        actions: Vec<CommunicationSimulatorAction>,
    }

    struct CorrectCommunicationChecker {
        join_handle: tokio::task::JoinHandle<()>,
        is_ready: oneshot::Receiver<Receiver<ToServer>>,
    }

    #[derive(Clone)]
    enum CommunicationSimulatorAction {
        WillSendMessage(FromServer),
        WillSendResponse(String, ResponseContent),
        ExpectReceiveRequest(String, RequestContentSpec),
    }

    impl CommunicationSimulator {
        fn create_server_connection(self) -> (CorrectCommunicationChecker, ServerConnection) {
            let (from_server, cli_receiver) = mpsc::channel::<FromServer>(1);
            let (to_server, mut server_receiver) = mpsc::channel::<ToServer>(1);

            let (is_ready_sender, is_ready) = oneshot::channel();

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
                                .send(FromServer::Response(Response {
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
                            let ToServer::Request(actual_request) = actual_message else {
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
                CorrectCommunicationChecker {
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

        pub fn will_send_response(&mut self, request_name: &str, response: ResponseContent) {
            self.actions
                .push(CommunicationSimulatorAction::WillSendResponse(
                    request_name.to_string(),
                    response,
                ));
        }

        pub fn expect_receive_request(&mut self, request_name: &str, request: RequestContentSpec) {
            self.actions
                .push(CommunicationSimulatorAction::ExpectReceiveRequest(
                    request_name.to_string(),
                    request,
                ));
        }
    }

    impl CorrectCommunicationChecker {
        fn check_communication(mut self) {
            let Ok(mut to_server) = self.is_ready.try_recv() else {
                panic!("Not all messages have been sent or received");
            };
            self.join_handle.abort();
            if let Ok(message) = to_server.try_recv() {
                panic!("Received unexpected message: {message:#?}");
            }
        }
    }

    impl Drop for CorrectCommunicationChecker {
        fn drop(&mut self) {
            self.join_handle.abort();
        }
    }

    fn complete_state(workload_name: &str) -> CompleteStateSpec {
        CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: [(
                        workload_name.into(),
                        WorkloadSpec {
                            agent: fixtures::AGENT_NAMES[0].into(),
                            runtime: fixtures::RUNTIME_NAMES[0].into(),
                            ..Default::default()
                        },
                    )]
                    .into(),
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn instance_name(workload_name: &str) -> WorkloadInstanceNameSpec {
        format!(
            "{workload_name}.{}.{}",
            fixtures::WORKLOAD_IDS[0],
            fixtures::AGENT_NAMES[0]
        )
        .try_into()
        .unwrap()
    }

    #[tokio::test]
    async fn utest_get_complete_state() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: vec![FIELD_MASK.into()],
                subscribe_for_events: false,
            }),
        );

        let proto_complete_state = generate_test_proto_complete_state(&[(
            fixtures::WORKLOAD_NAMES[0],
            generate_test_workload().into(),
        )]);

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::CompleteStateResponse(Box::new(CompleteStateResponse {
                complete_state: Some(proto_complete_state.clone()),
                ..Default::default()
            })),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let received_complete_state = server_connection
            .get_complete_state(&[FIELD_MASK.into()])
            .await;
        let expected_complete_state = proto_complete_state;

        assert!(received_complete_state.is_ok());
        assert_eq!(received_complete_state.unwrap(), expected_complete_state);
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_get_complete_state_fails_at_request() {
        let sim = CommunicationSimulator::default();
        let (_, mut server_connection) = sim.create_server_connection();
        // sending the GetCompleteState request to the server, shall already fail
        let (to_server, _) = mpsc::channel(1);
        server_connection.to_server = to_server;

        let result = server_connection
            .get_complete_state(&[FIELD_MASK.into()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_get_complete_state_fails_no_response() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: vec![FIELD_MASK.into()],
                subscribe_for_events: false,
            }),
        );
        let (_checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&[FIELD_MASK.into()])
            .await;
        assert!(result.is_err());
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_get_complete_state_other_response_in_between() {
        let proto_complete_state = generate_test_proto_complete_state(&[(
            fixtures::WORKLOAD_NAMES[0],
            generate_test_workload().into(),
        )]);

        let other_response = FromServer::Response(Response {
            request_id: OTHER_REQUEST.into(),
            response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                CompleteStateResponse {
                    complete_state: Some(generate_test_proto_complete_state(&[(
                        fixtures::WORKLOAD_NAMES[1],
                        generate_test_workload().into(),
                    )])),
                    ..Default::default()
                },
            ))),
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: vec![FIELD_MASK.into()],
                subscribe_for_events: false,
            }),
        );
        sim.will_send_message(other_response.clone());
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::CompleteStateResponse(Box::new(CompleteStateResponse {
                complete_state: Some(proto_complete_state.clone()),
                ..Default::default()
            })),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&[FIELD_MASK.into()])
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), proto_complete_state);
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
        let proto_complete_state = generate_test_proto_complete_state(&[(
            fixtures::WORKLOAD_NAMES[0],
            generate_test_workload().into(),
        )]);

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: vec![FIELD_MASK.into()],
                subscribe_for_events: false,
            }),
        );
        sim.will_send_message(other_message.clone());
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::CompleteStateResponse(Box::new(CompleteStateResponse {
                complete_state: Some(proto_complete_state.clone()),
                ..Default::default()
            })),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .get_complete_state(&[FIELD_MASK.into()])
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), proto_complete_state);
        assert_eq!(
            server_connection.take_missed_from_server_messages(),
            vec![other_message]
        );
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_update_state() {
        let update_state_success = UpdateStateSuccess {
            added_workloads: vec![fixtures::WORKLOAD_NAMES[0].into()],
            deleted_workloads: vec![],
        };

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::UpdateStateRequest(Box::new(UpdateStateRequestSpec {
                new_state: complete_state(fixtures::WORKLOAD_NAMES[0]),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::UpdateStateSuccess(update_state_success.clone()),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
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
        let (to_server, _) = mpsc::channel(1);
        server_connection.to_server = to_server;

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_update_state_fails_no_response() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::UpdateStateRequest(Box::new(UpdateStateRequestSpec {
                new_state: complete_state(fixtures::WORKLOAD_NAMES[0]),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );

        let (_, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_update_state_fails_error_response() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::UpdateStateRequest(Box::new(UpdateStateRequestSpec {
                new_state: complete_state(fixtures::WORKLOAD_NAMES[0]),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::Error(Error { message: "".into() }),
        );

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
            .await;

        assert!(result.is_err());
        checker.check_communication();
    }

    #[tokio::test]
    async fn utest_update_state_fails_response_timeout() {
        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::UpdateStateRequest(Box::new(UpdateStateRequestSpec {
                new_state: complete_state(fixtures::WORKLOAD_NAMES[0]),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );

        let (checker, mut server_connection) = sim.create_server_connection();
        let (_to_client, from_server) = mpsc::channel(1);
        server_connection.from_server = from_server;

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
            .await;

        assert!(result.is_err());
        checker.check_communication();
    }

    // [utest->swdd~cli-stores-unexpected-message~1]
    #[tokio::test]
    async fn utest_update_state_other_response_in_between() {
        let update_state_success = UpdateStateSuccess {
            added_workloads: vec![fixtures::WORKLOAD_NAMES[0].into()],
            deleted_workloads: vec![],
        };
        let other_response = FromServer::Response(Response {
            request_id: OTHER_REQUEST.into(),
            response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                CompleteStateResponse {
                    complete_state: Some(generate_test_proto_complete_state(&[(
                        fixtures::WORKLOAD_NAMES[1],
                        generate_test_workload().into(),
                    )])),
                    ..Default::default()
                },
            ))),
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::UpdateStateRequest(Box::new(UpdateStateRequestSpec {
                new_state: complete_state(fixtures::WORKLOAD_NAMES[0]),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_message(other_response.clone());
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::UpdateStateSuccess(update_state_success.clone()),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
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
            added_workloads: vec![fixtures::WORKLOAD_NAMES[0].into()],
            deleted_workloads: vec![],
        };
        let other_message = FromServer::UpdateWorkloadState(UpdateWorkloadState {
            workload_states: vec![],
        });

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::UpdateStateRequest(Box::new(UpdateStateRequestSpec {
                new_state: complete_state(fixtures::WORKLOAD_NAMES[0]),
                update_mask: vec![FIELD_MASK.into()],
            })),
        );
        sim.will_send_message(other_message.clone());
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::UpdateStateSuccess(update_state_success.clone()),
        );
        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .update_state(
                complete_state(fixtures::WORKLOAD_NAMES[0]),
                vec![FIELD_MASK.into()],
            )
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
            workload_states: vec![WorkloadStateSpec {
                instance_name: instance_name(fixtures::WORKLOAD_NAMES[0]),
                execution_state: ExecutionStateSpec::running(),
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
        let other_message = FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.into(),
            response_content: Some(ResponseContent::Error(Error { message: "".into() })),
        });
        let update_workload_state = UpdateWorkloadState {
            workload_states: vec![WorkloadStateSpec {
                instance_name: instance_name(fixtures::WORKLOAD_NAMES[0]),
                execution_state: ExecutionStateSpec::running(),
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

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    // [utest->swdd~cli-handles-log-responses-from-server~1]
    // [utest->swdd~cli-stops-log-output-for-specific-workloads~1]
    // [utest->swdd~cli-outputs-logs-in-specific-format~1]
    #[tokio::test]
    async fn utest_stream_logs_multiple_workloads_no_follow() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        let log_args = LogsArgs {
            workload_name: vec![
                fixtures::WORKLOAD_NAMES[0].to_string(),
                fixtures::WORKLOAD_NAMES[1].to_string(),
            ],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let instance_name_2 = instance_name(fixtures::WORKLOAD_NAMES[1]);

        let mut sim = CommunicationSimulator::default();
        let instance_names = vec![instance_name_1.clone(), instance_name_2.clone()];
        let instance_names_set: BTreeSet<WorkloadInstanceNameSpec> =
            instance_names.iter().cloned().collect();

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsRequest(LogsRequestSpec {
                workload_names: instance_names,
                follow: log_args.follow,
                tail: log_args.tail,
                since: log_args.since.clone(),
                until: log_args.until.clone(),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                workload_names: vec![
                    instance_name_1.clone().into(),
                    instance_name_2.clone().into(),
                ],
            }),
        );

        let log_entries = vec![
            LogEntry {
                workload_name: Some(instance_name_1.clone().into()),
                message: "some log line".to_string(),
            },
            LogEntry {
                workload_name: Some(instance_name_2.clone().into()),
                message: "another log line".to_string(),
            },
        ];

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogEntriesResponse(LogEntriesResponse {
                log_entries: log_entries.clone(),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsStopResponse(LogsStopResponse {
                workload_name: Some(instance_name_1.into()),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsStopResponse(LogsStopResponse {
                workload_name: Some(instance_name_2.into()),
            }),
        );

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert!(result.is_ok());

        checker.check_communication();

        let actual_log_data: BTreeSet<String> = TEST_LOG_OUTPUT_DATA.take().into_iter().collect();

        let expected_log_data: BTreeSet<String> = log_entries
            .into_iter()
            .map(|log_entry| {
                let workload_name = log_entry.workload_name.unwrap_or_default().workload_name;
                format!("{} {}\n", workload_name, log_entry.message)
            })
            .collect();

        assert_eq!(actual_log_data, expected_log_data);
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_stream_logs_send_logs_request_channel_closed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_names_set = BTreeSet::from([instance_name(fixtures::WORKLOAD_NAMES[0])]);

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context.expect().never();

        let (_from_server_sender, cli_receiver) = mpsc::channel::<FromServer>(1);
        let (to_server, mut server_receiver) = mpsc::channel::<ToServer>(1);

        server_receiver.close();

        let mut server_connection = ServerConnection {
            to_server,
            from_server: cli_receiver,
            task: tokio::spawn(async {}),
            missed_from_server_messages: Vec::new(),
        };

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert!(result.is_err());
    }

    // [utest->swdd~cli-outputs-logs-in-specific-format~1]
    #[test]
    fn utest_output_log_line_without_workload_name_upon_single_workload() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_names = BTreeSet::from([instance_name_1.clone()]);

        let output_log_fn = select_log_format_function(&instance_names, log_args.output_names);

        let log_message = "some log line";
        let log_entry = LogEntry {
            workload_name: Some(instance_name_1.clone().into()),
            message: log_message.to_string(),
        };

        output_log_fn(vec![log_entry]);

        let actual_log_data = TEST_LOG_OUTPUT_DATA.take();
        assert_eq!(actual_log_data, vec![format!("{log_message}\n")]);
    }

    // [utest->swdd~cli-outputs-logs-in-specific-format~1]
    #[test]
    fn utest_output_log_line_with_prefixed_workload_name_upon_provided_force_names_argument() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: true,
        };

        let instance_names = BTreeSet::from([instance_name_1.clone()]);

        let output_log_fn = select_log_format_function(&instance_names, log_args.output_names);

        let log_message = "some log line";
        let log_entry = LogEntry {
            workload_name: Some(instance_name_1.clone().into()),
            message: log_message.to_string(),
        };

        output_log_fn(vec![log_entry]);

        let actual_log_data = TEST_LOG_OUTPUT_DATA.take();
        assert_eq!(
            actual_log_data,
            vec![format!(
                "{} {}\n",
                instance_name_1.workload_name(),
                log_message
            )]
        );
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    // [utest->swdd~cli-handles-log-responses-from-server~1]
    #[tokio::test]
    async fn utest_stream_logs_response_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let mut sim = CommunicationSimulator::default();
        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let instance_names = vec![instance_name_1.clone()];
        let instance_names_set: BTreeSet<WorkloadInstanceNameSpec> =
            instance_names.iter().cloned().collect();

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsRequest(LogsRequestSpec {
                workload_names: instance_names,
                follow: log_args.follow,
                tail: log_args.tail,
                since: log_args.since.clone(),
                until: log_args.until.clone(),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                workload_names: vec![instance_name_1.into()],
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::Error(Error {
                message: "log collection error.".to_string(),
            }),
        );

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert_eq!(
            result,
            Err(ServerConnectionError::ExecutionError(
                "Error streaming logs: 'log collection error.'".to_string()
            ))
        );

        checker.check_communication();

        let actual_log_data = TEST_LOG_OUTPUT_DATA.take();
        assert!(actual_log_data.is_empty());
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    // [utest->swdd~cli-handles-log-responses-from-server~1]
    #[tokio::test]
    async fn utest_stream_logs_ignore_unrelated_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let mut sim = CommunicationSimulator::default();
        let instance_names = vec![instance_name_1.clone()];
        let instance_names_set: BTreeSet<WorkloadInstanceNameSpec> =
            instance_names.iter().cloned().collect();

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsRequest(LogsRequestSpec {
                workload_names: instance_names,
                follow: log_args.follow,
                tail: log_args.tail,
                since: log_args.since.clone(),
                until: log_args.until.clone(),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                workload_names: vec![instance_name_1.clone().into()],
            }),
        );

        // Send unrelated response that should be ignored in the log streaming
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                added_workloads: vec![fixtures::WORKLOAD_NAMES[1].into()],
                ..Default::default()
            }),
        );

        // just to stop the streaming
        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsStopResponse(LogsStopResponse {
                workload_name: Some(instance_name_1.into()),
            }),
        );

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert!(result.is_ok());

        checker.check_communication();

        let actual_log_data = TEST_LOG_OUTPUT_DATA.take();
        assert!(actual_log_data.is_empty());
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    // [utest->swdd~cli-sends-logs-cancel-request-upon-termination~1]
    #[tokio::test]
    async fn utest_stream_logs_send_logs_cancel_request_upon_termination_signal() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: true,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let mut sim = CommunicationSimulator::default();
        let instance_names = vec![instance_name_1.clone()];
        let instance_names_set: BTreeSet<WorkloadInstanceNameSpec> =
            instance_names.iter().cloned().collect();

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsRequest(LogsRequestSpec {
                workload_names: instance_names,
                follow: log_args.follow,
                tail: log_args.tail,
                since: log_args.since.clone(),
                until: log_args.until.clone(),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                workload_names: vec![instance_name_1.into()],
            }),
        );

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsCancelRequest(LogsCancelRequestSpec {}),
        );

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::ready(())));

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert!(result.is_ok());

        tokio::time::sleep(std::time::Duration::from_millis(100)).await; // wait until server connection receives all messages

        checker.check_communication();
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_stream_logs_ignore_unexpected_message_instead_of_logs_request_accepted() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        let log_args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let mut sim = CommunicationSimulator::default();
        let instance_names = vec![instance_name_1];
        let instance_names_set: BTreeSet<WorkloadInstanceNameSpec> =
            instance_names.iter().cloned().collect();

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsRequest(LogsRequestSpec {
                workload_names: instance_names,
                follow: log_args.follow,
                tail: log_args.tail,
                since: log_args.since.clone(),
                until: log_args.until.clone(),
            }),
        );

        let unexpected_message = ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
            added_workloads: vec![fixtures::WORKLOAD_NAMES[0].into()],
            deleted_workloads: vec![],
        });

        sim.will_send_response(fixtures::REQUEST_ID, unexpected_message);

        // error to stop the loop after the ignored message
        let error_response = ResponseContent::Error(Error {
            message: "connection interruption".to_string(),
        });

        sim.will_send_response(fixtures::REQUEST_ID, error_response);

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context.expect().never();

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert!(result.is_err());

        checker.check_communication();
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_get_logs_accepted_response_channel_closed() {
        let sim = CommunicationSimulator::default();

        let (_checker, mut server_connection) = sim.create_server_connection();

        server_connection.from_server.close();

        let result = server_connection
            .get_logs_accepted_response(fixtures::REQUEST_ID.to_owned())
            .await;

        assert_eq!(
            result,
            Err(ServerConnectionError::ExecutionError(
                "Connection to server interrupted while waiting for LogsRequestAccepted response."
                    .to_string()
            ))
        );
    }

    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_stream_logs_invalid_workload_names_in_logs_request_accepted() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        let log_args = LogsArgs {
            workload_name: vec![
                fixtures::WORKLOAD_NAMES[0].to_string(),
                fixtures::WORKLOAD_NAMES[1].to_string(),
            ],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        let instance_name_1 = instance_name(fixtures::WORKLOAD_NAMES[0]);
        let instance_name_2 = instance_name(fixtures::WORKLOAD_NAMES[1]);
        let mut sim = CommunicationSimulator::default();
        let instance_names = vec![instance_name_1.clone()];
        let instance_names_set: BTreeSet<WorkloadInstanceNameSpec> =
            instance_names.iter().cloned().collect();

        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::LogsRequest(LogsRequestSpec {
                workload_names: instance_names,
                follow: log_args.follow,
                tail: log_args.tail,
                since: log_args.since.clone(),
                until: log_args.until.clone(),
            }),
        );

        sim.will_send_response(
            fixtures::REQUEST_ID,
            ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                workload_names: vec![instance_name_2.into()],
            }),
        );

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context.expect().never();

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .stream_logs(instance_names_set, log_args)
            .await;

        assert_eq!(
            result,
            Err(ServerConnectionError::ExecutionError(format!(
                "Workload '{}' is not accepted by the server to receive logs from.",
                fixtures::WORKLOAD_NAMES[0]
            )))
        );

        checker.check_communication();
    }

    // [utest->swdd~cli-subscribes-for-events~1]
    #[tokio::test]
    async fn utest_subscribe_and_listen_for_events_success() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: field_mask.clone(),
                subscribe_for_events: true,
            }),
        );

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .subscribe_and_listen_for_events(field_mask, false)
            .await;

        assert!(result.is_ok());
        let subscription = result.unwrap();
        assert!(!subscription.initial_response_received);
        assert!(!subscription.request_id.is_empty());

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        checker.check_communication();
    }

    // [utest->swdd~cli-subscribes-for-events~1]
    #[tokio::test]
    async fn utest_subscribe_and_listen_for_events_empty_field_mask() {
        let field_mask = vec![];

        let mut sim = CommunicationSimulator::default();
        sim.expect_receive_request(
            fixtures::REQUEST_ID,
            RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: field_mask.clone(),
                subscribe_for_events: true,
            }),
        );

        let (checker, mut server_connection) = sim.create_server_connection();

        let result = server_connection
            .subscribe_and_listen_for_events(field_mask, false)
            .await;

        assert!(result.is_ok());

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        checker.check_communication();
    }

    // [utest->swdd~cli-handles-event-subscription-errors~1]
    #[tokio::test]
    async fn utest_subscribe_and_listen_for_events_fails_channel_closed() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let sim = CommunicationSimulator::default();
        let (_, mut server_connection) = sim.create_server_connection();

        let (to_server, _) = tokio::sync::mpsc::channel(1);
        server_connection.to_server = to_server;

        let result = server_connection
            .subscribe_and_listen_for_events(field_mask, false)
            .await;

        assert!(result.is_err());
    }

    // [utest->swdd~cli-receives-events~1]
    #[tokio::test]
    async fn utest_receive_next_event_initial_state_then_event() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut sim = CommunicationSimulator::default();

        sim.will_send_message(FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.to_string(),
            response_content: Some(generate_test_complete_state_response(&[(
                fixtures::WORKLOAD_NAMES[0],
                Workload {
                    agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                    runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                    tags: Some(Tags::default()),
                    dependencies: Some(Dependencies::default()),
                    restart_policy: Some(RestartPolicy::Never as i32),
                    runtime_config: Some(String::default()),
                    control_interface_access: None,
                    configs: Some(ConfigMappings {
                        configs: [
                            ("ref1".into(), "config_1".into()),
                            ("ref2".into(), "config_2".into()),
                        ]
                        .into(),
                    }),
                    files: Some(generate_test_files().into()),
                },
            )])),
        }));

        sim.will_send_message(FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.to_string(),
            response_content: Some(generate_test_complete_state_response(&[(
                fixtures::WORKLOAD_NAMES[1],
                Workload {
                    agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                    runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                    tags: Some(Tags::default()),
                    dependencies: Some(Dependencies::default()),
                    restart_policy: Some(RestartPolicy::Never as i32),
                    runtime_config: Some(String::default()),
                    control_interface_access: None,
                    configs: Some(ConfigMappings {
                        configs: [
                            ("ref1".into(), "config_1".into()),
                            ("ref2".into(), "config_2".into()),
                        ]
                        .into(),
                    }),
                    files: Some(generate_test_files().into()),
                },
            )])),
        }));

        sim.will_send_message(FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.to_string(),
            response_content: Some(generate_test_complete_state_response(&[(
                fixtures::WORKLOAD_NAMES[2],
                Workload {
                    agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                    runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                    tags: Some(Tags::default()),
                    dependencies: Some(Dependencies::default()),
                    restart_policy: Some(RestartPolicy::Never as i32),
                    runtime_config: Some(String::default()),
                    control_interface_access: None,
                    configs: Some(ConfigMappings {
                        configs: [
                            ("ref1".into(), "config_1".into()),
                            ("ref2".into(), "config_2".into()),
                        ]
                        .into(),
                    }),
                    files: Some(generate_test_files().into()),
                },
            )])),
        }));

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let mut subscription = super::EventSubscription {
            request_id: fixtures::REQUEST_ID.to_string(),
            initial_response_received: false,
            output_initial_response: false,
        };

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;
        assert!(result.is_ok());
        assert!(subscription.initial_response_received);
        assert!(result.unwrap().is_some());

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;
        assert!(result.is_ok());
        let event = result.unwrap();
        assert!(event.is_some());

        checker.check_communication();
    }

    // [utest->swdd~cli-receives-events~1]
    // [utest->swdd~cli-handles-event-subscription-errors~1]
    #[tokio::test]
    async fn utest_receive_next_event_signal_interruption() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sim = CommunicationSimulator::default();

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::ready(())));

        let (_, mut server_connection) = sim.create_server_connection();

        let mut subscription = super::EventSubscription {
            request_id: fixtures::REQUEST_ID.to_string(),
            initial_response_received: true,
            output_initial_response: false,
        };

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // [utest->swdd~cli-receives-events~1]
    // [utest->swdd~cli-handles-event-subscription-errors~1]
    #[tokio::test]
    async fn utest_receive_next_event_error_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut sim = CommunicationSimulator::default();
        sim.will_send_message(FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.to_string(),
            response_content: Some(ResponseContent::Error(Error {
                message: "Event subscription error".to_string(),
            })),
        }));

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let mut subscription = super::EventSubscription {
            request_id: fixtures::REQUEST_ID.to_string(),
            initial_response_received: true,
            output_initial_response: false,
        };

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;

        assert!(result.is_err());
        match result {
            Err(ServerConnectionError::ExecutionError(msg)) => {
                assert!(msg.contains("Event subscription failed"));
            }
            _ => panic!("Expected ExecutionError"),
        }

        checker.check_communication();
    }

    // [utest->swdd~cli-receives-events~1]
    // [utest->swdd~cli-handles-event-subscription-errors~1]
    #[tokio::test]
    async fn utest_receive_next_event_connection_interrupted() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sim = CommunicationSimulator::default();

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (_, mut server_connection) = sim.create_server_connection();

        server_connection.from_server.close();

        let mut subscription = super::EventSubscription {
            request_id: fixtures::REQUEST_ID.to_string(),
            initial_response_received: true,
            output_initial_response: false,
        };

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;

        assert!(result.is_err());
        match result {
            Err(ServerConnectionError::ExecutionError(msg)) => {
                assert!(msg.contains("Connection to server interrupted"));
            }
            _ => panic!("Expected ExecutionError"),
        }
    }

    // [utest->swdd~cli-receives-events~1]
    #[tokio::test]
    async fn utest_receive_next_event_ignores_unexpected_messages() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut sim = CommunicationSimulator::default();

        sim.will_send_message(FromServer::Response(Response {
            request_id: OTHER_REQUEST.into(),
            response_content: Some(generate_test_complete_state_response(&[(
                fixtures::WORKLOAD_NAMES[0],
                Workload {
                    agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                    runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                    tags: Some(Tags::default()),
                    dependencies: Some(Dependencies::default()),
                    restart_policy: Some(RestartPolicy::Never as i32),
                    runtime_config: Some(String::default()),
                    control_interface_access: None,
                    configs: Some(ConfigMappings {
                        configs: [
                            ("ref1".into(), "config_1".into()),
                            ("ref2".into(), "config_2".into()),
                        ]
                        .into(),
                    }),
                    files: Some(generate_test_files().into()),
                },
            )])),
        }));

        sim.will_send_message(FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.to_string(),
            response_content: Some(generate_test_complete_state_response(&[(
                fixtures::WORKLOAD_NAMES[1],
                Workload {
                    agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                    runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                    tags: Some(Tags::default()),
                    dependencies: Some(Dependencies::default()),
                    restart_policy: Some(RestartPolicy::Never as i32),
                    runtime_config: Some(String::default()),
                    control_interface_access: None,
                    configs: Some(ConfigMappings {
                        configs: [
                            ("ref1".into(), "config_1".into()),
                            ("ref2".into(), "config_2".into()),
                        ]
                        .into(),
                    }),
                    files: Some(generate_test_files().into()),
                },
            )])),
        }));

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let mut subscription = super::EventSubscription {
            request_id: fixtures::REQUEST_ID.to_string(),
            initial_response_received: true,
            output_initial_response: false,
        };

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;

        assert!(result.is_ok());
        let event = result.unwrap();
        assert!(event.is_some());

        checker.check_communication();
    }

    // [utest->swdd~cli-receives-events~1]
    #[tokio::test]
    async fn utest_receive_next_event_multiple_events_in_sequence() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut sim = CommunicationSimulator::default();

        sim.will_send_message(FromServer::Response(Response {
            request_id: fixtures::REQUEST_ID.to_string(),
            response_content: Some(generate_test_complete_state_response(&[(
                fixtures::WORKLOAD_NAMES[0],
                Workload {
                    agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                    runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                    tags: Some(Tags::default()),
                    dependencies: Some(Dependencies::default()),
                    restart_policy: Some(RestartPolicy::Never as i32),
                    runtime_config: Some(String::default()),
                    control_interface_access: None,
                    configs: Some(ConfigMappings {
                        configs: HashMap::new(),
                    }),
                    files: Some(generate_test_files().into()),
                },
            )])),
        }));

        for i in 1..=3 {
            sim.will_send_message(FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(generate_test_complete_state_response(&[(
                    &format!("workload_{i}"),
                    Workload {
                        agent: Some(fixtures::AGENT_NAMES[0].to_string()),
                        runtime: Some(fixtures::RUNTIME_NAMES[0].to_string()),
                        tags: Some(Tags::default()),
                        dependencies: Some(Dependencies::default()),
                        restart_policy: Some(RestartPolicy::Never as i32),
                        runtime_config: Some(String::default()),
                        control_interface_access: None,
                        configs: Some(ConfigMappings {
                            configs: HashMap::new(),
                        }),
                        files: Some(generate_test_files().into()),
                    },
                )])),
            }));
        }

        let signal_handler_context = MockSignalHandler::wait_for_signals_context();
        signal_handler_context
            .expect()
            .returning(|| Box::pin(std::future::pending()));

        let (checker, mut server_connection) = sim.create_server_connection();

        let mut subscription = super::EventSubscription {
            request_id: fixtures::REQUEST_ID.to_string(),
            initial_response_received: false,
            output_initial_response: false,
        };

        let result = server_connection
            .receive_next_event(&mut subscription)
            .await;
        assert!(result.is_ok());
        assert!(subscription.initial_response_received);
        assert!(result.unwrap().is_some());

        for _ in 1..=2 {
            let result = server_connection
                .receive_next_event(&mut subscription)
                .await;
            assert!(result.is_ok());
            assert!(result.unwrap().is_some());
        }

        checker.check_communication();
    }
}
