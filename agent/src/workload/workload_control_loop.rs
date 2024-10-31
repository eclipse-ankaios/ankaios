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
use crate::workload_state::{WorkloadStateSender, WorkloadStateSenderInterface};
use common::objects::{ExecutionState, RestartPolicy, WorkloadInstanceName, WorkloadSpec};
use common::std_extensions::IllegalStateResult;
use futures_util::Future;
use std::path::PathBuf;
use std::str::FromStr;

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
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        loop {
            tokio::select! {
                // [impl->swdd~workload-control-loop-receives-workload-states~1]
                received_workload_state = control_loop_state.state_checker_workload_state_receiver.recv() => {
                    log::trace!("Received new workload state for workload '{}'",
                        control_loop_state.workload_spec.instance_name.workload_name());

                    let new_workload_state = received_workload_state
                        .ok_or("Channel to listen to workload states of state checker closed.")
                        .unwrap_or_illegal_state();

                    // [impl->swdd~workload-control-loop-checks-workload-state-validity~1]
                    if Self::is_same_workload(control_loop_state.instance_name(), &new_workload_state.instance_name) {

                        /* forward immediately the new workload state to the agent manager
                        to avoid delays through the restart handling */
                        // [impl->swdd~workload-control-loop-sends-workload-states~2]
                        Self::send_workload_state_to_agent(
                            &control_loop_state.to_agent_workload_state_sender,
                            &new_workload_state.instance_name,
                            new_workload_state.execution_state.clone(),
                        ).await;

                        // [impl->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
                        if Self::restart_policy_matches_execution_state(&control_loop_state.workload_spec.restart_policy, &new_workload_state.execution_state) {
                            // [impl->swdd~workload-control-loop-handles-workload-restarts~2]
                            control_loop_state = Self::restart_workload_on_runtime(control_loop_state).await;
                        }
                    }

                    log::trace!("Restart handling done.");
                }
                workload_command = control_loop_state.command_receiver.recv() => {
                    match workload_command {
                        // [impl->swdd~agent-workload-control-loop-executes-delete~2]
                        Some(WorkloadCommand::Delete) => {
                            log::debug!("Received WorkloadCommand::Delete.");

                            if let Some(new_control_loop_state) = Self::delete_workload_on_runtime(control_loop_state).await {
                                control_loop_state = new_control_loop_state;
                            } else {
                                // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
                                return;
                            }
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-update~2]
                        Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                            log::debug!("Received WorkloadCommand::Update.");

                            control_loop_state = Self::update_workload_on_runtime(
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

                            control_loop_state = Self::retry_create_workload_on_runtime(
                                control_loop_state,
                                *instance_name,
                            )
                            .await;
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-create~3]
                        Some(WorkloadCommand::Create) => {
                            log::debug!("Received WorkloadCommand::Create.");

                            Self::send_workload_state_to_agent(
                                &control_loop_state.to_agent_workload_state_sender,
                                control_loop_state.instance_name(),
                                ExecutionState::starting_triggered(),
                            )
                            .await;

                            control_loop_state = Self::create_workload_on_runtime(
                                control_loop_state,
                                Self::send_retry_for_workload,
                            )
                            .await;
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-resume~1]
                        Some(WorkloadCommand::Resume) => {
                            log::debug!("Received WorkloadCommand::Resume.");
                            control_loop_state = Self::resume_workload_on_runtime(control_loop_state).await;
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

    async fn send_workload_state_to_agent(
        workload_state_sender: &WorkloadStateSender,
        instance_name: &WorkloadInstanceName,
        execution_state: ExecutionState,
    ) {
        workload_state_sender
            .report_workload_execution_state(instance_name, execution_state)
            .await;
    }

    async fn restart_workload_on_runtime<WorkloadId, StChecker>(
        control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        log::debug!(
            "Restart workload '{}' with restart policy '{}'",
            control_loop_state
                .workload_spec
                .instance_name
                .workload_name(),
            control_loop_state.workload_spec.restart_policy,
        );

        let workload_spec = control_loop_state.workload_spec.clone();
        let control_interface_path = control_loop_state.control_interface_path.clone();

        // update the workload with its existing config since a restart is represented by an update operation
        // [impl->swdd~workload-control-loop-restarts-workloads-using-update~1]
        Self::update_workload_on_runtime(
            control_loop_state,
            Some(Box::new(workload_spec)),
            control_interface_path,
        )
        .await
    }

    fn is_same_workload(
        lhs_instance_name: &WorkloadInstanceName,
        rhs_instance_name: &WorkloadInstanceName,
    ) -> bool {
        lhs_instance_name.eq(rhs_instance_name)
    }

    fn restart_policy_matches_execution_state(
        restart_policy: &RestartPolicy,
        execution_state: &ExecutionState,
    ) -> bool {
        match restart_policy {
            RestartPolicy::Never => false,
            RestartPolicy::OnFailure => execution_state.is_failed(),
            RestartPolicy::Always => execution_state.is_failed() || execution_state.is_succeeded(),
        }
    }

    async fn send_retry_for_workload<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceName,
        error_msg: String,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        log::info!(
            "Retrying workload creation for: '{}'. Error: '{}'",
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

    async fn send_retry_when_limit_not_exceeded<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceName,
        error_msg: String,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
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

            // [impl->swdd~agent-workload-control-loop-retry-limit-set-execution-state~2]
            Self::send_workload_state_to_agent(
                &control_loop_state.to_agent_workload_state_sender,
                control_loop_state.instance_name(),
                ExecutionState::retry_failed_no_retry(error_msg),
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

    // [impl->swdd~agent-workload-control-loop-executes-create~3]
    async fn create_workload_on_runtime<WorkloadId, StChecker, ErrorFunc, Fut>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        func_on_error: ErrorFunc,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
        Fut: Future<Output = ControlLoopState<WorkloadId, StChecker>> + 'static,
        ErrorFunc: FnOnce(ControlLoopState<WorkloadId, StChecker>, WorkloadInstanceName, String) -> Fut
            + 'static,
    {
        let new_instance_name = control_loop_state.workload_spec.instance_name.clone();

        match control_loop_state
            .runtime
            .create_workload(
                control_loop_state.workload_spec.clone(),
                control_loop_state.workload_id.clone(),
                control_loop_state.control_interface_path.clone(),
                control_loop_state
                    .state_checker_workload_state_sender
                    .clone(),
            )
            .await
        {
            Ok((new_workload_id, new_state_checker)) => {
                log::info!(
                    "Successfully created workload '{}'.",
                    new_instance_name.workload_name()
                );
                control_loop_state.workload_id = Some(new_workload_id);
                control_loop_state.state_checker = Some(new_state_checker);
                control_loop_state
            }
            Err(err) => {
                let current_retry_counter = control_loop_state.retry_counter.current_retry();

                Self::send_workload_state_to_agent(
                    &control_loop_state.to_agent_workload_state_sender,
                    &new_instance_name,
                    ExecutionState::retry_starting(
                        current_retry_counter,
                        MAX_RETRIES,
                        err.to_string(),
                    ),
                )
                .await;

                func_on_error(control_loop_state, new_instance_name, err.to_string()).await
            }
        }
    }

    // [impl->swdd~agent-workload-control-loop-executes-delete~2]
    async fn delete_workload_on_runtime<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) -> Option<ControlLoopState<WorkloadId, StChecker>>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionState::stopping_requested(),
        )
        .await;

        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                Self::send_workload_state_to_agent(
                    &control_loop_state.to_agent_workload_state_sender,
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
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionState::removed(),
        )
        .await;

        None
    }

    // [impl->swdd~agent-workload-control-loop-executes-update~2]
    async fn update_workload_on_runtime<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        new_workload_spec: Option<Box<WorkloadSpec>>,
        control_interface_path: Option<PathBuf>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionState::stopping_requested(),
        )
        .await;

        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                Self::send_workload_state_to_agent(
                    &control_loop_state.to_agent_workload_state_sender,
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
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionState::removed(),
        )
        .await;

        // [impl->swdd~agent-workload-control-loop-reset-retry-attempts-on-update~1]
        control_loop_state.retry_counter.reset();

        // [impl->swdd~agent-workload-control-loop-executes-update-delete-only~1]
        if let Some(spec) = new_workload_spec {
            // [impl->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
            control_loop_state.workload_spec = *spec;
            control_loop_state.control_interface_path = control_interface_path;

            Self::send_workload_state_to_agent(
                &control_loop_state.to_agent_workload_state_sender,
                control_loop_state.instance_name(),
                ExecutionState::starting_triggered(),
            )
            .await;

            control_loop_state =
                Self::create_workload_on_runtime(control_loop_state, Self::send_retry_for_workload)
                    .await;
        }
        control_loop_state
    }

    async fn retry_create_workload_on_runtime<WorkloadId, StChecker>(
        control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceName,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if Self::is_same_workload(control_loop_state.instance_name(), &instance_name)
            && control_loop_state.workload_id.is_none()
        {
            log::debug!("Next retry attempt.");
            Self::create_workload_on_runtime(
                control_loop_state,
                Self::send_retry_when_limit_not_exceeded,
            )
            .await
        } else {
            // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~1]
            log::debug!("Skip retry creation of workload.");
            control_loop_state
        }
    }

    // [impl->swdd~agent-workload-control-loop-executes-resume~1]
    async fn resume_workload_on_runtime<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let workload_name = control_loop_state.instance_name().workload_name();
        let workload_id = control_loop_state
            .runtime
            .get_workload_id(&control_loop_state.workload_spec.instance_name)
            .await;

        let state_checker: Option<StChecker> = match workload_id.as_ref() {
            Ok(wl_id) => control_loop_state
                .runtime
                .start_checker(
                    wl_id,
                    control_loop_state.workload_spec.clone(),
                    control_loop_state
                        .state_checker_workload_state_sender
                        .clone(),
                )
                .await
                .map_err(|err| {
                    log::warn!(
                        "Failed to start state checker when resuming workload '{}': '{}'",
                        workload_name,
                        err
                    );
                    err
                })
                .ok(),
            Err(err) => {
                log::warn!(
                    "Failed to get workload id when resuming workload '{}': '{}'",
                    workload_name,
                    err
                );
                None
            }
        };

        // assign the workload id and state checker to the control loop state
        control_loop_state.workload_id = workload_id.ok();
        control_loop_state.state_checker = state_checker;
        control_loop_state
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
mockall::mock! {
    pub WorkloadControlLoop {
        pub async fn run<WorkloadId, StChecker>(
            control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        )
        where
            WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
            StChecker: StateChecker<WorkloadId> + Send + Sync + 'static;
    }
}

#[cfg(test)]
mod tests {
    use super::WorkloadControlLoop;
    use std::time::Duration;

    use common::objects::{
        generate_test_workload_spec_with_control_interface_access,
        generate_test_workload_spec_with_param, ExecutionState, WorkloadInstanceName,
    };
    use common::objects::{generate_test_workload_state_with_workload_spec, RestartPolicy};

    use tokio::{sync::mpsc, time::timeout};

    use crate::workload_state::WorkloadStateSenderInterface;
    use crate::{
        runtime_connectors::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
        workload::{ControlLoopState, WorkloadCommandSender},
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
        new_workload_spec.runtime_config = "changed config".to_string();
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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

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
        new_workload_spec.runtime_config = "changed config".to_string();
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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

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
        new_workload_spec.runtime_config = "changed config".to_string();
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
        new_workload_spec.runtime_config = "changed config".to_string();
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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

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
        new_workload_spec.runtime_config = "changed config".to_string();
        new_workload_spec.instance_name = WorkloadInstanceName::builder()
            .agent_name(old_workload_spec.instance_name.agent_name())
            .workload_name(old_workload_spec.instance_name.workload_name())
            .config(&new_workload_spec.runtime_config)
            .build();

        let create_runtime_error_msg = "some create error";
        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        create_runtime_error_msg.to_owned(),
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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(old_workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

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
                    ExecutionState::retry_starting(1, super::MAX_RETRIES, create_runtime_error_msg),
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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(mock_state_checker);

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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(mock_state_checker);

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

    // [utest->swdd~agent-workload-control-loop-executes-create~3]
    #[tokio::test]
    async fn utest_workload_obj_run_create_successful() {
        let _ = env_logger::builder().is_test(true).try_init();
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

    // [utest->swdd~agent-workload-control-loop-executes-create~3]
    // [utest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_creation_successful_after_create_command_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
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

    // [utest->swdd~agent-workload-control-loop-executes-create~3]
    // [utest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_create_with_retry_workload_command_channel_closed() {
        let _ = env_logger::builder().is_test(true).try_init();
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

        let new_control_loop_state = WorkloadControlLoop::create_workload_on_runtime(
            control_loop_state,
            WorkloadControlLoop::send_retry_for_workload,
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // send some randomly selected command
        assert!(new_control_loop_state.retry_sender.delete().await.is_err());
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    // [utest->swdd~agent-workload-control-loop-requests-retries-on-failing-retry-attempt~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_creation_successful_after_create_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
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
    // [utest->swdd~agent-workload-control-loop-retry-limit-set-execution-state~2]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_attempts_exceeded_workload_creation() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name.clone();

        let create_runtime_error_msg = "some create error";
        let mut runtime_expectations = vec![];

        // instead of short vector initialization a for loop is used because RuntimeCall with its submembers shall not be clone-able.
        for _ in 0..super::MAX_RETRIES {
            runtime_expectations.push(RuntimeCall::CreateWorkload(
                workload_spec.clone(),
                Some(PIPES_LOCATION.into()),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    create_runtime_error_msg.to_owned(),
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
                (
                    &instance_name,
                    ExecutionState::retry_starting(1, super::MAX_RETRIES, create_runtime_error_msg),
                ),
                (
                    &instance_name,
                    ExecutionState::retry_starting(2, super::MAX_RETRIES, create_runtime_error_msg),
                ),
                (
                    &instance_name,
                    ExecutionState::retry_failed_no_retry(create_runtime_error_msg),
                ),
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

        let new_control_loop_state = WorkloadControlLoop::retry_create_workload_on_runtime(
            control_loop_state,
            instance_name,
        )
        .await;

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
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_state_checker);

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
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_control_interface_access(
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
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_state_checker);

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

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec)
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_state_checker);

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

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
                RuntimeCall::GetWorkloadId(
                    workload_spec.instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::StartChecker(
                    WORKLOAD_ID.to_string(),
                    workload_spec.clone(),
                    state_checker_workload_state_sender.clone(),
                    Ok(new_mock_state_checker),
                ),
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(state_change_tx)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender.clone())
            .build()
            .unwrap();

        control_loop_state.state_checker_workload_state_sender =
            state_checker_workload_state_sender;
        control_loop_state.state_checker_workload_state_receiver =
            state_checker_workload_state_receiver;

        workload_command_sender.resume().await.unwrap();
        workload_command_sender.delete().await.unwrap();

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload_workload_id_and_state_checker_updated() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_spec.instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::StartChecker(
                    WORKLOAD_ID.to_string(),
                    workload_spec.clone(),
                    state_checker_workload_state_sender.clone(),
                    Ok(StubStateChecker::new()),
                ),
            ])
            .await;

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(state_change_tx)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender.clone())
            .build()
            .unwrap();

        control_loop_state.state_checker_workload_state_sender =
            state_checker_workload_state_sender;
        control_loop_state.state_checker_workload_state_receiver =
            state_checker_workload_state_receiver;

        assert!(control_loop_state.workload_id.is_none());
        assert!(control_loop_state.state_checker.is_none());

        let new_control_loop_state =
            WorkloadControlLoop::resume_workload_on_runtime(control_loop_state).await;

        assert_eq!(new_control_loop_state.workload_id, Some(WORKLOAD_ID.into()));
        assert!(new_control_loop_state.state_checker.is_some());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload_get_workload_id_fails() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![RuntimeCall::GetWorkloadId(
                workload_spec.instance_name.clone(),
                Err(crate::runtime_connectors::RuntimeError::List(
                    "some list workload error".to_string(),
                )),
            )])
            .await;

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(state_change_tx)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender.clone())
            .build()
            .unwrap();

        let new_control_loop_state =
            WorkloadControlLoop::resume_workload_on_runtime(control_loop_state).await;

        assert!(new_control_loop_state.workload_id.is_none());
        assert!(new_control_loop_state.state_checker.is_none());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload_start_state_checker_fails() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::GetWorkloadId(
                    workload_spec.instance_name.clone(),
                    Ok(WORKLOAD_ID.to_string()),
                ),
                RuntimeCall::StartChecker(
                    WORKLOAD_ID.to_string(),
                    workload_spec.clone(),
                    state_checker_workload_state_sender.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some state checker error".to_string(),
                    )),
                ),
            ])
            .await;

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(state_change_tx)
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender.clone())
            .build()
            .unwrap();

        control_loop_state.state_checker_workload_state_sender =
            state_checker_workload_state_sender;
        control_loop_state.state_checker_workload_state_receiver =
            state_checker_workload_state_receiver;

        let new_control_loop_state =
            WorkloadControlLoop::resume_workload_on_runtime(control_loop_state).await;

        assert_eq!(new_control_loop_state.workload_id, Some(WORKLOAD_ID.into()));
        assert!(new_control_loop_state.state_checker.is_none());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~workload-control-loop-receives-workload-states~1]
    // [utest->swdd~workload-control-loop-checks-workload-state-validity~1]
    // [utest->swdd~workload-control-loop-sends-workload-states~2]
    #[tokio::test]
    async fn utest_forward_received_workload_states_of_state_checker() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, mut workload_state_forward_rx) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![]).await;

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(70)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        // clone the control loop state's internally created workload state sender used by the state checker
        let state_checker_wl_state_sender = control_loop_state
            .state_checker_workload_state_sender
            .clone();

        let workload_state = generate_test_workload_state_with_workload_spec(
            &workload_spec,
            ExecutionState::running(),
        );

        state_checker_wl_state_sender
            .report_workload_execution_state(
                &workload_state.instance_name,
                workload_state.execution_state.clone(),
            )
            .await;

        assert!(timeout(
            Duration::from_millis(100),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        assert_eq!(
            Ok(Some(workload_state)),
            timeout(Duration::from_millis(100), workload_state_forward_rx.recv()).await
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~workload-control-loop-receives-workload-states~1]
    #[tokio::test]
    #[should_panic]
    async fn utest_panic_on_closed_workload_state_channel() {
        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, mut workload_state_forward_rx) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![]).await;

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(70)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        // close the channel to panic within the workload control loop
        workload_state_forward_rx.close();

        assert!(timeout(
            Duration::from_millis(100),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    // [utest->swdd~workload-control-loop-handles-workload-restarts~2]
    // [utest->swdd~workload-control-loop-restarts-workloads-using-update~1]
    #[tokio::test]
    async fn utest_restart_workload() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, _workload_state_forward_rx) =
            mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())), // delete operation of the restarted workload
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(70)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_spec(workload_spec.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .control_interface_path(Some(PIPES_LOCATION.into()))
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(WORKLOAD_ID.into());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        // clone the control loop state's internally created workload state sender used by the state checker
        let state_checker_wl_state_sender = control_loop_state
            .state_checker_workload_state_sender
            .clone();

        let workload_state = generate_test_workload_state_with_workload_spec(
            &workload_spec,
            ExecutionState::succeeded(),
        );

        state_checker_wl_state_sender
            .report_workload_execution_state(
                &workload_state.instance_name,
                workload_state.execution_state.clone(),
            )
            .await;

        assert!(timeout(
            Duration::from_millis(100),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    #[test]
    fn utest_is_restart_allowed_never() {
        let restart_policy = RestartPolicy::Never;

        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionState::running()
            )
        );
        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionState::succeeded()
            )
        );
        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionState::failed("some error".to_owned())
            )
        );
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    #[test]
    fn utest_is_restart_allowed_on_failure() {
        let restart_policy = RestartPolicy::OnFailure;

        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionState::running()
            )
        );
        assert!(WorkloadControlLoop::restart_policy_matches_execution_state(
            &restart_policy,
            &ExecutionState::failed("some error".to_owned())
        ));
        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionState::succeeded()
            )
        );
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    #[test]
    fn utest_restart_policy_matches_execution_state_always() {
        let restart_policy = RestartPolicy::Always;

        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionState::running()
            )
        );
        assert!(WorkloadControlLoop::restart_policy_matches_execution_state(
            &restart_policy,
            &ExecutionState::failed("some error".to_owned())
        ));
        assert!(WorkloadControlLoop::restart_policy_matches_execution_state(
            &restart_policy,
            &ExecutionState::succeeded()
        ));
    }

    // [utest->swdd~agent-sends-workload-states-of-its-workloads-to-server~2]
    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    // [utest->swdd~workload-control-loop-checks-workload-state-validity~1]
    #[test]
    fn utest_is_same_workload() {
        let current_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .agent_name(AGENT_NAME)
            .config(&String::from("existing config"))
            .build();

        assert!(WorkloadControlLoop::is_same_workload(
            &current_instance_name,
            &current_instance_name
        ));

        let new_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .agent_name(AGENT_NAME)
            .config(&String::from("different config"))
            .build();

        assert!(!WorkloadControlLoop::is_same_workload(
            &current_instance_name,
            &new_instance_name
        ));
    }
}
