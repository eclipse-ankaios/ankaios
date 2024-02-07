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
use super::workload_command_channel::WorkloadCommandReceiver;
use crate::runtime_connectors::{RuntimeConnector, StateChecker};
use crate::workload::WorkloadCommand;
use crate::workload::WorkloadCommandSender;
use common::objects::WorkloadExecutionInstanceName;
use common::{
    objects::{ExecutionState, WorkloadInstanceName, WorkloadSpec},
    std_extensions::IllegalStateResult,
    to_server_interface::{ToServerInterface, ToServerSender},
};
use futures_util::Future;
use std::path::PathBuf;

#[cfg(not(test))]
const MAX_RESTARTS: usize = 20;

#[cfg(test)]
const MAX_RESTARTS: usize = 2;

#[cfg(not(test))]
const RETRY_WAITING_TIME_MS: u64 = 1000;

#[cfg(test)]
const RETRY_WAITING_TIME_MS: u64 = 50;

pub struct RestartCounter {
    restart_counter: usize,
}

impl RestartCounter {
    pub fn new() -> Self {
        RestartCounter { restart_counter: 1 }
    }

    pub fn reset(&mut self) {
        self.restart_counter = 1;
    }

    pub fn limit(&self) -> usize {
        MAX_RESTARTS
    }

    pub fn limit_exceeded(&self) -> bool {
        self.restart_counter > MAX_RESTARTS
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
    WorkloadId: ToString + Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub instance_name: WorkloadExecutionInstanceName,
    pub workload_id: Option<WorkloadId>,
    pub state_checker: Option<StChecker>,
    pub update_state_tx: ToServerSender,
    pub runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>,
    pub command_receiver: WorkloadCommandReceiver,
    pub workload_channel: WorkloadCommandSender,
    pub restart_counter: RestartCounter,
}

pub struct WorkloadControlLoop;

impl WorkloadControlLoop {
    async fn send_restart<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        error_msg: String,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        log::info!(
            "Failed to create workload: '{}': '{}'",
            control_loop_state.instance_name.workload_name(),
            error_msg
        );
        control_loop_state.workload_id = None;
        control_loop_state.state_checker = None;

        // [impl->swdd~agent-workload-control-loop-restart-workload-on-create-failure~1]
        control_loop_state
            .workload_channel
            .restart(runtime_workload_config, control_interface_path)
            .await
            .unwrap_or_else(|err| log::info!("Could not send WorkloadCommand::Restart: '{}'", err));
        control_loop_state
    }

    async fn send_restart_delayed<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        error_msg: String,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        control_loop_state.workload_id = None;
        control_loop_state.state_checker = None;
        let restart_counter: &mut RestartCounter = &mut control_loop_state.restart_counter;

        log::info!(
            "Restart '{}' out of '{}': Failed to create workload: '{}': '{}'",
            restart_counter.current_restart(),
            restart_counter.limit(),
            control_loop_state.instance_name.workload_name(),
            error_msg
        );

        restart_counter.count_restart();

        // [impl->swdd~agent-workload-control-loop-limit-restart-attempts~1]
        if restart_counter.limit_exceeded() {
            log::info!(
                "Abort restarts: reached maximum amount of restarts ('{}')",
                restart_counter.limit()
            );

            // [impl->swdd~agent-workload-control-loop-restart-limit-set-execution-state~1]
            control_loop_state
                .update_state_tx
                .update_workload_state(vec![common::objects::WorkloadState {
                    instance_name: control_loop_state.instance_name.to_owned(),
                    execution_state: ExecutionState::restart_failed_no_retry(),
                    ..Default::default()
                }])
                .await
                .unwrap_or_else(|err| {
                    log::error!(
                        "Failed to update workload state of workload '{}': '{}'",
                        control_loop_state.instance_name.workload_name(),
                        err
                    )
                });
            return control_loop_state;
        }

        let sender = control_loop_state.workload_channel.clone();
        tokio::task::spawn(async move {
            // [impl->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
            tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_WAITING_TIME_MS)).await;
            log::debug!("Send WorkloadCommand::Restart.");

