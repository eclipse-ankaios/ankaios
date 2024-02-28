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

#[async_trait]
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
    // use common::{
    //     objects::{ExecutionState, WorkloadInstanceName, WorkloadState},
    //     state_change_interface::StateChangeCommand,
    //     test_utils::generate_test_workload_spec,
    // };

    // use crate::workload_state::{
    //     WorkloadStateMessage, WorkloadStateProxy, WorkloadStateSenderInterface,
    // };

    // const BUFFER_SIZE: usize = 20;

    // #[tokio::test]
    // async fn utest_workload_state_proxy_start_from_checker_stores_and_forwards() {
    //     let (workload_state_sender, proxy_receiver) =
    //         tokio::sync::mpsc::channel::<WorkloadStateMessage>(BUFFER_SIZE);
    //     let (to_server, mut server_receiver) =
    //         tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    //     let mut test_workload_state_proxy =
    //         WorkloadStateProxy::new(to_server.clone(), proxy_receiver);

    //     let workload_spec = generate_test_workload_spec();
    //     let instance_name = workload_spec.instance_name();

    //     let expected_states = vec![WorkloadState {
    //         workload_name: instance_name.workload_name().to_string(),
    //         agent_name: instance_name.agent_name().to_string(),
    //         execution_state: ExecutionState::ExecStarting,
    //     }];

    //     workload_state_sender.report_starting(&instance_name).await;

    //     drop(workload_state_sender);

    //     test_workload_state_proxy.start().await;

    //     let result = server_receiver.recv().await.unwrap();

    //     assert!(matches!(
    //         result,
    //         StateChangeCommand::UpdateWorkloadState(common::commands::UpdateWorkloadState{workload_states})
    //         if workload_states == expected_states));

    //     assert_eq!(
    //         test_workload_state_proxy
    //             .states_db
    //             .get_state_of_workload(instance_name.workload_name()),
    //         Some(&ExecutionState::ExecStarting)
    //     );
    // }

    // #[tokio::test]
    // async fn utest_workload_state_proxy_start_from_checker_transforms_stores_and_forwards_remains()
    // {
    //     let (workload_state_sender, proxy_receiver) =
    //         tokio::sync::mpsc::channel::<WorkloadStateMessage>(BUFFER_SIZE);
    //     let (to_server, mut server_receiver) =
    //         tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    //     let mut test_workload_state_proxy =
    //         WorkloadStateProxy::new(to_server.clone(), proxy_receiver);

    //     let workload_spec = generate_test_workload_spec();
    //     let instance_name = workload_spec.instance_name();

    //     let initial_state = WorkloadState {
    //         workload_name: instance_name.workload_name().to_string(),
    //         agent_name: instance_name.agent_name().to_string(),
    //         execution_state: ExecutionState::ExecStopping,
    //     };

    //     let mut expected_state = initial_state.clone();
    //     expected_state.execution_state = ExecutionState::ExecRunning;

    //     let workload_states = vec![initial_state.clone()];

    //     // Fill the state store with a value
    //     test_workload_state_proxy
    //         .states_db
    //         .update_workload_state(workload_states[0].clone());

    //     workload_state_sender
    //         .report_workload_execution_state(
    //             instance_name.workload_name().to_string(),
    //             instance_name.agent_name().to_string(),
    //             ExecutionState::ExecRunning,
    //         )
    //         .await
    //         .unwrap();

    //     drop(workload_state_sender);

    //     test_workload_state_proxy.start().await;

    //     let result = server_receiver.recv().await.unwrap();

    //     assert!(matches!(
    //         result,
    //         StateChangeCommand::UpdateWorkloadState(common::commands::UpdateWorkloadState{workload_states})
    //         if workload_states == vec![initial_state.clone()]));

    //     assert_eq!(
    //         test_workload_state_proxy
    //             .states_db
    //             .get_state_of_workload(instance_name.workload_name()),
    //         Some(&initial_state.execution_state)
    //     );
    // }

    // #[tokio::test]
    // async fn utest_workload_state_proxy_start_from_checker_transforms_stores_and_forwards_changed()
    // {
    //     let (workload_state_sender, proxy_receiver) =
    //         tokio::sync::mpsc::channel::<WorkloadStateMessage>(BUFFER_SIZE);
    //     let (to_server, mut server_receiver) =
    //         tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    //     let mut test_workload_state_proxy =
    //         WorkloadStateProxy::new(to_server.clone(), proxy_receiver);

    //     let workload_spec = generate_test_workload_spec();
    //     let instance_name = workload_spec.instance_name();

    //     let initial_state = WorkloadState {
    //         workload_name: instance_name.workload_name().to_string(),
    //         agent_name: instance_name.agent_name().to_string(),
    //         execution_state: ExecutionState::ExecStarting,
    //     };

    //     let mut expected_state = initial_state.clone();
    //     expected_state.execution_state = ExecutionState::ExecRunning;

    //     let workload_states = vec![initial_state.clone()];

    //     // Fill the state store with a value
    //     test_workload_state_proxy
    //         .states_db
    //         .update_workload_state(workload_states[0].clone());

    //     workload_state_sender
    //         .report_workload_execution_state(
    //             instance_name.workload_name().to_string(),
    //             instance_name.agent_name().to_string(),
    //             ExecutionState::ExecRunning,
    //         )
    //         .await
    //         .unwrap();

    //     drop(workload_state_sender);

    //     test_workload_state_proxy.start().await;

    //     let result = server_receiver.recv().await.unwrap();

    //     assert!(matches!(
    //         result,
    //         StateChangeCommand::UpdateWorkloadState(common::commands::UpdateWorkloadState{workload_states})
    //         if workload_states == vec![expected_state.clone()]));

    //     assert_eq!(
    //         test_workload_state_proxy
    //             .states_db
    //             .get_state_of_workload(instance_name.workload_name()),
    //         Some(&expected_state.execution_state)
    //     );
    // }

    // #[tokio::test]
    // async fn utest_workload_state_proxy_start_stores_from_server() {
    //     let (workload_state_sender, proxy_receiver) =
    //         tokio::sync::mpsc::channel::<WorkloadStateMessage>(BUFFER_SIZE);
    //     let (to_server, mut server_receiver) =
    //         tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    //     let mut test_workload_state_proxy =
    //         WorkloadStateProxy::new(to_server.clone(), proxy_receiver);

    //     let workload_spec = generate_test_workload_spec();
    //     let instance_name = workload_spec.instance_name();

    //     let remote_states = vec![WorkloadState {
    //         workload_name: instance_name.workload_name().to_string(),
    //         agent_name: instance_name.agent_name().to_string(),
    //         execution_state: ExecutionState::ExecStarting,
    //     }];

    //     workload_state_sender
    //         .store_remote_workload_states(remote_states)
    //         .await
    //         .unwrap();

    //     drop(workload_state_sender);

    //     test_workload_state_proxy.start().await;

    //     assert!(server_receiver.try_recv().is_err());

    //     assert_eq!(
    //         test_workload_state_proxy
    //             .states_db
    //             .get_state_of_workload(instance_name.workload_name()),
    //         Some(&ExecutionState::ExecStarting)
    //     );
    // }
}
