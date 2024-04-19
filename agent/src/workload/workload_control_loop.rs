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
use crate::runtime_connectors::StateChecker;
use crate::workload::{ControlLoopState, WorkloadCommand};
use crate::workload_state::WorkloadStateSenderInterface;
use common::objects::{
    ExecutionState, RestartAllowed, WorkloadInstanceName, WorkloadSpec, WorkloadState,
};
use futures_util::Future;
use std::path::PathBuf;

#[cfg(not(test))]
const MAX_RETRIES: usize = 20;

#[cfg(test)]
const MAX_RETRIES: usize = 2;

#[cfg(not(test))]
const RETRY_WAITING_TIME_MS: u64 = 1000;

#[cfg(test)]
const RETRY_WAITING_TIME_MS: u64 = 50;

pub struct RetryCounter {
    retry_counter: usize,
}

impl RetryCounter {
    pub fn new() -> Self {
        RetryCounter { retry_counter: 1 }
    }

    pub fn reset(&mut self) {
        self.retry_counter = 1;
    }

    pub fn limit(&self) -> usize {
        MAX_RETRIES
    }

    pub fn limit_exceeded(&self) -> bool {
        self.retry_counter > MAX_RETRIES
    }

    pub fn count_retry(&mut self) {
        if self.retry_counter <= MAX_RETRIES {
            self.retry_counter += 1;
        }
    }

    pub fn current_retry(&self) -> usize {
        self.retry_counter
    }
}

pub struct WorkloadControlLoop;