            sender
                .restart(runtime_workload_config, control_interface_path)
                .await
                .unwrap_or_else(|err| {
                    log::info!("Could not send WorkloadCommand::Restart: '{}'", err)
                });
        });
        control_loop_state
    }

    async fn create<WorkloadId, StChecker, Fut>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
        func_on_error: impl FnOnce(
            ControlLoopState<WorkloadId, StChecker>,
            WorkloadSpec,
            Option<PathBuf>,
            String,
        ) -> Fut,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
        Fut: Future<Output = ControlLoopState<WorkloadId, StChecker>>,
    {
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
                log::debug!(
                    "Created workload '{}' successfully.",
                    control_loop_state.instance_name.workload_name()
                );
                control_loop_state.workload_id = Some(new_workload_id);
                control_loop_state.state_checker = Some(new_state_checker);
                control_loop_state
            }
            Err(err) => {
                func_on_error(
                    control_loop_state,
                    runtime_workload_config,
                    control_interface_path,
                    err.to_string(),
                )
                .await
            }
        }
    }

    async fn delete<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) -> Option<ControlLoopState<WorkloadId, StChecker>>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let workload_name = control_loop_state.instance_name.workload_name();
        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                // [impl->swdd~agent-workload-control-loop-delete-failed-allows-retry~1]
                log::warn!("Could not stop workload '{}': '{}'", workload_name, err);
                control_loop_state.workload_id = Some(old_id);

                return Some(control_loop_state);
            } else {
                if let Some(old_checker) = control_loop_state.state_checker.take() {
                    old_checker.stop_checker().await;
                }
                log::debug!("Stop workload complete");

                // Successfully stopped the workload and the state checker. Send a removed on the channel
                control_loop_state
                    .update_state_tx
                    .update_workload_state(vec![common::objects::WorkloadState {
                        instance_name: control_loop_state.instance_name.to_owned(),
                        execution_state: ExecutionState::removed(),
                        workload_id: old_id.to_string(),
                    }])
                    .await
                    .unwrap_or_illegal_state();
            }
        } else {
            // [impl->swdd~agent-workload-control-loop-delete-broken-allowed~1]
            log::debug!("Workload '{}' already gone.", workload_name);

            // TODO: this has to be done in a better way and not repeating the code. The
            // new functionality taking care of this will come with a dedicated PR
            //
            // Successfully stopped the workload and the state checker. Send a removed on the channel
            control_loop_state
                .update_state_tx
                .update_workload_state(vec![common::objects::WorkloadState {
                    instance_name: control_loop_state.instance_name.to_owned(),
                    execution_state: ExecutionState::removed(),
                    ..Default::default() // no id
                }])
                .await
                .unwrap_or_illegal_state();
        }

        None
    }

    async fn update<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let workload_name = control_loop_state.instance_name.workload_name();
        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                // [impl->swdd~agent-workload-control-loop-update-delete-failed-allows-retry~1]
                log::warn!("Could not update workload '{}': '{}'", workload_name, err);
                control_loop_state.workload_id = Some(old_id);
                return control_loop_state;
            } else if let Some(old_checker) = control_loop_state.state_checker.take() {
                old_checker.stop_checker().await;
            }
        } else {
            // [impl->swdd~agent-workload-control-loop-update-broken-allowed~1]
            log::debug!("Workload '{}' already gone.", workload_name);
        }

        // [impl->swdd~agent-workload-control-loop-reset-restart-attempts-on-update~1]
        control_loop_state.restart_counter.reset();

        // [impl->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
        Self::create(
            control_loop_state,
            runtime_workload_config,
            control_interface_path,
            Self::send_restart,
        )
        .await
    }

    async fn restart<WorkloadId, StChecker>(
        control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        runtime_workload_config: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if control_loop_state.instance_name == runtime_workload_config.instance_name()
            && control_loop_state.workload_id.is_none()
        {
            log::debug!("Next restart attempt.");
            Self::create(
                control_loop_state,
                runtime_workload_config,
                control_interface_path,
                Self::send_restart_delayed,
            )
            .await
        } else {
            // [impl->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
            log::debug!("Skip restart workload.");
            control_loop_state
        }
    }

    pub async fn run<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) where
        WorkloadId: ToString + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        loop {
            match control_loop_state.command_receiver.recv().await {
                // [impl->swdd~agent-workload-control-loop-executes-delete~1]
                Some(WorkloadCommand::Delete) => {
                    log::debug!("Received WorkloadCommand::Delete.");

                    if let Some(new_control_loop_state) = Self::delete(control_loop_state).await {
                        control_loop_state = new_control_loop_state;
                    } else {
                        // [impl->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
                        return;
                    }
                }
                // [impl->swdd~agent-workload-control-loop-executes-update~1]
                Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                    control_loop_state.instance_name = runtime_workload_config.instance_name();
                    log::debug!("Received WorkloadCommand::Update.");

                    control_loop_state = Self::update(
                        control_loop_state,
                        *runtime_workload_config,
                        control_interface_path,
                    )
                    .await;

                    log::debug!("Update workload complete");
                }
                // [impl->swdd~agent-workload-control-loop-executes-restart~1]
                Some(WorkloadCommand::Restart(runtime_workload_config, control_interface_path)) => {
                    control_loop_state = Self::restart(
                        control_loop_state,
                        *runtime_workload_config,
                        control_interface_path,
                    )
                    .await;
                }
                // [impl->swdd~agent-workload-control-loop-executes-create~1]
                Some(WorkloadCommand::Create(runtime_workload_config, control_interface_path)) => {
                    control_loop_state = Self::create(
                        control_loop_state,
                        *runtime_workload_config,
                        control_interface_path,
                        Self::send_restart,
                    )
                    .await;
                }
                _ => {
                    log::warn!(
                        "Could not wait for internal stop command for workload '{}'.",
                        control_loop_state.instance_name.workload_name(),
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
        objects::{ExecutionState, WorkloadInstanceName},
        test_utils::generate_test_workload_spec_with_param,
        to_server_interface::ToServer,
    };
    use tokio::{sync::mpsc, time::timeout};

    use crate::{
        runtime_connectors::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker},
        workload::{ControlLoopState, RestartCounter, WorkloadCommandSender, WorkloadControlLoop},
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "workload_id_1";
    const WORKLOAD_ID_2: &str = "workload_id_2";
    const WORKLOAD_ID_3: &str = "workload_id_3";
    const PIPES_LOCATION: &str = "/some/path";
    const OLD_WORKLOAD_ID: &str = "old_workload_id";

    const TEST_EXEC_COMMAND_BUFFER_SIZE: usize = 5;

    // Unfortunately this test also executes a delete of the newly updated workload.
    // We could not avoid this as it is the only possibility to check the internal variables
    // and to properly stop the control loop in the await new command method
    // [utest->swdd~agent-workload-control-loop-executes-update~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

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

        let instance_name = workload_spec.instance_name();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
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
            instance_name,
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(old_mock_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    WORKLOAD_ID,
                    ExecutionState::removed(),
                ),
            ],
        };

        assert_eq!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(ToServer::UpdateWorkloadState(expected_state)))
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-update-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_broken_allowed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also be stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
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
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    WORKLOAD_ID,
                    ExecutionState::removed(),
                ),
            ],
        };

        assert_eq!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(ToServer::UpdateWorkloadState(expected_state)))
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-update-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_delete_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

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
            instance_name,
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(old_mock_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    OLD_WORKLOAD_ID,
                    ExecutionState::removed(),
                ),
            ],
        };

        assert_eq!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(ToServer::UpdateWorkloadState(expected_state)))
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_create_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
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
            instance_name,
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(old_mock_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    "",
                    ExecutionState::removed(),
                ),
            ],
        };

        assert!(matches!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(ToServer::UpdateWorkloadState(workload_state)))
        if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-delete~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

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

        let control_loop_state = ControlLoopState {
            instance_name: workload_spec.instance_name(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(mock_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    OLD_WORKLOAD_ID,
                    ExecutionState::removed(),
                ),
            ],
        };

        assert_eq!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(ToServer::UpdateWorkloadState(expected_state)))
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_failed_allows_retry() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

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

        let control_loop_state = ControlLoopState {
            instance_name: workload_spec.instance_name(),
            workload_id: Some(OLD_WORKLOAD_ID.to_string()),
            state_checker: Some(mock_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    OLD_WORKLOAD_ID,
                    ExecutionState::removed(),
                ),
            ],
        };

        assert_eq!(
            timeout(Duration::from_millis(200), to_server_rx.recv()).await,
            Ok(Some(ToServer::UpdateWorkloadState(expected_state)))
        );

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-delete-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_already_gone() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let runtime_mock = MockRuntimeConnector::new();

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.clone().delete().await.unwrap();

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let control_loop_state = ControlLoopState {
            instance_name: workload_spec.instance_name(),
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~1]
    #[tokio::test]
    async fn utest_workload_obj_run_create_successful() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .create(workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(100),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~1]
    // [utest->swdd~agent-workload-control-loop-restart-workload-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_successful_after_create_command_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Ok((WORKLOAD_ID.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .create(workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(200),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~1]
    // [utest->swdd~agent-workload-control-loop-restart-workload-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_create_with_restart_workload_command_channel_closed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let runtime_expectations = vec![RuntimeCall::CreateWorkload(
            workload_spec.clone(),
            Some(PIPES_LOCATION.into()),
            to_server_tx.clone(),
            Err(crate::runtime_connectors::RuntimeError::Create(
                "some create error".to_string(),
            )),
        )];

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_receiver.close();

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        let new_control_loop_state = WorkloadControlLoop::create(
            control_loop_state,
            workload_spec,
            Some(PIPES_LOCATION.into()),
            WorkloadControlLoop::send_restart,
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // send some randomly selected command
        assert!(new_control_loop_state
            .workload_channel
            .delete()
            .await
            .is_err());
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_successful_after_create_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock
            .expect(vec![
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
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
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
    // [utest->swdd~agent-workload-control-loop-limit-restart-attempts~1]
    // [utest->swdd~agent-workload-control-loop-restart-limit-set-execution-state~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_attempts_exceeded_workload_creation() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, mut to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let mut runtime_expectations = vec![];

        // instead of short vector initialization a for loop is used because RuntimeCall with its submembers shall not be clone-able.
        for _ in 0..super::MAX_RESTARTS {
            runtime_expectations.push(RuntimeCall::CreateWorkload(
                workload_spec.clone(),
                Some(PIPES_LOCATION.into()),
                to_server_tx.clone(),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    "some create error".to_string(),
                )),
            ));
        }

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_sender
            .restart(workload_spec.clone(), Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        // We also send a delete command, but as no new workload was generated, there is also no
        // new ID so no call to the runtime is expected to happen here.
        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        let expected_state = UpdateWorkloadState {
            workload_states: vec![
                common::objects::generate_test_workload_state_with_workload_spec(
                    &workload_spec,
                    "",
                    ExecutionState::restart_failed_no_retry(),
                ),
            ],
        };

        assert!(matches!(to_server_rx.try_recv(),
            Ok(ToServer::UpdateWorkloadState(workload_state))
            if workload_state == expected_state));

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-limit-restart-attempts~1]
    // [utest->swdd~agent-workload-control-loop-restart-limit-set-execution-state~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_attempts_exceeded_workload_state_channel_closed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let runtime_expectations = vec![RuntimeCall::CreateWorkload(
            workload_spec.clone(),
            Some(PIPES_LOCATION.into()),
            to_server_tx.clone(),
            Err(crate::runtime_connectors::RuntimeError::Create(
                "some create error".to_string(),
            )),
        )];

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        let mut restart_counter = RestartCounter::new();
        // Increase the counter until the penultimate restart limit
        for _ in restart_counter.current_restart()..super::MAX_RESTARTS {
            restart_counter.count_restart();
        }

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter,
        };

        // dropping the channel causes the failing send of ToServer message after the restart limit is exceeded.
        drop(to_server_rx);

        // execute last restart => restart limit is exceeded after this last try
        let new_control_loop_state = WorkloadControlLoop::restart(
            control_loop_state,
            workload_spec,
            Some(PIPES_LOCATION.into()),
        )
        .await;

        assert!(new_control_loop_state.update_state_tx.is_closed());
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_workload_command_channel_closed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name = workload_spec.instance_name();

        let runtime_expectations = vec![RuntimeCall::CreateWorkload(
            workload_spec.clone(),
            Some(PIPES_LOCATION.into()),
            to_server_tx.clone(),
            Err(crate::runtime_connectors::RuntimeError::Create(
                "some create error".to_string(),
            )),
        )];

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(runtime_expectations).await;

        workload_command_receiver.close();

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        let new_control_loop_state = WorkloadControlLoop::restart(
            control_loop_state,
            workload_spec,
            Some(PIPES_LOCATION.into()),
        )
        .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // send some randomly selected command
        assert!(new_control_loop_state
            .workload_channel
            .delete()
            .await
            .is_err());
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_stop_restart_commands_on_update_command() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let instance_name = workload_spec.instance_name();

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
                    to_server_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .restart(workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        workload_command_sender
            .update(new_workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: None,
            state_checker: None,
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_on_update_with_create_failure() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let instance_name = workload_spec.instance_name();

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
                    to_server_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // after 1 restart attempt the create with the new workload is successful
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .update(new_workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(125)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: Some(WORKLOAD_ID.into()),
            state_checker: Some(old_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
    // [utest->swdd~agent-workload-control-loop-reset-restart-attempts-on-update~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_reset_restart_counter_on_update() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let instance_name = workload_spec.instance_name();

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
                    to_server_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // after 1 restart attempt the create with the new workload is successful
                RuntimeCall::CreateWorkload(
                    new_workload_spec.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Ok((WORKLOAD_ID_2.to_string(), new_mock_state_checker)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_2.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .update(new_workload_spec, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(125)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut restart_counter = RestartCounter::new();
        // simulate an already incremented restart counter due to restart attempts on initial workload creation
        restart_counter.count_restart();
        restart_counter.count_restart();
        assert_eq!(restart_counter.current_restart(), 3);

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: Some(WORKLOAD_ID.into()),
            state_checker: Some(old_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter,
        };

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }

    // [utest->swdd~agent-workload-control-loop-executes-restart~1]
    // [utest->swdd~agent-workload-control-loop-request-restarts-on-failing-restart-attempt~1]
    // [utest->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
    #[tokio::test]
    async fn utest_workload_obj_run_restart_create_correct_workload_on_two_updates() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (to_server_tx, _to_server_rx) = mpsc::channel(TEST_EXEC_COMMAND_BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let instance_name = workload_spec.instance_name();

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
                    to_server_tx.clone(),
                    Err(crate::runtime_connectors::RuntimeError::Create(
                        "some create error".to_string(),
                    )),
                ),
                // next the second update is executed and shall be successful
                // no delete expected because no workload was created within update1
                RuntimeCall::CreateWorkload(
                    new_workload_spec_update2.clone(),
                    Some(PIPES_LOCATION.into()),
                    to_server_tx.clone(),
                    Ok((WORKLOAD_ID_3.to_string(), new_mock_state_checker_update2)),
                ),
                // Since we also send a delete command to exit the control loop properly, the new workload
                // will also be deleted. This also tests if the new workload id was properly stored.
                RuntimeCall::DeleteWorkload(WORKLOAD_ID_3.to_string(), Ok(())),
            ])
            .await;

        workload_command_sender
            .update(new_workload_spec_update1, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        workload_command_sender
            .update(new_workload_spec_update2, Some(PIPES_LOCATION.into()))
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(125)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let control_loop_state = ControlLoopState {
            instance_name,
            workload_id: Some(WORKLOAD_ID.into()),
            state_checker: Some(old_state_checker),
            update_state_tx: to_server_tx.clone(),
            runtime: Box::new(runtime_mock.clone()),
            command_receiver: workload_command_receiver,
            workload_channel: workload_command_sender,
            restart_counter: RestartCounter::new(),
        };

        assert!(timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state)
        )
        .await
        .is_ok());

        runtime_mock.assert_all_expectations().await;
    }
}
