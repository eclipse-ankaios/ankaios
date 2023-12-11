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
    objects::ExecutionState,
    state_change_interface::{StateChangeInterface, StateChangeSender},
    std_extensions::IllegalStateResult,
};

#[cfg(test)]
use mockall::automock;

use super::workload_command_channel::WorkloadCommandReceiver;

const MAX_RETIRES: usize = 20;
const RETRY_WAITING_TIME_MS: u64 = 1000;

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
}

pub struct WorkloadControlLoop;

#[cfg_attr(test, automock)]
impl WorkloadControlLoop {
    pub async fn await_new_command<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
    ) where
        WorkloadId: Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let mut retry_counter: usize = 1;
        let mut quit_retry = false;
        loop {
            match control_loop_state.command_receiver.recv().await {
                // [impl->swdd~agent-workload-tasks-executes-delete~1]
                Some(WorkloadCommand::Delete) => {
                    quit_retry = true;
                    if let Some(old_id) = control_loop_state.workload_id.take() {
                        if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await
                        {
                            // [impl->swdd~agent-workload-task-delete-failed-allows-retry~1]
                            log::warn!(
                                "Could not stop workload '{}': '{}'",
                                control_loop_state.workload_name,
                                err
                            );
                            control_loop_state.workload_id = Some(old_id);
                            continue;
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

                    return;
                }
                // [impl->swdd~agent-workload-task-executes-update~1]
                Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                    quit_retry = true;
                    log::debug!("###################### Setting quit_retry = true");

                    if let Some(old_id) = control_loop_state.workload_id.take() {
                        if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await
                        {
                            // [impl->swdd~agent-workload-task-update-delete-failed-allows-retry~1]
                            log::warn!(
                                "Could not update workload '{}': '{}'",
                                control_loop_state.workload_name,
                                err
                            );
                            control_loop_state.workload_id = Some(old_id);
                            continue;
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
                            *runtime_workload_config,
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

                    log::debug!("Update workload complete");
                }
                Some(WorkloadCommand::Restart(runtime_workload_config, control_interface_path)) => {
                    if retry_counter > MAX_RETIRES {
                        log::warn!(
                            "Abort retry: maximum amount of retries ('{}') reached.",
                            MAX_RETIRES
                        );
                        continue;
                    }

                    if quit_retry {
                        log::debug!("Skip restart workload, quit_retry = '{}'", quit_retry);
                        continue;
                    }

                    match control_loop_state
                        .runtime
                        .create_workload(
                            *runtime_workload_config.clone(),
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
                                "Retry '{}' out of '{}': Failed to create workload: '{}': '{}'",
                                retry_counter,
                                MAX_RETIRES,
                                control_loop_state.workload_name,
                                err
                            );

                            log::debug!(
                                "#### Enqueue restart workload command, quit_retry = {quit_retry}."
                            );

                            let sender = control_loop_state.workload_channel.clone();
                            retry_counter += 1;
                            tokio::task::spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_millis(
                                    RETRY_WAITING_TIME_MS,
                                ))
                                .await;
                                sender
                                    .restart(*runtime_workload_config, control_interface_path)
                                    .await
                                    .unwrap_or_else(|err| {
                                        log::warn!(
                                            "Could not send restart workload command: '{}'",
                                            err
                                        )
                                    });
                            });
                        }
                    }
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
