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

use async_trait::async_trait;
use common::{
    objects::{ExecutionState, WorkloadInstanceName, WorkloadState},
    std_extensions::IllegalStateResult,
};

pub type WorkloadStateReceiver = tokio::sync::mpsc::Receiver<WorkloadState>;
pub type WorkloadStateSender = tokio::sync::mpsc::Sender<WorkloadState>;

#[async_trait]
pub trait WorkloadStateSenderInterface {
    async fn report_workload_execution_state(
        &self,
        instance_name: &WorkloadInstanceName,
        execution_state: ExecutionState,
    );
}

impl WorkloadStateSenderInterface for WorkloadStateSender {
    async fn report_workload_execution_state(
        &self,
        instance_name: &WorkloadInstanceName,
        execution_state: ExecutionState,
    ) {
        self.send(WorkloadState {
            instance_name: instance_name.to_owned(),
            execution_state,
        })
        .await
        .unwrap_or_illegal_state()
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
    mut state_change_rx: WorkloadStateReceiver,
    expected_states: Vec<ExecutionState>,
) {
    for expected_execution_state in expected_states {
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_millis(200),
                state_change_rx.recv()
            )
            .await
            .unwrap()
            .unwrap()
            .execution_state,
            expected_execution_state
        );
    }
}

#[cfg(test)]
mod tests {
    use common::objects::{ExecutionState, WorkloadInstanceName, WorkloadState};

    use crate::workload_state::WorkloadStateSenderInterface;

    const BUFFER_SIZE: usize = 20;

    #[tokio::test]
    async fn utest_workload_state_sender_interface_report() {
        let (wl_state_tx, mut wl_state_rx) =
            tokio::sync::mpsc::channel::<WorkloadState>(BUFFER_SIZE);

        let instance_name = WorkloadInstanceName::builder()
            .workload_name("name1")
            .agent_name("agent_X")
            .config(&"config string".to_string())
            .build();
        let exec_state = ExecutionState::running();

        wl_state_tx
            .report_workload_execution_state(&instance_name, exec_state)
            .await;

        let expected_execution_state = WorkloadState {
            instance_name,
            execution_state: ExecutionState::running(),
        };

        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_millis(200), wl_state_rx.recv())
                .await
                .unwrap()
                .unwrap(),
            expected_execution_state
        );
    }

}
