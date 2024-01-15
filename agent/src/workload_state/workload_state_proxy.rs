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

use super::workload_state_db::WorkloadStateDB;

use async_trait::async_trait;
use common::{
    objects::{ExecutionState, WorkloadState},
    state_change_interface::{StateChangeInterface, StateChangeSender},
    std_extensions::IllegalStateResult,
};

#[cfg(test)]
use mockall::automock;

pub type WorkloadStateMsgReceiver = tokio::sync::mpsc::Receiver<WorkloadStateMessage>;
pub type WorkloadStateMsgSender = tokio::sync::mpsc::Sender<WorkloadStateMessage>;

#[async_trait]
pub trait WorkloadStateSenderInterface {
    async fn store_remote_workload_states(&self, states: Vec<WorkloadState>) -> Result<(), String>;
    async fn report_workload_execution_state(
        &self,
        workload_name: String,
        agent_name: String,
        execution_state: ExecutionState,
    ) -> Result<(), String>;
}

#[async_trait]
impl WorkloadStateSenderInterface for WorkloadStateMsgSender {
    async fn store_remote_workload_states(&self, states: Vec<WorkloadState>) -> Result<(), String> {
        self.send(WorkloadStateMessage::FromServer(states))
            .await
            .map_err(|error| error.to_string())
    }
    async fn report_workload_execution_state(
        &self,
        workload_name: String,
        agent_name: String,
        execution_state: ExecutionState,
    ) -> Result<(), String> {
        self.send(WorkloadStateMessage::FromChecker(WorkloadState {
            workload_name,
            agent_name,
            execution_state,
        }))
        .await
        .map_err(|error| error.to_string())
    }
}

// TODO probably this shall be only internal and the channel should be created via a function
pub enum WorkloadStateMessage {
    FromChecker(WorkloadState),
    FromServer(Vec<WorkloadState>),
}

pub struct WorkloadStateProxy {
    to_server: StateChangeSender,
    receiver: WorkloadStateMsgReceiver,
    states_db: WorkloadStateDB,
}

#[cfg_attr(test, automock)]
impl WorkloadStateProxy {
    pub fn new(to_server: StateChangeSender, receiver: WorkloadStateMsgReceiver) -> Self {
        WorkloadStateProxy {
            to_server,
            receiver,
            states_db: WorkloadStateDB::new(),
        }
    }

    pub async fn start(&mut self) {
        while let Some(command) = self.receiver.recv().await {
            match command {
                WorkloadStateMessage::FromChecker(mut single_workload_state) => {
                    if let Some(current_state) = self
                        .states_db
                        .get_state_of_workload(&single_workload_state.workload_name)
                    {
                        single_workload_state.execution_state = current_state
                            .clone()
                            .transition(single_workload_state.execution_state);
                    }

                    self.to_server
                        .update_workload_state(vec![single_workload_state])
                        .await
                        .unwrap_or_illegal_state();
                }
                WorkloadStateMessage::FromServer(workload_states) => {
                    for state in workload_states {
                        self.states_db.update_workload_state(state);
                    }
                }
            }
        }
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
pub async fn assert_execution_state_sequence(
    mut state_change_rx: WorkloadStateMsgReceiver,
    expected_states: Vec<ExecutionState>,
) {
    for expected_execution_state in expected_states {
        assert!(matches!(
                tokio::time::timeout(std::time::Duration::from_millis(200), state_change_rx.recv()).await,
                Ok(Some(WorkloadStateMessage::FromChecker(workload_state)))
            if workload_state.execution_state == expected_execution_state));
    }
}

#[cfg(test)]
mod tests {

    // TODO write tests
}
