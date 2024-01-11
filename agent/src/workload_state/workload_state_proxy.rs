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
    objects::WorkloadState, state_change_interface::{StateChangeSender, StateChangeInterface},
    std_extensions::IllegalStateResult,
};

#[cfg(test)]
use mockall::automock;
use serde_yaml::with::singleton_map_recursive;

pub type WorkloadStateMsgReceiver = tokio::sync::mpsc::Receiver<WorkloadStateMessage>;
pub type WorkloadStateMsgSender = tokio::sync::mpsc::Sender<WorkloadStateMessage>;

#[async_trait]
pub trait WorkloadStateSenderInterface {
    async fn store_remote_workload_states(&self, states: Vec<WorkloadState>);
    async fn report_local_workload_state(&self, state: WorkloadState);
}

#[async_trait]
impl WorkloadStateSenderInterface for WorkloadStateMsgSender {
    async fn store_remote_workload_states(&self, states: Vec<WorkloadState>) {
        self.send(WorkloadStateMessage::FromServer(states))
            .await
            .unwrap_or_illegal_state();
    }
    async fn report_local_workload_state(&self, state: WorkloadState) {
        self.send(WorkloadStateMessage::FromChecker(state))
            .await
            .unwrap_or_illegal_state();
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
