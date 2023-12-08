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
use tokio::sync::mpsc;

#[cfg(test)]
use mockall::automock;

const MAX_RETIRES: usize = 20;
const RETRY_WAITING_TIME_MS: u64 = 1000;

pub struct WorkloadControlLoop<WorkloadId, StChecker>
where
    WorkloadId: Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    workload_name: String,
    agent_name: String,
    workload_id: Option<WorkloadId>,
    state_checker: Option<StChecker>,
    update_state_tx: StateChangeSender,
    runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>,
    command_receiver: mpsc::Receiver<WorkloadCommand>,
    workload_channel: WorkloadCommandChannel,
}

#[cfg_attr(test, automock)]
impl<WorkloadId, StChecker> WorkloadControlLoop<WorkloadId, StChecker>
where
    WorkloadId: Send + Sync + 'static,
    StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
{
    pub fn new(
        workload_name: String,
        agent_name: String,
        workload_id: Option<WorkloadId>,
        state_checker: Option<StChecker>,
        update_state_tx: StateChangeSender,
        runtime: Box<dyn RuntimeConnector<WorkloadId, StChecker>>,
        command_receiver: mpsc::Receiver<WorkloadCommand>,
        workload_channel: WorkloadCommandChannel,
    ) -> Self {
        WorkloadControlLoop {
            workload_name,
            agent_name,
            workload_id,
            state_checker,
            update_state_tx,
            runtime,
            command_receiver,
            workload_channel,
        }
    }

    pub async fn await_new_command(&mut self) {
        let mut retry_counter: usize = 1;
        let mut quit_retry = false;
        loop {
            match self.command_receiver.recv().await {
                // [impl->swdd~agent-workload-tasks-executes-delete~1]
                Some(WorkloadCommand::Delete) => {
                    quit_retry = true;
                    if let Some(old_id) = self.workload_id.take() {
                        if let Err(err) = self.runtime.delete_workload(&old_id).await {
                            // [impl->swdd~agent-workload-task-delete-failed-allows-retry~1]
                            log::warn!(
                                "Could not stop workload '{}': '{}'",
                                self.workload_name,
                                err
                            );
                            self.workload_id = Some(old_id);
                            continue;
                        } else {
                            if let Some(old_checker) = self.state_checker.take() {
                                old_checker.stop_checker().await;
                            }
                            log::debug!("Stop workload complete");
                        }
                    } else {
                        // [impl->swdd~agent-workload-task-delete-broken-allowed~1]
                        log::debug!("Workload '{}' already gone.", self.workload_name);
                    }

                    // Successfully stopped the workload and the state checker. Send a removed on the channel
                    self.update_state_tx
                        .update_workload_state(vec![common::objects::WorkloadState {
                            agent_name: self.agent_name.clone(),
                            workload_name: self.workload_name.clone(),
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

                    if let Some(old_id) = self.workload_id.take() {
                        if let Err(err) = self.runtime.delete_workload(&old_id).await {
                            // [impl->swdd~agent-workload-task-update-delete-failed-allows-retry~1]
                            log::warn!(
                                "Could not update workload '{}': '{}'",
                                self.workload_name,
                                err
                            );
                            self.workload_id = Some(old_id);
                            continue;
                        } else if let Some(old_checker) = self.state_checker.take() {
                            old_checker.stop_checker().await;
                        }
                    } else {
                        // [impl->swdd~agent-workload-task-update-broken-allowed~1]
                        log::debug!("Workload '{}' already gone.", self.workload_name);
                    }

                    match self
                        .runtime
                        .create_workload(
                            *runtime_workload_config,
                            control_interface_path,
                            self.update_state_tx.clone(),
                        )
                        .await
                    {
                        Ok((new_workload_id, new_state_checker)) => {
                            self.workload_id = Some(new_workload_id);
                            self.state_checker = Some(new_state_checker);
                        }
                        Err(err) => {
                            // [impl->swdd~agent-workload-task-update-create-failed-allows-retry~1]
                            log::warn!(
                                "Could not start updated workload '{}': '{}'",
                                self.workload_name,
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

                    match self
                        .runtime
                        .create_workload(
                            *runtime_workload_config.clone(),
                            control_interface_path.clone(),
                            self.update_state_tx.clone(),
                        )
                        .await
                    {
                        Ok((new_workload_id, new_state_checker)) => {
                            self.workload_id = Some(new_workload_id);
                            self.state_checker = Some(new_state_checker);
                        }
                        Err(err) => {
                            log::warn!(
                                "Retry '{}' out of '{}': Failed to create workload: '{}': '{}'",
                                retry_counter,
                                MAX_RETIRES,
                                self.workload_name,
                                err
                            );

                            log::debug!(
                                "#### Enqueue restart workload command, quit_retry = {quit_retry}."
                            );

                            let sender = self.workload_channel.clone();
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
                        self.workload_name,
                    );
                    return;
                }
            }
        }
    }
}
