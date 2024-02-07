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

use super::workload_state_store::WorkloadStateStore;

use async_trait::async_trait;
use common::{
    objects::{ExecutionState, WorkloadExecutionInstanceName, WorkloadState},
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

    async fn report_starting(&self, instance_name: &WorkloadExecutionInstanceName);
    async fn report_stopping(&self, instance_name: &WorkloadExecutionInstanceName);
    async fn report_stopping_failed(&self, instance_name: &WorkloadExecutionInstanceName);
    async fn report_removed(&self, instance_name: &WorkloadExecutionInstanceName);
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

    async fn report_starting(&self, instance_name: &WorkloadExecutionInstanceName) {
        self.report_workload_execution_state(
            instance_name.workload_name().to_string(),
            instance_name.agent_name().to_string(),
            ExecutionState::ExecStarting,
        )
        .await
        .unwrap_or_illegal_state();
    }

    async fn report_stopping(&self, instance_name: &WorkloadExecutionInstanceName) {
        self.report_workload_execution_state(
            instance_name.workload_name().to_string(),
            instance_name.agent_name().to_string(),
            ExecutionState::ExecStopping,
        )
        .await
        .unwrap_or_illegal_state();
    }

    async fn report_stopping_failed(&self, instance_name: &WorkloadExecutionInstanceName) {
        self.report_workload_execution_state(
            instance_name.workload_name().to_string(),
            instance_name.agent_name().to_string(),
            ExecutionState::ExecStoppingFailed,
        )
        .await
        .unwrap_or_illegal_state();
    }

    async fn report_removed(&self, instance_name: &WorkloadExecutionInstanceName) {
        self.report_workload_execution_state(
            instance_name.workload_name().to_string(),
            instance_name.agent_name().to_string(),
            ExecutionState::ExecRemoved,
        )
        .await
        .unwrap_or_illegal_state();
    }
}
