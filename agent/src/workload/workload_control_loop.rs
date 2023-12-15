use std::path::PathBuf;

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
use crate::runtime_connectors::{RuntimeConnector, StateChecker};
use crate::workload::WorkloadCommand;
use crate::workload::WorkloadCommandChannel;
use common::{
    objects::{ExecutionState, WorkloadSpec},
    state_change_interface::{StateChangeInterface, StateChangeSender},
    std_extensions::IllegalStateResult,
};

#[cfg(test)]
use mockall::automock;

use super::workload_command_channel::WorkloadCommandReceiver;

#[cfg(not(test))]
const MAX_RESTARTS: usize = 20;

#[cfg(test)]
const MAX_RESTARTS: usize = 2;

#[cfg(not(test))]
const RETRY_WAITING_TIME_MS: u64 = 1000;

#[cfg(test)]
const RETRY_WAITING_TIME_MS: u64 = 200;

pub struct RestartState {
    quit_restart: bool,
    restart_counter: usize,
}

impl RestartState {
    pub fn new() -> Self {
        RestartState {
            quit_restart: false,
            restart_counter: 1,
        }
    }

    pub fn disable_restarts(&mut self) {
        self.quit_restart = true;
    }

    pub fn reset(&mut self) {
        self.quit_restart = false;
        self.restart_counter = 1;
    }

    pub fn restart_allowed(&self) -> bool {
        !self.quit_restart && self.restart_counter <= MAX_RESTARTS
    }

    pub fn count_restart(&mut self) {
        if self.restart_counter <= MAX_RESTARTS {
            self.restart_counter += 1;
        }
    }

    pub fn current_restart(&self) -> usize {
        self.restart_counter
    }
}

pub struct ControlLoopState<WorkloadId, StChecker>
where
    WorkloadId: Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub workload_name: String,
    pub agent_name: String,
    pub workload_id: Option<WorkloadId>,
    pub state_checker: Option<StChecker>,
    pub update_state_tx: StateChangeSender,
    pub runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>,
    pub command_receiver: WorkloadCommandReceiver,
    pub workload_channel: WorkloadCommandChannel,
    pub restart_state: RestartState,
}

pub struct WorkloadControlLoop;

