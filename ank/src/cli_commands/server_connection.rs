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

use std::{mem::take, time::Duration};

use crate::output_debug;
use common::{
    commands::{
        CompleteStateRequest, Response, ResponseContent, UpdateStateSuccess, UpdateWorkloadState,
    },
    from_server_interface::{FromServer, FromServerReceiver},
    objects::CompleteState,
    to_server_interface::{ToServerInterface, ToServerSender},
};

const WAIT_TIME_MS: Duration = Duration::from_millis(3000);

pub enum ServerConnectionError {
    ExecutionError(String),
}

pub struct ServerConnection {
    to_server: ToServerSender,
    from_server: FromServerReceiver,
    task: tokio::task::JoinHandle<()>,
    missed_from_server_messages: Vec<FromServer>,
}

impl ServerConnection {
    pub fn new(
        to_server: ToServerSender,
        from_server: FromServerReceiver,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            to_server,
            from_server,
            task,
            missed_from_server_messages: Vec::new(),
        }
    }

    pub async fn shut_down(self) {
        drop(self.to_server);

        let _ = self.task.await;
    }

    pub async fn get_complete_state(
        &mut self,
        object_field_mask: &Vec<String>,
    ) -> Result<Box<CompleteState>, ServerConnectionError> {
        output_debug!(
            "get_complete_state: object_field_mask={:?} ",
            object_field_mask
        );

        let request_id = uuid::Uuid::new_v4().to_string();

        // send complete state request to server
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
                    Some(FromServer::Response(Response {
                        request_id: received_request_id,
                        response_content: ResponseContent::CompleteState(res),
                    })) if received_request_id == request_id => return Ok(res),
                    None => return Err("Channel preliminary closed."),
                    Some(message) => {
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
    ) -> Result<UpdateStateSuccess, ServerConnectionError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        output_debug!("Sending the new state {:?}", new_state);
        self.to_server
            .update_state(request_id.clone(), new_state, update_mask)
            .await
            .map_err(|err| ServerConnectionError::ExecutionError(err.to_string()))?;

        let update_state_success = loop {
            let Some(server_message) = self.from_server.recv().await else {
                return Err(ServerConnectionError::ExecutionError(
                    "Connection to server interrupted".into(),
                ));
            };
            match server_message {
                FromServer::Response(response) => {
                    if response.request_id != request_id {
                        output_debug!(
                            "Received unexpected response for request ID: '{}'",
                            response.request_id
                        );
                    } else {
                        match response.response_content {
                            ResponseContent::UpdateStateSuccess(update_state_success) => {
                                break update_state_success
                            }
                            // [impl->swdd~cli-requests-update-state-with-watch-error~1]
                            ResponseContent::Error(error) => {
                                return Err(ServerConnectionError::ExecutionError(format!(
                                    "SetState failed with: '{}'",
                                    error.message
                                )));
                            }
                            // [impl->swdd~cli-requests-update-state-with-watch-error~1]
                            response_content => {
                                return Err(ServerConnectionError::ExecutionError(format!(
                                    "Received unexpected response: {:?}",
                                    response_content
                                )));
                            }
                        }
                    }
                }
                other_message => {
                    self.missed_from_server_messages.push(other_message);
                }
            }
        };

        output_debug!("Got update success: {:?}", update_state_success);
        Ok(update_state_success)
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
                self.missed_from_server_messages.push(server_message);
            };
        }
    }

    pub fn take_missed_from_server_messages(&mut self) -> Vec<FromServer> {
        take(&mut self.missed_from_server_messages)
    }
}