impl WorkloadControlLoop {
    pub async fn run<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        loop {
            tokio::select! {
                received_workload_state = control_loop_state.state_checker_workload_state_receiver.recv() => {
                    log::trace!("Received new workload state for workload '{}'",
                        control_loop_state.workload_spec.instance_name.workload_name());

                    control_loop_state = Self::handle_restart_on_received_workload_state(control_loop_state, received_workload_state).await;
                    log::trace!("Restart handling done.");
                }
                workload_command = control_loop_state.command_receiver.recv() => {
                    match workload_command {
                        // [impl->swdd~agent-workload-control-loop-executes-delete~2]
                        Some(WorkloadCommand::Delete) => {
                            log::debug!("Received WorkloadCommand::Delete.");

                            if let Some(new_control_loop_state) = Self::delete(control_loop_state).await {
                                control_loop_state = new_control_loop_state;
                            } else {
                                // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
                                return;
                            }
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-update~2]
                        Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                            log::debug!("Received WorkloadCommand::Update.");

                            control_loop_state = Self::update(
                                control_loop_state,
                                runtime_workload_config,
                                control_interface_path,
                            )
                            .await;

                            log::debug!("Update workload complete");
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-retry~1]
                        Some(WorkloadCommand::Retry(instance_name)) => {
                            log::debug!("Received WorkloadCommand::Retry.");

                            control_loop_state = Self::retry_create(
                                control_loop_state,
                                *instance_name,
                            )
                            .await;
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-create~2]
                        Some(WorkloadCommand::Create) => {
                            log::debug!("Received WorkloadCommand::Create.");

                            control_loop_state = Self::create(
                                control_loop_state,
                                Self::send_retry,
                            )
                            .await;
                        }
                        _ => {
                            log::warn!(
                                "Could not wait for internal stop command for workload '{}'.",
                                control_loop_state.instance_name().workload_name(),
                            );
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn handle_restart_on_received_workload_state<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        received_workload_state: Option<WorkloadState>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if let Some(new_workload_state) = received_workload_state {
            /* forward immediately the new workload state to the agent manager
            to avoid delays through the restart handling */
            let workload_state = new_workload_state.clone();
            control_loop_state
                .workload_state_sender
                .report_workload_execution_state(
                    &workload_state.instance_name,
                    workload_state.execution_state,
                )
                .await;

            let restart_policy = &control_loop_state.workload_spec.restart_policy;
            if restart_policy.is_restart_allowed(&new_workload_state.execution_state) {
                log::debug!(
                    "Restart workload '{}' with restart policy '{}' caused by current execution state '{}'.",
                    control_loop_state.workload_spec.instance_name.workload_name(),
                    restart_policy,
                    new_workload_state.execution_state
                );

                let workload_spec = control_loop_state.workload_spec.clone();
                let control_interface_path = control_loop_state.control_interface_path.clone();
                control_loop_state = Self::update(
                    control_loop_state,
                    Some(Box::new(workload_spec)),
                    control_interface_path,
                )
                .await;
            } else {
                log::trace!(
                    "Restart not allowed for workload '{}'.",
                    control_loop_state
                        .workload_spec
                        .instance_name
                        .workload_name()
                );
            }
        }

        control_loop_state
    }

    async fn send_retry<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceName,
        error_msg: String,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        log::info!(
            "Failed to create workload: '{}': '{}'",
            instance_name.workload_name(),
            error_msg
        );
        control_loop_state.workload_id = None;
        control_loop_state.state_checker = None;

        // [impl->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
        control_loop_state
            .retry_sender
            .retry(instance_name)
            .await
            .unwrap_or_else(|err| log::info!("Could not send WorkloadCommand::Retry: '{}'", err));
        control_loop_state
    }

    async fn send_retry_delayed<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceName,
        error_msg: String,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        control_loop_state.workload_id = None;
        control_loop_state.state_checker = None;

        log::info!(
            "Retry '{}' out of '{}': Failed to create workload: '{}': '{}'",
            control_loop_state.retry_counter.current_retry(),
            control_loop_state.retry_counter.limit(),
            instance_name,
            error_msg
        );

        let retry_counter: &mut RetryCounter = &mut control_loop_state.retry_counter;
        retry_counter.count_retry();

        // [impl->swdd~agent-workload-control-loop-limits-retry-attempts~1]
        if retry_counter.limit_exceeded() {
            log::warn!(
                "Abort retries: reached maximum amount of retries ('{}')",
                retry_counter.limit()
            );

            // [impl->swdd~agent-workload-control-loop-retry-limit-set-execution-state~1]
            control_loop_state
                .workload_state_sender
                .report_workload_execution_state(
                    control_loop_state.instance_name(),
                    ExecutionState::retry_failed_no_retry(),
                )
                .await;
            return control_loop_state;
        }

        let sender = control_loop_state.retry_sender.clone();
        tokio::task::spawn(async move {
            // [impl->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
            tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_WAITING_TIME_MS)).await;
            log::debug!("Send WorkloadCommand::Retry.");

            sender.retry(instance_name).await.unwrap_or_else(|err| {
                log::info!("Could not send WorkloadCommand::Retry: '{}'", err)
            });
        });
        control_loop_state
    }

    async fn update_create<WorkloadId, StChecker, Fut>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        new_workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        func_on_error: impl FnOnce(
            ControlLoopState<WorkloadId, StChecker>,
            WorkloadInstanceName,
            String,
        ) -> Fut,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
        Fut: Future<Output = ControlLoopState<WorkloadId, StChecker>>,
    {
        control_loop_state.workload_spec = new_workload_spec;
        control_loop_state.control_interface_path = control_interface_path;
        Self::create(control_loop_state, func_on_error).await
    }

    // [impl->swdd~agent-workload-control-loop-executes-create~2]
    async fn create<WorkloadId, StChecker, Fut>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        func_on_error: impl FnOnce(
            ControlLoopState<WorkloadId, StChecker>,
            WorkloadInstanceName,
            String,
        ) -> Fut,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
        Fut: Future<Output = ControlLoopState<WorkloadId, StChecker>>,
    {
        control_loop_state
            .workload_state_sender
            .report_workload_execution_state(
                control_loop_state.instance_name(),
                ExecutionState::starting_triggered(),
            )
            .await;

        let new_instance_name = control_loop_state.workload_spec.instance_name.clone();

        match control_loop_state
            .runtime
            .create_workload(
                control_loop_state.workload_spec.clone(),
                control_loop_state.control_interface_path.clone(),
                control_loop_state
                    .state_checker_workload_state_sender
                    .clone(),
            )
            .await
        {
            Ok((new_workload_id, new_state_checker)) => {
                log::debug!(
                    "Created workload '{}' successfully.",
                    new_instance_name.workload_name()
                );
                control_loop_state.workload_id = Some(new_workload_id);
                control_loop_state.state_checker = Some(new_state_checker);
                control_loop_state
            }
            Err(err) => {
                control_loop_state
                    .workload_state_sender
                    .report_workload_execution_state(
                        &new_instance_name,
                        ExecutionState::starting_failed(err.to_string()),
                    )
                    .await;

                func_on_error(control_loop_state, new_instance_name, err.to_string()).await
            }
        }
    }

    // [impl->swdd~agent-workload-control-loop-executes-delete~2]
    async fn delete<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) -> Option<ControlLoopState<WorkloadId, StChecker>>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        control_loop_state
            .workload_state_sender
            .report_workload_execution_state(
                control_loop_state.instance_name(),
                ExecutionState::stopping_requested(),
            )
            .await;

        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                control_loop_state
                    .workload_state_sender
                    .report_workload_execution_state(
                        control_loop_state.instance_name(),
                        ExecutionState::delete_failed(err.to_string()),
                    )
                    .await;
                // [impl->swdd~agent-workload-control-loop-delete-failed-allows-retry~1]
                log::warn!(
                    "Could not stop workload '{}': '{}'",
                    control_loop_state.instance_name().workload_name(),
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
            // [impl->swdd~agent-workload-control-loop-delete-broken-allowed~1]
            log::debug!(
                "Workload '{}' already gone.",
                control_loop_state.instance_name().workload_name()
            );
        }

        // Successfully stopped the workload. Send a removed on the channel
        control_loop_state
            .workload_state_sender
            .report_workload_execution_state(
                control_loop_state.instance_name(),
                ExecutionState::removed(),
            )
            .await;

        None
    }

    // [impl->swdd~agent-workload-control-loop-executes-update~2]
    async fn update<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        new_workload_spec: Option<Box<WorkloadSpec>>,
        control_interface_path: Option<PathBuf>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        control_loop_state
            .workload_state_sender
            .report_workload_execution_state(
                control_loop_state.instance_name(),
                ExecutionState::stopping_requested(),
            )
            .await;

        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                control_loop_state
                    .workload_state_sender
                    .report_workload_execution_state(
                        control_loop_state.instance_name(),
                        ExecutionState::delete_failed(err.to_string()),
                    )
                    .await;
                // [impl->swdd~agent-workload-control-loop-update-delete-failed-allows-retry~1]
                log::warn!(
                    "Could not update workload '{}': '{}'",
                    control_loop_state.instance_name().workload_name(),
                    err
                );
                control_loop_state.workload_id = Some(old_id);

                return control_loop_state;
            } else if let Some(old_checker) = control_loop_state.state_checker.take() {
                old_checker.stop_checker().await;
            }
        } else {
            // [impl->swdd~agent-workload-control-loop-update-broken-allowed~1]
            log::debug!(
                "Workload '{}' already gone.",
                control_loop_state.instance_name().workload_name()
            );
        }

        // workload is deleted or already gone, send the remove state
        control_loop_state
            .workload_state_sender
            .report_workload_execution_state(
                control_loop_state.instance_name(),
                ExecutionState::removed(),
            )
            .await;

        // [impl->swdd~agent-workload-control-loop-reset-retry-attempts-on-update~1]
        control_loop_state.retry_counter.reset();

        // [impl->swdd~agent-workload-control-loop-executes-update-delete-only~1]
        if let Some(spec) = new_workload_spec {
            // [impl->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
            control_loop_state = Self::update_create(
                control_loop_state,
                *spec,
                control_interface_path,
                Self::send_retry,
            )
            .await;
        }
        control_loop_state
    }

    async fn retry_create<WorkloadId, StChecker>(
        control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceName,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if control_loop_state.instance_name() == &instance_name
            && control_loop_state.workload_id.is_none()
        {
            log::debug!("Next retry attempt.");
            Self::create(control_loop_state, Self::send_retry_delayed).await
        } else {
            // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
            log::debug!("Skip retry creation of workload.");
            control_loop_state
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

    use common::objects::{
        generate_test_workload_spec_with_param, ExecutionState, WorkloadInstanceName,
    };

    use tokio::{sync::mpsc, time::timeout};

    use crate::{
        runtime_connectors::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
        workload::{ControlLoopState, WorkloadCommandSender, WorkloadControlLoop},
        workload_state::assert_execution_state_sequence,
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "workload_id_1";
    const WORKLOAD_ID_2: &str = "workload_id_2";
    const WORKLOAD_ID_3: &str = "workload_id_3";
    const PIPES_LOCATION: &str = "/some/path";
    const OLD_WORKLOAD_ID: &str = "old_workload_id";

    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 20;

    // Unfortunately this test also executes a delete of the newly updated workload.
    // We could not avoid this as it is the only possibility to check the internal variables
    // and to properly stop the control loop in the await new command method
    // [utest->swdd~agent-workload-control-loop-executes-update~2]
    #[tokio::test]
    async fn utest_workload_obj_run_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also we stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let old_workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = old_workload_spec.clone();
        new_workload_spec.runtime_config = "changed config".to_owned();
        new_workload_spec.instance_name = WorkloadInstanceName::builder()
            .agent_name(old_workload_spec.instance_name.agent_name())
            .workload_name(old_workload_spec.instance_name.workload_name())
            .config(&new_workload_spec.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(Some(new_workload_spec.clone()), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let old_instance_name = old_workload_spec.instance_name.clone();
        let new_instance_name = new_workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(old_mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
                (&new_instance_name, ExecutionState::starting_triggered()),
                (&new_instance_name, ExecutionState::stopping_requested()),
                (&new_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-update-delete-only~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_delete_only() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let old_workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                // The workload was already deleted with the previous runtime call delete.
            ])
            .await;

        // Send only the update to delete the workload
        workload_command_sender
            .update(None, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let old_instance_name = old_workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(old_mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-update-delete-only~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_after_update_delete_only() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also we stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let old_workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = old_workload_spec.clone();
        new_workload_spec.runtime_config = "changed config".to_owned();
        new_workload_spec.instance_name = WorkloadInstanceName::builder()
            .agent_name(old_workload_spec.instance_name.agent_name())
            .workload_name(old_workload_spec.instance_name.workload_name())
            .config(&new_workload_spec.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Delete the new updated workload to exit the infinite loop
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update delete only
        workload_command_sender
            .update(None, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        // Send the update
        workload_command_sender
            .update(Some(new_workload_spec.clone()), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let old_instance_name = old_workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(old_mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-update-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_broken_allowed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also be stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let old_workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = old_workload_spec.clone();
        new_workload_spec.runtime_config = "changed config".to_owned();
        new_workload_spec.instance_name = WorkloadInstanceName::builder()
            .agent_name(old_workload_spec.instance_name.agent_name())
            .workload_name(old_workload_spec.instance_name.workload_name())
            .config(&new_workload_spec.runtime_config)
            .build();
        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(Some(new_workload_spec.clone()), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let old_instance_name = old_workload_spec.instance_name.clone();
        let new_instance_name = new_workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_id(None)
            .state_checker(None)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
                (&new_instance_name, ExecutionState::starting_triggered()),
                (&new_instance_name, ExecutionState::stopping_requested()),
                (&new_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-update-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_delete_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let old_workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = old_workload_spec.clone();
        new_workload_spec.runtime_config = "changed config".to_owned();
        new_workload_spec.instance_name = WorkloadInstanceName::builder()
            .agent_name(old_workload_spec.instance_name.agent_name())
            .workload_name(old_workload_spec.instance_name.workload_name())
            .config(&new_workload_spec.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(
                    OLD_WORKLOAD_ID.to_string(),
                    Err(crate::runtime_connectors::RuntimeError::Delete(
                        "some delete error".to_string(),
                    )),
                ),
                // Since we also send a delete command to exit the control loop properly, we need to delete the workload now
                // This also tests if the old workload id was properly stored.
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(Some(new_workload_spec.clone()), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let old_instance_name = old_workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(old_mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionState::stopping_requested()),
                (
                    &old_instance_name,
                    ExecutionState::delete_failed("some delete error"),
                ),
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_create_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let old_workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = old_workload_spec.clone();
        new_workload_spec.runtime_config = "changed config".to_owned();
        new_workload_spec.instance_name = WorkloadInstanceName::builder()
            .agent_name(old_workload_spec.instance_name.agent_name())
            .workload_name(old_workload_spec.instance_name.workload_name())
            .config(&new_workload_spec.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
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
            .update(Some(new_workload_spec.clone()), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.clone().delete().await.unwrap();

        let old_instance_name = old_workload_spec.instance_name.clone();
        let new_instance_name = new_workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(old_mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionState::stopping_requested()),
                (&old_instance_name, ExecutionState::removed()),
                (&new_instance_name, ExecutionState::starting_triggered()),
                (
                    &new_instance_name,
                    ExecutionState::starting_failed("some create error"),
                ),
                (&new_instance_name, ExecutionState::stopping_requested()),
                (&new_instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-delete~2]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

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

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&instance_name, ExecutionState::stopping_requested()),
                (&instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

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

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let instance_name = workload_spec.instance_name.clone();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_id(Some(OLD_WORKLOAD_ID.to_string()))
            .state_checker(Some(mock_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&instance_name, ExecutionState::stopping_requested()),
                (
                    &instance_name,
                    ExecutionState::delete_failed("some delete error"),
                ),
                (&instance_name, ExecutionState::stopping_requested()),
                (&instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-delete-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_already_gone() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let runtime_mock = MockRuntimeConnector::new();

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.clone().delete().await.unwrap();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
    #[tokio::test]
    async fn utest_workload_obj_run_create_successful() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
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
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender.create().await.unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(state_change_tx.clone())
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(100),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
    // [utest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_creation_successful_after_create_command_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
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
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender.create().await.unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
    // [utest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_create_with_retry_workload_command_channel_closed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let runtime_expectations = vec![RuntimeCall::CreateWorkload(
            workload_spec.clone(),
            Some(PIPES_LOCATION.into()),
            Err(crate::runtime_connectors::RuntimeError::Create(
                "some create error".to_string(),
            )),
        )];

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_receiver.close();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        let new_control_loop_state =
            WorkloadControlLoop::create(control_loop_state, WorkloadControlLoop::send_retry).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // send some randomly selected command
        assert!(new_control_loop_state.retry_sender.delete().await.is_err());
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_creation_successful_after_create_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name.clone();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .retry(instance_name.clone())
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    // [utest->swdd~agent-workload-control-loop-limits-retry-attempts~1]
    // [utest->swdd~agent-workload-control-loop-retry-limit-set-execution-state~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_attempts_exceeded_workload_creation() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name.clone();

        let mut runtime_expectations = vec![];

        // instead of short vector initialization a for loop is used because RuntimeCall with its submembers shall not be clone-able.
        for _ in 0..super::MAX_RETRIES {
            runtime_expectations.push(RuntimeCall::CreateWorkload(
                workload_spec.clone(),
                Some(PIPES_LOCATION.into()),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    "some create error".to_string(),
                )),
            ));
        }

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_sender
            .retry(instance_name.clone())
            .await
            .unwrap();

        // We also send a delete command, but as no new workload was generated, there is also no
        // new ID so no call to the runtime is expected to happen here.
        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&instance_name, ExecutionState::starting_triggered()),
                (
                    &instance_name,
                    ExecutionState::starting_failed("some create error"),
                ),
                (&instance_name, ExecutionState::starting_triggered()),
                (
                    &instance_name,
                    ExecutionState::starting_failed("some create error"),
                ),
                (&instance_name, ExecutionState::retry_failed_no_retry()),
                (&instance_name, ExecutionState::stopping_requested()),
                (&instance_name, ExecutionState::removed()),
            ],
        )
        .await;

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_creation_workload_command_channel_closed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name.clone();

        let runtime_expectations = vec![RuntimeCall::CreateWorkload(
            workload_spec.clone(),
            Some(PIPES_LOCATION.into()),
            Err(crate::runtime_connectors::RuntimeError::Create(
                "some create error".to_string(),
            )),
        )];

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_receiver.close();

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        let new_control_loop_state =
            WorkloadControlLoop::retry_create(control_loop_state, instance_name).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // send some randomly selected command
        assert!(new_control_loop_state.retry_sender.delete().await.is_err());
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_stop_retry_commands_on_update_command() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let instance_name = workload_spec.instance_name.clone();

        let mut new_workload_spec = workload_spec.clone();
        new_workload_spec.runtime_config = "Changed".to_string();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender.retry(instance_name).await.unwrap();

        workload_command_sender
            .update(Some(new_workload_spec), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_on_update_with_create_failure() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = workload_spec.clone();
        new_workload_spec.runtime_config = "Changed".to_string();

        let mut old_state_checker = StubStateChecker::new();
        old_state_checker.panic_if_not_stopped();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                // the update deletes first the old workload
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
                // next the create workload fails
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // after 1 retry attempt the create with the new workload is successful
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .update(Some(new_workload_spec), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(125)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_id(Some(WORKLOAD_ID.into()))
            .state_checker(Some(old_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
    // [utest->swdd~agent-workload-control-loop-reset-retry-attempts-on-update~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_reset_retry_counter_on_update() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec = workload_spec.clone();
        new_workload_spec.runtime_config = "Changed".to_string();

        let mut old_state_checker = StubStateChecker::new();
        old_state_checker.panic_if_not_stopped();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                // the update deletes first the old workload
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
                // next the create workload fails
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // after 1 retry attempt the create with the new workload is successful
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .update(Some(new_workload_spec), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(125)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_id(Some(WORKLOAD_ID.into()))
            .state_checker(Some(old_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        // simulate an already incremented retry counter due to retry attempts on initial workload creation
        control_loop_state.retry_counter.count_retry();
        control_loop_state.retry_counter.count_retry();
        assert_eq!(control_loop_state.retry_counter.current_retry(), 3);

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_create_correct_workload_on_two_updates() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut new_workload_spec_update1 = workload_spec.clone();
        new_workload_spec_update1.runtime_config = "Changed".to_string();

        let mut new_workload_spec_update2 = workload_spec.clone();
        new_workload_spec_update2.runtime_config = "Changed again".to_string();

        let mut old_state_checker = StubStateChecker::new();
        old_state_checker.panic_if_not_stopped();

        let mut new_mock_state_checker_update2 = StubStateChecker::new();
        new_mock_state_checker_update2.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                // the update deletes first the old workload
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
                // next the create workload fails for the first update
                RuntimeCall::CreateWorkload(
                    new_workload_spec_update1.clone(),
                    Some(PIPES_LOCATION.into()),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // next the second update is executed and shall be successful
                // no delete expected because no workload was created within update1
                RuntimeCall::CreateWorkload(
                    new_workload_spec_update2.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID_3.to_string(), new_mock_state_checker_update2)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_3.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .update(Some(new_workload_spec_update1), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        workload_command_sender
            .update(Some(new_workload_spec_update2), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(125)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_id(Some(WORKLOAD_ID.into()))
            .state_checker(Some(old_state_checker))
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }
}