#[cfg_attr(test, automock)]
impl WorkloadControlLoop {
    async fn do_delete<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) -> Option<ControlLoopState<WorkloadId, StChecker>>
    where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                // [impl->swdd~agent-workload-task-delete-failed-allows-retry~1]
                log::warn!(
                    "Could not stop workload '{}': '{}'",
                    control_loop_state.workload_name,
                    err
                );
                control_loop_state.workload_id = Some(old_id);

                return Some(control_loop_state);
            } else {
                if let Some(old_checker) = control_loop_state.state_checker.take() {
                    old_checker.stop_checker().await;
                }
                log::debug!("Stop workload complete");
            }
        } else {
            // [impl->swdd~agent-workload-task-delete-broken-allowed~1]
            log::debug!(
                "Workload '{}' already gone.",
                control_loop_state.workload_name
            );
        }

        // Successfully stopped the workload and the state checker. Send a removed on the channel
        control_loop_state
            .update_state_tx
            .update_workload_state(vec![common::objects::WorkloadState {
                agent_name: control_loop_state.agent_name.clone(),
                workload_name: control_loop_state.workload_name.clone(),
                execution_state: ExecutionState::ExecRemoved,
            }])
            .await
            .unwrap_or_illegal_state();

        None
    }

    async fn do_update<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                // [impl->swdd~agent-workload-task-update-delete-failed-allows-retry~1]
                log::warn!(
                    "Could not update workload '{}': '{}'",
                    control_loop_state.workload_name,
                    err
                );
                control_loop_state.workload_id = Some(old_id);
                return control_loop_state;
            } else if let Some(old_checker) = control_loop_state.state_checker.take() {
                old_checker.stop_checker().await;
            }
        } else {
            // [impl->swdd~agent-workload-task-update-broken-allowed~1]
            log::debug!(
                "Workload '{}' already gone.",
                control_loop_state.workload_name
            );
        }

        match control_loop_state
            .runtime
            .create_workload(
                runtime_workload_config,
                control_interface_path,
                control_loop_state.update_state_tx.clone(),
            )
            .await
        {
            Ok((new_workload_id, new_state_checker)) => {
                control_loop_state.workload_id = Some(new_workload_id);
                control_loop_state.state_checker = Some(new_state_checker);
            }
            Err(err) => {
                // [impl->swdd~agent-workload-task-update-create-failed-allows-retry~1]
                log::warn!(
                    "Could not start updated workload '{}': '{}'",
                    control_loop_state.workload_name,
                    err
                )
            }
        }

        control_loop_state
    }

    async fn do_restart<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let restart_state: &mut RestartState = &mut control_loop_state.restart_state;
        if !restart_state.restart_allowed() {
            log::debug!("Skip restart workload");
            return control_loop_state;
        }

        match control_loop_state
            .runtime
            .create_workload(
                runtime_workload_config.clone(),
                control_interface_path.clone(),
                control_loop_state.update_state_tx.clone(),
            )
            .await
        {
            Ok((new_workload_id, new_state_checker)) => {
                control_loop_state.workload_id = Some(new_workload_id);
                control_loop_state.state_checker = Some(new_state_checker);
            }
            Err(err) => {
                log::warn!(
                    "Restart '{}' out of '{}': Failed to create workload: '{}': '{}'",
                    restart_state.current_restart(),
                    MAX_RESTARTS,
                    control_loop_state.workload_name,
                    err
                );

                if !restart_state.restart_allowed() {
                    log::warn!(
                        "Abort restarts: maximum amount of restarts ('{}') reached.",
                        MAX_RESTARTS
                    );
                    restart_state.disable_restarts();
                    return control_loop_state;
                }

                restart_state.count_restart();
                let sender = control_loop_state.workload_channel.clone();
                tokio::task::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_WAITING_TIME_MS))
                        .await;
                    log::debug!("Send WorkloadCommand::Restart.");

                    sender
                        .restart(runtime_workload_config, control_interface_path)
                        .await
                        .unwrap_or_else(|err| {
                            log::warn!("Could not send WorkloadCommand::Restart: '{}'", err)
                        });
                });
            }
        }

        control_loop_state
    }

    pub async fn run<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        loop {
            match control_loop_state.command_receiver.recv().await {
                // [impl->swdd~agent-workload-tasks-executes-delete~1]
                Some(WorkloadCommand::Delete) => {
                    control_loop_state.restart_state.disable_restarts();
                    log::debug!("Received WorkloadCommand::Delete, disable restarts.");

                    if let Some(new_control_loop_state) = Self::do_delete(control_loop_state).await
                    {
                        control_loop_state = new_control_loop_state;
                    } else {
                        return;
                    }
                }
                // [impl->swdd~agent-workload-task-executes-update~1]
                Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                    control_loop_state.restart_state.disable_restarts();
                    log::debug!("Received WorkloadCommand::Update, disable_restarts");

                    control_loop_state = Self::do_update(
                        control_loop_state,
                        *runtime_workload_config,
                        control_interface_path,
                    )
                    .await;

                    log::debug!("Update workload complete");
                }
                Some(WorkloadCommand::Restart(runtime_workload_config, control_interface_path)) => {
                    control_loop_state = Self::do_restart(
                        control_loop_state,
                        *runtime_workload_config,
                        control_interface_path,
                    )
                    .await;
                }
                _ => {
                    log::warn!(
                        "Could not wait for internal stop command for workload '{}'.",
                        control_loop_state.workload_name,
                    );
                    return;
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
mod tests {
    use std::time::Duration;

    use common::{
        commands::UpdateWorkloadState,
        objects::{ExecutionState, WorkloadState},
        state_change_interface::StateChangeCommand,
        test_utils::generate_test_workload_spec_with_param,
    };
    use tokio::{sync::mpsc, time::timeout};

    use crate::{
        runtime_connectors::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
        workload::{ControlLoopState, RestartState, WorkloadCommandChannel, WorkloadControlLoop},
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "workload_id_1";
    const PIPES_LOCATION: &str = "/some/path";
    const OLD_WORKLOAD_ID: &str = "old_workload_id";

    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 5;

    // Unfortunately this test also executes a delete of the newly updated workload.
    // We could not avoid this as it is the only possibility to check the internal variables
    // and to properly stop the control loop in the await new command method
    // [utest->swdd~agent-workload-task-executes-update~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also we stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(workload_spec.clone(), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(old_mock_state_checker),
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-task-update-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_broken_allowed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also be stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(workload_spec.clone(), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: None,
            state_checker: None,
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-task-update-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_delete_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(
                    OLD_WORKLOAD_ID.to_string(),
                    Err(crate::runtime_connectors::RuntimeError::Delete(
                        "some delete error".to_string(),
                    )),
                ),
                // Since we also send a delete command to exit the control loop properly, we need to delete the workload mow
                // This also tests if the old workload id was properly stored.
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(workload_spec.clone(), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(old_mock_state_checker),
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-task-update-create-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_create_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // We also send a delete command, but as no new workload was generated, there is also no
                // new ID so no call to the runtime is expected to happen here.
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(workload_spec.clone(), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(old_mock_state_checker),
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-tasks-executes-delete~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut mock_state_checker = StubStateChecker::new();
        mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![RuntimeCall::DeleteWorkload(
                OLD_WORKLOAD_ID.to_string(),
                Ok(()),
            )])
            .await;

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(mock_state_checker),
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-task-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, mut state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut mock_state_checker = StubStateChecker::new();
        mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(
                    OLD_WORKLOAD_ID.to_string(),
                    Err(crate::runtime_connectors::RuntimeError::Delete(
                        "some delete error".to_string(),
                    )),
                ),
                // First fail, now success
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.clone().delete().await.unwrap();
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(mock_state_checker),
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: WORKLOAD_1_NAME.to_string(),
                agent_name: AGENT_NAME.to_string(),
                execution_state: ExecutionState::ExecRemoved,
            }],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), state_change_rx.recv()).await,
            Ok(Some(StateChangeCommand::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-task-delete-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_already_gone() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let runtime_mock = MockRuntimeConnector::new();

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.clone().delete().await.unwrap();

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: None,
            state_checker: None,
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    #[tokio::test]
    async fn utest_workload_obj_run_restart_successful_after_create_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .restart(workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: None,
            state_checker: None,
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(700),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    #[tokio::test]
    async fn utest_workload_obj_run_restart_exceeded_workload_creation() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_expectations = vec![];

        // instead of short vector initialization a for loop is used because RuntimeCall with its submembers shall not be clonable.
        for _ in 0..super::MAX_RESTARTS {
            runtime_expectations.push(RuntimeCall::CreateWorkload(
                workload_spec.clone(),
                Some(PIPES_LOCATION.into()),
                state_change_tx.clone(),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    "some create error".to_string(),
                )),
            ));
        }

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_sender
            .restart(workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        // We also send a delete command, but as no new workload was generated, there is also no
        // new ID so no call to the runtime is expected to happen here.
        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: None,
            state_checker: None,
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(700),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    #[tokio::test]
    async fn utest_workload_obj_run_restart_stop_restart_commands_on_update_command() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandChannel::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    state_change_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .restart(workload_spec.clone(), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        workload_command_sender
            .update(workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_NAME.to_string(),
            workload_id: None,
            state_checker: None,
            update_state_tx: state_change_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_state: RestartState::new(),
        };

        assert!(timeout(
            Duration::from_millis(700),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }
}
