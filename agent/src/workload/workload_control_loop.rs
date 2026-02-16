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

use crate::control_interface::ControlInterfacePath;
use crate::io_utils::FileSystemError;
use crate::runtime_connectors::log_fetcher::LogFetcher;
use crate::runtime_connectors::{LogRequestOptions, RuntimeError, StateChecker};
use crate::workload::{ControlLoopState, WorkloadCommand};
use crate::workload_files::WorkloadFilesBasePath;
use crate::workload_state::{WorkloadStateSender, WorkloadStateSenderInterface};

use ankaios_api::ank_base::{
    ExecutionStateSpec, RestartPolicy, WorkloadInstanceNameSpec, WorkloadNamed,
};
use common::std_extensions::IllegalStateResult;

use futures_util::Future;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::filesystem_async;

#[cfg_attr(test, mockall_double::double)]
use crate::workload_files::WorkloadFilesCreator;

#[cfg_attr(test, mockall_double::double)]
use super::retry_manager::RetryToken;

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
                        control_loop_state.workload_named.instance_name.workload_name());

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
                        if Self::restart_policy_matches_execution_state(&control_loop_state.workload_named.workload.restart_policy, &new_workload_state.execution_state) {
                            // [impl->swdd~workload-control-loop-handles-workload-restarts~2]
                            control_loop_state = Self::restart_workload_on_runtime(control_loop_state, &new_workload_state.execution_state).await;
                        }
                    }

                    log::trace!("Restart handling done.");
                }
                workload_command = control_loop_state.command_receiver.recv() => {
                    match workload_command {
                        // [impl->swdd~agent-workload-control-loop-executes-delete~3]
                        Some(WorkloadCommand::Delete) => {
                            log::debug!("Received WorkloadCommand::Delete.");

                            // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
                            control_loop_state.retry_manager.invalidate();

                            if let Some(new_control_loop_state) = Self::delete_workload_on_runtime(control_loop_state).await {
                                control_loop_state = new_control_loop_state;
                            } else {
                                return;
                            }
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-update~3]
                        Some(WorkloadCommand::Update(runtime_workload_config, control_interface_path)) => {
                            log::debug!("Received WorkloadCommand::Update.");

                            // [impl->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
                            control_loop_state.retry_manager.invalidate();

                            control_loop_state = Self::update_workload_on_runtime(
                                control_loop_state,
                                runtime_workload_config,
                                control_interface_path,
                            )
                            .await;

                            log::debug!("Update workload complete");
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-retry~1]
                        Some(WorkloadCommand::Retry(_instance_name, retry_token)) => {
                            log::debug!("Received WorkloadCommand::Retry.");

                            control_loop_state = Self::retry_create_workload_on_runtime(
                                control_loop_state,
                                retry_token
                            )
                            .await;
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-create~4]
                        Some(WorkloadCommand::Create) => {
                            log::debug!("Received WorkloadCommand::Create.");

                            Self::send_workload_state_to_agent(
                                &control_loop_state.to_agent_workload_state_sender,
                                control_loop_state.instance_name(),
                                ExecutionStateSpec::starting_triggered(),
                            )
                            .await;

                            let retry_token = control_loop_state.retry_manager.new_token();

                            control_loop_state = Self::create_workload_on_runtime(
                                control_loop_state,
                                retry_token,
                                Self::send_retry_for_workload,
                            )
                            .await;
                        }
                        // [impl->swdd~agent-workload-control-loop-executes-resume~1]
                        Some(WorkloadCommand::Resume) => {
                            log::debug!("Received WorkloadCommand::Resume.");

                            control_loop_state = Self::resume_workload_on_runtime(control_loop_state).await;
                        }
                        Some(WorkloadCommand::StartLogFetcher(log_request_options, result_sink)) =>  {
                            match Self::create_log_fetcher(&control_loop_state, &log_request_options) {
                                Ok(logger) => {if let Err(error) = result_sink.send(logger){
                                    log::warn!("Could not return log fetcher: '{error:?}'");
                                }},
                                Err(error) => {
                                    log::warn!("Could not start log fetcher: '{error:?}'");
                                }
                            }
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
        instance_name: &WorkloadInstanceNameSpec,
        execution_state: ExecutionStateSpec,
    ) {
        workload_state_sender
            .report_workload_execution_state(instance_name, execution_state)
            .await;
    }

    async fn restart_workload_on_runtime<WorkloadId, StChecker>(
        control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        execution_state: &ExecutionStateSpec,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        log::debug!(
            "Restart workload '{}' with restart policy '{:?}'",
            control_loop_state
                .workload_named
                .instance_name
                .workload_name(),
            control_loop_state.workload_named.workload.restart_policy,
        );

        let workload_named = control_loop_state.workload_named.clone();
        let control_interface_path = control_loop_state.control_interface_path.clone();

        // [impl->swdd~workload-control-loop-reuses-bundle-on-successful-restart~1]
        if execution_state.is_succeeded() && control_loop_state.workload_id.is_some() {
            Self::restart_workload_with_bundle_reuse(
                control_loop_state,
                Some(Box::new(workload_named)),
                control_interface_path,
            )
            .await
        } else {
            // update the workload with its existing config since a restart is represented by an update operation
            // [impl->swdd~workload-control-loop-restarts-workloads-using-update~1]
            Self::update_workload_on_runtime(
                control_loop_state,
                Some(Box::new(workload_named)),
                control_interface_path,
            )
            .await
        }
    }

    fn is_same_workload(
        lhs_instance_name: &WorkloadInstanceNameSpec,
        rhs_instance_name: &WorkloadInstanceNameSpec,
    ) -> bool {
        lhs_instance_name.eq(rhs_instance_name)
    }

    fn restart_policy_matches_execution_state(
        restart_policy: &RestartPolicy,
        execution_state: &ExecutionStateSpec,
    ) -> bool {
        match restart_policy {
            RestartPolicy::Never => false,
            RestartPolicy::OnFailure => execution_state.is_failed(),
            RestartPolicy::Always => execution_state.is_failed() || execution_state.is_succeeded(),
        }
    }

    async fn send_retry_for_workload<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        instance_name: WorkloadInstanceNameSpec,
        retry_token: RetryToken,
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

        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            &instance_name,
            ExecutionStateSpec::retry_starting(retry_token.counter() + 1, error_msg),
        )
        .await;

        // [impl->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
        control_loop_state
            .retry_sender
            .retry(instance_name, retry_token)
            .await
            .unwrap_or_else(|err| log::info!("Could not send WorkloadCommand::Retry: '{err}'"));
        control_loop_state
    }

    // [impl->swdd~agent-workload-control-loop-executes-create~4]
    async fn create_workload_on_runtime<WorkloadId, StChecker, ErrorFunc, Fut>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        retry_token: RetryToken,
        func_on_recoverable_error: ErrorFunc,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
        Fut: Future<Output = ControlLoopState<WorkloadId, StChecker>> + 'static,
        ErrorFunc: FnOnce(
                ControlLoopState<WorkloadId, StChecker>,
                WorkloadInstanceNameSpec,
                RetryToken,
                String,
            ) -> Fut
            + 'static,
    {
        let host_file_path_mount_point_mappings =
            match Self::handle_mount_point_creation(&control_loop_state).await {
                Ok(mapping) => mapping,
                Err(err) => {
                    log::error!(
                        "Failed to create workload files for workload '{}': '{}'",
                        control_loop_state.instance_name(),
                        err
                    );
                    return control_loop_state;
                }
            };

        let new_instance_name = control_loop_state.workload_named.instance_name.clone();

        match control_loop_state
            .runtime
            .create_workload(
                control_loop_state.workload_named.clone(),
                control_loop_state.workload_id.clone(),
                control_loop_state
                    .control_interface_path
                    .as_ref()
                    .map(|path| path.to_path_buf()),
                control_loop_state
                    .state_checker_workload_state_sender
                    .clone(),
                host_file_path_mount_point_mappings,
            )
            .await
        {
            Ok((new_workload_id, new_state_checker)) => {
                log::info!(
                    "Successfully created workload '{}'.",
                    new_instance_name.workload_name()
                );

                // [impl->swdd~agent-workload-control-loop-updates-internal-state~1]
                control_loop_state.workload_id = Some(new_workload_id);
                control_loop_state.state_checker = Some(new_state_checker);
                control_loop_state
            }
            Err(err) => {
                // [impl->swdd~agent-workload-control-loop-handles-failed-workload-creation~1]

                Self::delete_folder(&WorkloadFilesBasePath::from((
                    &control_loop_state.run_folder,
                    &new_instance_name,
                )))
                .await;

                match &err {
                    RuntimeError::Unsupported(msg) => {
                        Self::send_workload_state_to_agent(
                            &control_loop_state.to_agent_workload_state_sender,
                            &new_instance_name,
                            ExecutionStateSpec::starting_failed(msg.to_string()),
                        )
                        .await;

                        log::error!("Failed to create workload with error: '{err}'");

                        control_loop_state
                    }
                    _ => {
                        func_on_recoverable_error(
                            control_loop_state,
                            new_instance_name,
                            retry_token,
                            err.to_string(),
                        )
                        .await
                    }
                }
            }
        }
    }

    async fn handle_mount_point_creation<WorkloadId, StChecker>(
        control_loop_state: &ControlLoopState<WorkloadId, StChecker>,
    ) -> Result<HashMap<PathBuf, PathBuf>, String>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if !control_loop_state
            .workload_named
            .workload
            .files
            .files
            .is_empty()
        {
            let workload_files_base_path = WorkloadFilesBasePath::from((
                &control_loop_state.run_folder,
                control_loop_state.instance_name(),
            ));

            match WorkloadFilesCreator::create_files(
                &workload_files_base_path,
                &control_loop_state.workload_named.workload.files.files,
            )
            .await
            {
                Ok(mapping) => Ok(mapping),
                Err(err) => {
                    // [impl->swdd~agent-workload-control-loop-aborts-create-upon-workload-files-creation-error~1]
                    filesystem_async::remove_dir_all(&workload_files_base_path)
                        .await
                        .unwrap_or_else(|err| {
                            log::error!(
                                "Failed to remove workload files base folder after failed creation '{}': '{}'",
                                workload_files_base_path.display(),
                                err
                            )
                        });

                    Self::send_workload_state_to_agent(
                        &control_loop_state.to_agent_workload_state_sender,
                        control_loop_state.instance_name(),
                        ExecutionStateSpec::starting_failed(&err),
                    )
                    .await;

                    Err(err.to_string())
                }
            }
        } else {
            Ok(HashMap::new())
        }
    }

    // [impl->swdd~agent-workload-control-loop-executes-delete~3]
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
            ExecutionStateSpec::stopping_requested(),
        )
        .await;

        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                Self::send_workload_state_to_agent(
                    &control_loop_state.to_agent_workload_state_sender,
                    control_loop_state.instance_name(),
                    ExecutionStateSpec::delete_failed(err.to_string()),
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

        let workload_dir = control_loop_state
            .instance_name()
            .pipes_folder_name(&control_loop_state.run_folder);
        Self::delete_folder(&workload_dir).await;

        // Successfully stopped the workload. Send a removed on the channel
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionStateSpec::removed(),
        )
        .await;

        None
    }

    // [impl->swdd~agent-workload-control-loop-executes-update~3]
    async fn update_workload_on_runtime<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        new_workload_named: Option<Box<WorkloadNamed>>,
        control_interface_path: Option<ControlInterfacePath>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionStateSpec::stopping_requested(),
        )
        .await;

        if let Some(old_id) = control_loop_state.workload_id.take() {
            if let Err(err) = control_loop_state.runtime.delete_workload(&old_id).await {
                Self::send_workload_state_to_agent(
                    &control_loop_state.to_agent_workload_state_sender,
                    control_loop_state.instance_name(),
                    ExecutionStateSpec::delete_failed(err.to_string()),
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

        log::debug!(
            "Deleting the workload files of workload '{}'",
            control_loop_state.instance_name()
        );
        Self::delete_folder(&WorkloadFilesBasePath::from((
            &control_loop_state.run_folder,
            control_loop_state.instance_name(),
        )))
        .await;

        let new_workload_named = if let Some(new_named) = new_workload_named {
            if !Self::is_same_workload(control_loop_state.instance_name(), &new_named.instance_name)
            {
                let workload_dir = control_loop_state
                    .instance_name()
                    .pipes_folder_name(&control_loop_state.run_folder);
                Self::delete_folder(&workload_dir).await;
            }
            Some(new_named)
        } else {
            None
        };

        // workload is deleted or already gone, send the remove state
        Self::send_workload_state_to_agent(
            &control_loop_state.to_agent_workload_state_sender,
            control_loop_state.instance_name(),
            ExecutionStateSpec::removed(),
        )
        .await;

        // [impl->swdd~agent-workload-control-loop-executes-update-delete-only~1]
        if let Some(named) = new_workload_named {
            // [impl->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
            control_loop_state.workload_named = *named;
            control_loop_state.control_interface_path = control_interface_path;

            Self::send_workload_state_to_agent(
                &control_loop_state.to_agent_workload_state_sender,
                control_loop_state.instance_name(),
                ExecutionStateSpec::starting_triggered(),
            )
            .await;

            let retry_token = control_loop_state.retry_manager.new_token();

            control_loop_state = Self::create_workload_on_runtime(
                control_loop_state,
                retry_token,
                Self::send_retry_for_workload,
            )
            .await;
        }
        control_loop_state
    }

    // [impl->swdd~workload-control-loop-reuses-bundle-on-successful-restart~1]
    async fn restart_workload_with_bundle_reuse<WorkloadId, StChecker>(
        mut control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        new_workload_named: Option<Box<WorkloadNamed>>,
        control_interface_path: Option<ControlInterfacePath>,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        log::debug!(
            "Reusing bundle and workload files for workload '{}' due to restart policy",
            control_loop_state.instance_name().workload_name()
        );
        if let Some(old_checker) = control_loop_state.state_checker.take() {
            old_checker.stop_checker().await;
        }

        let new_workload_named = if let Some(new_named) = new_workload_named {
            if !Self::is_same_workload(control_loop_state.instance_name(), &new_named.instance_name)
            {
                let workload_dir = control_loop_state
                    .instance_name()
                    .pipes_folder_name(&control_loop_state.run_folder);
                Self::delete_folder(&workload_dir).await;
            }
            Some(new_named)
        } else {
            None
        };

        // [impl->swdd~agent-workload-control-loop-executes-update-delete-only~1]
        if let Some(named) = new_workload_named {
            // [impl->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
            control_loop_state.workload_named = *named;
            control_loop_state.control_interface_path = control_interface_path;

            Self::send_workload_state_to_agent(
                &control_loop_state.to_agent_workload_state_sender,
                control_loop_state.instance_name(),
                ExecutionStateSpec::starting_triggered(),
            )
            .await;

            let retry_token = control_loop_state.retry_manager.new_token();

            control_loop_state = Self::create_workload_on_runtime(
                control_loop_state,
                retry_token,
                Self::send_retry_for_workload,
            )
            .await;
        }
        control_loop_state
    }

    async fn retry_create_workload_on_runtime<WorkloadId, StChecker>(
        control_loop_state: ControlLoopState<WorkloadId, StChecker>,
        retry_token: RetryToken,
    ) -> ControlLoopState<WorkloadId, StChecker>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        if retry_token.is_valid() {
            Self::create_workload_on_runtime(
                control_loop_state,
                retry_token,
                Self::send_retry_for_workload,
            )
            .await
        } else {
            log::debug!("Ignore outdated retry command");
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
            .get_workload_id(&control_loop_state.workload_named.instance_name)
            .await;

        let state_checker: Option<StChecker> = match workload_id.as_ref() {
            Ok(wl_id) => control_loop_state
                .runtime
                .start_checker(
                    wl_id,
                    control_loop_state.workload_named.clone(),
                    control_loop_state
                        .state_checker_workload_state_sender
                        .clone(),
                )
                .await
                .map_err(|err| {
                    log::warn!(
                        "Failed to start state checker when resuming workload '{workload_name}': '{err}'"
                    );
                    err
                })
                .ok(),
            Err(err) => {
                log::warn!(
                    "Failed to get workload id when resuming workload '{workload_name}': '{err}'"
                );
                None
            }
        };

        // assign the workload id and state checker to the control loop state
        control_loop_state.workload_id = workload_id.ok();
        control_loop_state.state_checker = state_checker;
        control_loop_state
    }

    async fn delete_folder(path: &Path) {
        filesystem_async::remove_dir_all(path)
            .await
            .unwrap_or_else(|err| match err {
                FileSystemError::NotFoundDirectory(_) => {}
                _ => log::warn!("Failed to delete folder: '{err}'"),
            });
    }

    // [impl->swdd~agent-workload-control-loop-creates-log-fetcher~1]
    fn create_log_fetcher<WorkloadId, StChecker>(
        control_loop_state: &ControlLoopState<WorkloadId, StChecker>,
        log_request_options: &LogRequestOptions,
    ) -> Result<Box<dyn LogFetcher + Send>, RuntimeError>
    where
        WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
        StChecker: StateChecker<WorkloadId> + Send + Sync + 'static,
    {
        let Some(workload_id) = &control_loop_state.workload_id else {
            return Err(RuntimeError::CollectLog(
                "Could not start collecting logs for as it has no workload ID yet.".into(),
            ));
        };

        control_loop_state
            .runtime
            .get_log_fetcher(workload_id.clone(), log_request_options)
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
    use super::{ControlInterfacePath, WorkloadControlLoop};
    use crate::io_utils::mock_filesystem_async;
    use crate::runtime_connectors::test::{MockRuntimeConnector, RuntimeCall, StubStateChecker};
    use crate::runtime_connectors::{LogRequestOptions, RuntimeError, log_fetcher::MockLogFetcher};
    use crate::workload::{
        ControlLoopState, WorkloadCommand, retry_manager::MockRetryToken,
        workload_command_channel::WorkloadCommandSender,
    };
    use crate::workload_files::{
        MockWorkloadFilesCreator, WorkloadFileCreationError, WorkloadFilesBasePath,
    };
    use crate::workload_state::{WorkloadStateSenderInterface, assert_execution_state_sequence};

    use ankaios_api::ank_base::{
        ExecutionStateEnumSpec, ExecutionStateSpec, Pending, RestartPolicy,
        WorkloadInstanceNameSpec,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_workload_named, generate_test_workload_state_with_workload_named,
    };

    use mockall::{lazy_static, predicate};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::sync::oneshot;
    use tokio::{sync::mpsc, time::timeout};

    const OLD_WORKLOAD_ID: &str = "old_workload_id";

    lazy_static! {
        pub static ref CONTROL_INTERFACE_PATH: Option<ControlInterfacePath> = Some(
            ControlInterfacePath::new(PathBuf::from(fixtures::PIPES_LOCATION))
        );
    }

    // Unfortunately this test also executes a delete of the newly updated workload.
    // We could not avoid this as it is the only possibility to check the internal variables
    // and to properly stop the control loop in the await new command method
    // [utest->swdd~agent-workload-control-loop-executes-update~3]
    #[tokio::test]
    async fn utest_workload_obj_run_update_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also we stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut old_workload = generate_test_workload_named();
        old_workload.workload.files = Default::default();

        let mut new_workload = old_workload.clone();
        new_workload.workload.runtime_config = "changed config".to_string();
        new_workload.instance_name = WorkloadInstanceNameSpec::builder()
            .agent_name(old_workload.instance_name.agent_name())
            .workload_name(old_workload.instance_name.workload_name())
            .config(&new_workload.workload.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            RuntimeCall::CreateWorkload(
                new_workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[0].to_string(),
                    new_mock_state_checker,
                )),
            ),
            // Since we also send a delete command to exit the control loop properly, the new workload
            // will also be deleted. This also tests if the new workload id was properly stored.
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(Some(new_workload.clone()), CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.delete().await.unwrap();

        let old_instance_name = old_workload.instance_name.clone();
        let new_instance_name = new_workload.instance_name.clone();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(old_workload)
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        control_loop_state
            .retry_manager
            .expect_invalidate()
            .times(2)
            .return_const(());

        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .return_once(|| mock_retry_token);

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (&old_instance_name, ExecutionStateSpec::removed()),
                (&new_instance_name, ExecutionStateSpec::starting_triggered()),
                (&new_instance_name, ExecutionStateSpec::stopping_requested()),
                (&new_instance_name, ExecutionStateSpec::removed()),
            ],
        )
        .await;

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-update-delete-only~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_delete_only() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let old_workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            // The workload was already deleted with the previous runtime call delete.
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send only the update to delete the workload
        workload_command_sender
            .update(None, CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();

        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.delete().await.unwrap();

        let old_instance_name = old_workload.instance_name.clone();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(old_workload)
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        control_loop_state
            .retry_manager
            .expect_invalidate()
            .times(2)
            .return_const(());

        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .return_once(|| mock_retry_token);

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (&old_instance_name, ExecutionStateSpec::removed()),
            ],
        )
        .await;

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-update-delete-only~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_after_update_delete_only() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also we stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut old_workload = generate_test_workload_named();
        old_workload.workload.files = Default::default();

        let mut new_workload = old_workload.clone();
        new_workload.workload.runtime_config = "changed config".to_string();
        new_workload.instance_name = WorkloadInstanceNameSpec::builder()
            .agent_name(old_workload.instance_name.agent_name())
            .workload_name(old_workload.instance_name.workload_name())
            .config(&new_workload.workload.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            RuntimeCall::CreateWorkload(
                new_workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[0].to_string(),
                    new_mock_state_checker,
                )),
            ),
            // Delete the new updated workload to exit the infinite loop
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send the update delete only
        workload_command_sender
            .update(None, CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();

        // Send the update
        workload_command_sender
            .update(Some(new_workload.clone()), CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();

        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.delete().await.unwrap();

        let old_instance_name = old_workload.instance_name.clone();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(old_workload)
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        control_loop_state
            .retry_manager
            .expect_invalidate()
            .times(3)
            .return_const(());

        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .return_once(|| mock_retry_token);

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (&old_instance_name, ExecutionStateSpec::removed()),
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (&old_instance_name, ExecutionStateSpec::removed()),
            ],
        )
        .await;

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-update-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_broken_allowed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        // Since we also send a delete command to exit the control loop properly, the new state
        // checker will also be stopped. This also tests if the new state checker was properly stored.
        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut old_workload = generate_test_workload_named();
        old_workload.workload.files = Default::default();

        let mut new_workload = old_workload.clone();
        new_workload.workload.runtime_config = "changed config".to_string();
        new_workload.instance_name = WorkloadInstanceNameSpec::builder()
            .agent_name(old_workload.instance_name.agent_name())
            .workload_name(old_workload.instance_name.workload_name())
            .config(&new_workload.workload.runtime_config)
            .build();
        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::CreateWorkload(
                new_workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[0].to_string(),
                    new_mock_state_checker,
                )),
            ),
            // Since we also send a delete command to exit the control loop properly, the new workload
            // will also be deleted. This also tests if the new workload id was properly stored.
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(Some(new_workload.clone()), CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.delete().await.unwrap();

        let old_instance_name = old_workload.instance_name.clone();
        let new_instance_name = new_workload.instance_name.clone();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(old_workload)
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        control_loop_state
            .retry_manager
            .expect_invalidate()
            .times(2)
            .return_const(());

        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .return_once(|| mock_retry_token);

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (&old_instance_name, ExecutionStateSpec::removed()),
                (&new_instance_name, ExecutionStateSpec::starting_triggered()),
                (&new_instance_name, ExecutionStateSpec::stopping_requested()),
                (&new_instance_name, ExecutionStateSpec::removed()),
            ],
        )
        .await;

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-update-delete-failed-allows-retry~1]
    // [utest->swdd~agent-workload-control-loop-delete-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_update_delete_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let old_workload = generate_test_workload_named();

        let mut new_workload = old_workload.clone();
        new_workload.workload.runtime_config = "changed config".to_string();
        new_workload.instance_name = WorkloadInstanceNameSpec::builder()
            .agent_name(old_workload.instance_name.agent_name())
            .workload_name(old_workload.instance_name.workload_name())
            .config(&new_workload.workload.runtime_config)
            .build();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::DeleteWorkload(
                OLD_WORKLOAD_ID.to_string(),
                Err(RuntimeError::Delete("some delete error".into())),
            ),
            // Since we also send a delete command to exit the control loop properly, the new workload
            // will also be deleted. This also tests if the new workload id was properly stored.
            RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send the update command now. It will be buffered until the await receives it.
        workload_command_sender
            .update(Some(new_workload.clone()), CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();
        // Send also a delete command so that we can properly get out of the loop
        workload_command_sender.delete().await.unwrap();

        let old_instance_name = old_workload.instance_name.clone();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(old_workload)
            .workload_id(Some(OLD_WORKLOAD_ID.into()))
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        control_loop_state
            .retry_manager
            .expect_invalidate()
            .times(2)
            .return_const(());

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (
                    &old_instance_name,
                    ExecutionStateSpec::delete_failed("some delete error"),
                ),
                (&old_instance_name, ExecutionStateSpec::stopping_requested()),
                (&old_instance_name, ExecutionStateSpec::removed()),
            ],
        )
        .await;

        assert!(workload_command_receiver2.is_empty());
        assert!(workload_command_receiver2.is_closed());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-delete~3]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut mock_state_checker = StubStateChecker::new();
        mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::DeleteWorkload(
            OLD_WORKLOAD_ID.to_string(),
            Ok(()),
        )]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.delete().await.unwrap();

        let workload = generate_test_workload_named();

        let instance_name = workload.instance_name.clone();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(mock_state_checker);

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_execution_state_sequence(
            state_change_rx,
            vec![
                (&instance_name, ExecutionStateSpec::stopping_requested()),
                (&instance_name, ExecutionStateSpec::removed()),
            ],
        )
        .await;

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-delete-broken-allowed~1]
    #[tokio::test]
    async fn utest_workload_obj_run_delete_already_gone() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let runtime_mock = MockRuntimeConnector::new();

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        // Send the delete command now. It will be buffered until the await receives it.
        workload_command_sender.delete().await.unwrap();

        let workload = generate_test_workload_named();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        assert!(
            timeout(
                Duration::from_millis(200),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    // [utest->swdd~agent-workload-control-loop-updates-internal-state~1]
    #[tokio::test]
    async fn utest_workload_obj_run_create_successful() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut workload = generate_test_workload_named();
        workload.workload.files = Default::default();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::CreateWorkload(
                workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[0].to_string(),
                    new_mock_state_checker,
                )),
            ),
            // Since we also send a delete command to exit the control loop properly, the new workload
            // will also be deleted. This also tests if the new workload id was properly stored.
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        workload_command_sender.create().await.unwrap();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            workload_command_sender.delete().await.unwrap();
        });

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(state_change_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());
        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .return_once(|| mock_retry_token);

        assert!(
            timeout(
                Duration::from_millis(100),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::GetWorkloadId(
                workload.instance_name.clone(),
                Ok(fixtures::WORKLOAD_IDS[0].to_string()),
            ),
            RuntimeCall::StartChecker(
                fixtures::WORKLOAD_IDS[0].to_string(),
                workload.clone(),
                state_checker_workload_state_sender.clone(),
                Ok(new_mock_state_checker),
            ),
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        control_loop_state.state_checker_workload_state_sender =
            state_checker_workload_state_sender;
        control_loop_state.state_checker_workload_state_receiver =
            state_checker_workload_state_receiver;

        workload_command_sender.resume().unwrap();
        workload_command_sender.delete().await.unwrap();

        assert!(
            timeout(
                Duration::from_millis(150),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload_workload_id_and_state_checker_updated() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (_, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::GetWorkloadId(
                workload.instance_name.clone(),
                Ok(fixtures::WORKLOAD_IDS[0].to_string()),
            ),
            RuntimeCall::StartChecker(
                fixtures::WORKLOAD_IDS[0].to_string(),
                workload.clone(),
                state_checker_workload_state_sender.clone(),
                Ok(StubStateChecker::new()),
            ),
        ]);

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
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

        assert_eq!(
            new_control_loop_state.workload_id,
            Some(fixtures::WORKLOAD_IDS[0].into())
        );
        assert!(new_control_loop_state.state_checker.is_some());

        drop(new_control_loop_state);
        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload_get_workload_id_fails() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (_, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::GetWorkloadId(
            workload.instance_name.clone(),
            Err(crate::runtime_connectors::RuntimeError::List(
                "some list workload error".to_string(),
            )),
        )]);

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let new_control_loop_state =
            WorkloadControlLoop::resume_workload_on_runtime(control_loop_state).await;

        assert!(new_control_loop_state.workload_id.is_none());
        assert!(new_control_loop_state.state_checker.is_none());

        drop(new_control_loop_state);
        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-resume~1]
    #[tokio::test]
    async fn utest_resume_workload_start_state_checker_fails() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (_, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);
        let (state_checker_workload_state_sender, state_checker_workload_state_receiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::GetWorkloadId(
                workload.instance_name.clone(),
                Ok(fixtures::WORKLOAD_IDS[0].to_string()),
            ),
            RuntimeCall::StartChecker(
                fixtures::WORKLOAD_IDS[0].to_string(),
                workload.clone(),
                state_checker_workload_state_sender.clone(),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    "some state checker error".to_string(),
                )),
            ),
        ]);

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(state_change_tx)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state.state_checker_workload_state_sender =
            state_checker_workload_state_sender;
        control_loop_state.state_checker_workload_state_receiver =
            state_checker_workload_state_receiver;

        let new_control_loop_state =
            WorkloadControlLoop::resume_workload_on_runtime(control_loop_state).await;

        assert_eq!(
            new_control_loop_state.workload_id,
            Some(fixtures::WORKLOAD_IDS[0].into())
        );
        assert!(new_control_loop_state.state_checker.is_none());

        drop(new_control_loop_state);
        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~workload-control-loop-receives-workload-states~1]
    // [utest->swdd~workload-control-loop-checks-workload-state-validity~1]
    // [utest->swdd~workload-control-loop-sends-workload-states~2]
    #[tokio::test]
    async fn utest_forward_received_workload_states_of_state_checker() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, mut workload_state_forward_rx) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![]);
        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(70)).await;
            workload_command_sender.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        // clone the control loop state's internally created workload state sender used by the state checker
        let state_checker_wl_state_sender = control_loop_state
            .state_checker_workload_state_sender
            .clone();

        let workload_state = generate_test_workload_state_with_workload_named(
            &workload,
            ExecutionStateSpec::running(),
        );

        state_checker_wl_state_sender
            .report_workload_execution_state(
                &workload_state.instance_name,
                workload_state.execution_state.clone(),
            )
            .await;

        assert!(
            timeout(
                Duration::from_millis(100),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert_eq!(
            Ok(Some(workload_state)),
            timeout(Duration::from_millis(100), workload_state_forward_rx.recv()).await
        );

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    // [utest->swdd~workload-control-loop-handles-workload-restarts~2]
    // [utest->swdd~workload-control-loop-restarts-workloads-using-update~1]
    #[tokio::test]
    async fn utest_restart_workload() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, _workload_state_forward_rx) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut workload = generate_test_workload_named();
        workload.workload.files = Default::default();

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())), // delete operation of the restarted workload
            RuntimeCall::CreateWorkload(
                workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[1].to_string(), // new workload id after restart
                    new_mock_state_checker,
                )),
            ),
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[1].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(70)).await;
            workload_command_sender.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(fixtures::WORKLOAD_IDS[0].into());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());
        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .once()
            .return_once(|| mock_retry_token);

        // clone the control loop state's internally created workload state sender used by the state checker
        let state_checker_wl_state_sender = control_loop_state
            .state_checker_workload_state_sender
            .clone();

        let workload_state = generate_test_workload_state_with_workload_named(
            &workload,
            ExecutionStateSpec::failed("test error".to_owned()),
        );

        state_checker_wl_state_sender
            .report_workload_execution_state(
                &workload_state.instance_name,
                workload_state.execution_state.clone(),
            )
            .await;

        assert!(
            timeout(
                Duration::from_millis(100),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    // [utest->swdd~workload-control-loop-handles-workload-restarts~2]
    // [utest->swdd~workload-control-loop-reuses-bundle-on-successful-restart~1]
    #[tokio::test]
    async fn utest_restart_workload_with_bundle_reuse() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, _workload_state_forward_rx) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut workload = generate_test_workload_named();
        workload.workload.files = Default::default();

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let mut new_mock_state_checker = StubStateChecker::new();
        new_mock_state_checker.panic_if_not_stopped();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::CreateWorkload(
                workload.clone(),
                Some(fixtures::WORKLOAD_IDS[0].to_string()), // reuse the existing ID
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[0].to_string(),
                    new_mock_state_checker,
                )),
            ),
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(70)).await;
            workload_command_sender.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state.workload_id = Some(fixtures::WORKLOAD_IDS[0].into());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());
        let mock_retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };
        control_loop_state
            .retry_manager
            .expect_new_token()
            .once()
            .return_once(|| mock_retry_token);

        let state_checker_wl_state_sender = control_loop_state
            .state_checker_workload_state_sender
            .clone();

        let workload_state = generate_test_workload_state_with_workload_named(
            &workload,
            ExecutionStateSpec::succeeded(),
        );

        state_checker_wl_state_sender
            .report_workload_execution_state(
                &workload_state.instance_name,
                workload_state.execution_state.clone(),
            )
            .await;

        assert!(
            timeout(
                Duration::from_millis(100),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    #[test]
    fn utest_is_restart_allowed_never() {
        let restart_policy = RestartPolicy::Never;

        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionStateSpec::running()
            )
        );
        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionStateSpec::succeeded()
            )
        );
        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionStateSpec::failed("some error".to_owned())
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
                &ExecutionStateSpec::running()
            )
        );
        assert!(WorkloadControlLoop::restart_policy_matches_execution_state(
            &restart_policy,
            &ExecutionStateSpec::failed("some error".to_owned())
        ));
        assert!(
            !WorkloadControlLoop::restart_policy_matches_execution_state(
                &restart_policy,
                &ExecutionStateSpec::succeeded()
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
                &ExecutionStateSpec::running()
            )
        );
        assert!(WorkloadControlLoop::restart_policy_matches_execution_state(
            &restart_policy,
            &ExecutionStateSpec::failed("some error".to_owned())
        ));
        assert!(WorkloadControlLoop::restart_policy_matches_execution_state(
            &restart_policy,
            &ExecutionStateSpec::succeeded()
        ));
    }

    // [utest->swdd~agent-sends-workload-states-of-its-workloads-to-server~2]
    // [utest->swdd~workload-control-loop-restarts-workload-with-enabled-restart-policy~2]
    // [utest->swdd~workload-control-loop-checks-workload-state-validity~1]
    #[test]
    fn utest_is_same_workload() {
        let current_instance_name = WorkloadInstanceNameSpec::builder()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .agent_name(fixtures::AGENT_NAMES[0])
            .config(&String::from("existing config"))
            .build();

        assert!(WorkloadControlLoop::is_same_workload(
            &current_instance_name,
            &current_instance_name
        ));

        let new_instance_name = WorkloadInstanceNameSpec::builder()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .agent_name(fixtures::AGENT_NAMES[0])
            .config(&String::from("different config"))
            .build();

        assert!(!WorkloadControlLoop::is_same_workload(
            &current_instance_name,
            &new_instance_name
        ));
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    // [utest->swdd~agent-workload-control-loop-updates-internal-state~1]
    #[tokio::test]
    async fn utest_create_workload_on_runtime_create_workload_files() {
        let _ = env_logger::builder().is_test(true).try_init();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (_, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, _workload_state_forward_rx) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let workload_configs_dir = WorkloadFilesBasePath::from((
            &PathBuf::from(fixtures::RUN_FOLDER),
            &workload.instance_name,
        ));

        let expected_mount_point_mappings = HashMap::from([
            (
                workload_configs_dir.join(fixtures::FILE_TEXT_PATH.trim_start_matches('/')),
                PathBuf::from(fixtures::FILE_TEXT_PATH),
            ),
            (
                workload_configs_dir.join(fixtures::FILE_BINARY_PATH.trim_start_matches('/')),
                PathBuf::from(fixtures::FILE_BINARY_PATH),
            ),
        ]);

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::CreateWorkload(
            workload.clone(),
            None,
            None,
            expected_mount_point_mappings.clone(),
            Ok((
                fixtures::WORKLOAD_IDS[0].to_string(),
                StubStateChecker::new(),
            )),
        )]);

        let mock_workload_files_creator_context = MockWorkloadFilesCreator::create_files_context();
        mock_workload_files_creator_context
            .expect()
            .once()
            .with(
                predicate::eq(workload_configs_dir),
                predicate::eq(workload.workload.files.files.clone()),
            )
            .returning(move |_, _| Ok(expected_mount_point_mappings.clone()));

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        let new_control_loop_state = WorkloadControlLoop::create_workload_on_runtime(
            control_loop_state,
            retry_token,
            WorkloadControlLoop::send_retry_for_workload,
        )
        .await;

        assert_eq!(
            new_control_loop_state.workload_named.workload.files,
            workload.workload.files
        );

        drop(new_control_loop_state);
        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    // [utest->swdd~agent-workload-control-loop-aborts-create-upon-workload-files-creation-error~1]
    #[tokio::test]
    async fn utest_create_workload_on_runtime_create_workload_files_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (_, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, mut workload_state_forward_rx) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mock_workload_files_creator_context = MockWorkloadFilesCreator::create_files_context();
        mock_workload_files_creator_context
            .expect()
            .once()
            .returning(move |_, _| {
                Err(WorkloadFileCreationError::new(
                    "failed to create workload files.".to_string(),
                ))
            });

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().once().returning(|_| Ok(()));

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(MockRuntimeConnector::new()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        WorkloadControlLoop::create_workload_on_runtime(
            control_loop_state,
            retry_token,
            WorkloadControlLoop::send_retry_for_workload,
        )
        .await;

        let workload_state_result =
            timeout(Duration::from_millis(100), workload_state_forward_rx.recv())
                .await
                .ok();
        assert!(workload_state_result.is_some());
        let workload_state = workload_state_result.unwrap().unwrap();

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        let expected_execution_state = ExecutionStateEnumSpec::Pending(Pending::StartingFailed);
        assert_eq!(
            *workload_state.execution_state.state(),
            expected_execution_state,
        );
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    // [utest->swdd~agent-workload-control-loop-handles-failed-workload-creation~1]
    #[tokio::test]
    async fn utest_create_workload_on_runtime_runtime_fails_with_unsupported_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (_, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (workload_state_forward_tx, mut workload_state_forward_rx) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::CreateWorkload(
            workload.clone(),
            None,
            None,
            HashMap::default(),
            Err(RuntimeError::Unsupported("unsupported error".to_string())),
        )]);

        let mock_workload_files_creator_context = MockWorkloadFilesCreator::create_files_context();
        mock_workload_files_creator_context
            .expect()
            .once()
            .returning(move |_, _| Ok(HashMap::default()));

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload.clone())
            .workload_state_sender(workload_state_forward_tx.clone())
            .run_folder(fixtures::RUN_FOLDER.into())
            .runtime(Box::new(runtime_mock))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        WorkloadControlLoop::create_workload_on_runtime(
            control_loop_state,
            retry_token,
            WorkloadControlLoop::send_retry_for_workload,
        )
        .await;

        let workload_state_result =
            timeout(Duration::from_millis(100), workload_state_forward_rx.recv())
                .await
                .ok();
        assert!(workload_state_result.is_some());
        let workload_state = workload_state_result.unwrap().unwrap();

        assert!(workload_command_receiver2.is_closed());
        assert!(workload_command_receiver2.is_empty());
        let expected_execution_state = ExecutionStateEnumSpec::Pending(Pending::StartingFailed);
        assert_eq!(
            *workload_state.execution_state.state(),
            expected_execution_state
        );
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    // [utest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
    // [utest->swdd~agent-workload-control-loop-update-create-failed-allows-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_create_failed_sends_retry() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, mut workload_command_receiver2) =
            WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut workload = generate_test_workload_named();
        workload.workload.files = Default::default();

        let instance_name = workload.instance_name.clone();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::CreateWorkload(
            workload.clone(),
            None,
            Some(fixtures::PIPES_LOCATION.into()),
            HashMap::default(),
            Err(crate::runtime_connectors::RuntimeError::Create(
                "some create error".to_string(),
            )),
            // We also send a delete command, but as no new workload was generated, there is also no
            // new ID so no call to the runtime is expected to happen here.
        )]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        workload_command_sender.create().await.unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        control_loop_state
            .retry_manager
            .expect_new_token()
            .once()
            .return_once(|| retry_token);

        assert!(
            timeout(
                Duration::from_millis(150),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        let Some(WorkloadCommand::Retry(received_instance_name, _received_retry_token)) =
            workload_command_receiver2.recv().await
        else {
            panic!()
        };
        assert_eq!(received_instance_name.as_ref(), &instance_name);
        assert!(workload_command_receiver2.is_empty());
        assert!(workload_command_receiver2.is_closed());
        runtime_mock.assert_all_expectations();
    }

    #[tokio::test]
    async fn utest_workload_obj_update_create_failed_send_retry() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, mut workload_command_receiver2) =
            WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut old_mock_state_checker = StubStateChecker::new();
        old_mock_state_checker.panic_if_not_stopped();

        let mut old_workload = generate_test_workload_named();
        old_workload.workload.files = Default::default();

        let mut new_workload = old_workload.clone();
        new_workload.workload.runtime_config = "changed config".to_string();
        new_workload.instance_name = WorkloadInstanceNameSpec::builder()
            .agent_name(old_workload.instance_name.agent_name())
            .workload_name(old_workload.instance_name.workload_name())
            .config(&new_workload.workload.runtime_config)
            .build();

        let new_instance_name = new_workload.instance_name.clone();

        let create_runtime_error_msg = "some create error";
        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::DeleteWorkload(OLD_WORKLOAD_ID.to_string(), Ok(())),
            RuntimeCall::CreateWorkload(
                new_workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    create_runtime_error_msg.to_owned(),
                )),
            ), // We also send a delete command, but as no new workload was generated, there is also no
               // new ID so no call to the runtime is expected to happen here.
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        workload_command_sender
            .update(Some(new_workload.clone()), CONTROL_INTERFACE_PATH.clone())
            .await
            .unwrap();
        workload_command_sender.delete().await.unwrap();

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(old_workload)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        // [utest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
        control_loop_state
            .retry_manager
            .expect_invalidate()
            .times(2)
            .return_const(());

        control_loop_state.workload_id = Some(OLD_WORKLOAD_ID.to_string());
        control_loop_state.state_checker = Some(old_mock_state_checker);

        control_loop_state
            .retry_manager
            .expect_new_token()
            .once()
            .return_once(|| retry_token);

        assert!(
            timeout(
                Duration::from_millis(150),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        let Some(WorkloadCommand::Retry(received_instance_name, _received_retry_token)) =
            workload_command_receiver2.recv().await
        else {
            panic!()
        };
        assert_eq!(received_instance_name.as_ref(), &new_instance_name);
        assert!(workload_command_receiver2.is_empty());
        assert!(workload_command_receiver2.is_closed());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-executes-retry~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_successful() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut workload = generate_test_workload_named();
        workload.workload.files = Default::default();

        let instance_name = workload.instance_name.clone();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            RuntimeCall::CreateWorkload(
                workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Ok((
                    fixtures::WORKLOAD_IDS[0].to_string(),
                    StubStateChecker::new(),
                )),
            ),
            RuntimeCall::DeleteWorkload(fixtures::WORKLOAD_IDS[0].to_string(), Ok(())),
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        workload_command_sender
            .retry(instance_name.clone(), retry_token)
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        assert!(
            timeout(
                Duration::from_millis(150),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_empty());
        assert!(workload_command_receiver2.is_closed());
        runtime_mock.assert_all_expectations();
    }

    #[tokio::test]
    async fn utest_workload_obj_run_retry_skip_if_retry_token_invalid() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, workload_command_receiver2) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let instance_name = workload.instance_name.clone();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![
            // We also send a delete command, but as no new workload was generated, there is also no
            // new ID so no call to the runtime is expected to happen here.
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let retry_token = MockRetryToken {
            valid: false,
            has_been_called: false,
        };

        workload_command_sender
            .retry(instance_name.clone(), retry_token)
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        assert!(
            timeout(
                Duration::from_millis(150),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        assert!(workload_command_receiver2.is_empty());
        assert!(workload_command_receiver2.is_closed());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
    #[tokio::test]
    async fn utest_workload_obj_run_retry_failed_sends_retry() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, mut workload_command_receiver2) =
            WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let mut workload = generate_test_workload_named();
        workload.workload.files = Default::default();

        let instance_name = workload.instance_name.clone();

        let mut runtime_mock = MockRuntimeConnector::new();
        let create_runtime_error_msg = "some create error";
        runtime_mock.expect(vec![
            RuntimeCall::CreateWorkload(
                workload.clone(),
                None,
                Some(fixtures::PIPES_LOCATION.into()),
                HashMap::default(),
                Err(crate::runtime_connectors::RuntimeError::Create(
                    create_runtime_error_msg.to_owned(),
                )),
            ), // We also send a delete command, but as no new workload was generated, there is also no
               // new ID so no call to the runtime is expected to happen here.
        ]);

        let mock_remove_dir = mock_filesystem_async::remove_dir_all_context();
        mock_remove_dir.expect().returning(|_| Ok(()));

        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        workload_command_sender
            .retry(instance_name.clone(), retry_token)
            .await
            .unwrap();

        let workload_command_sender_clone = workload_command_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            workload_command_sender_clone.delete().await.unwrap();
        });

        let mut control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        control_loop_state
            .retry_manager
            .expect_invalidate()
            .once()
            .return_const(());

        assert!(
            timeout(
                Duration::from_millis(150),
                WorkloadControlLoop::run(control_loop_state)
            )
            .await
            .is_ok()
        );

        let Some(WorkloadCommand::Retry(received_instance_name, _received_retry_token)) =
            workload_command_receiver2.recv().await
        else {
            panic!()
        };
        assert_eq!(received_instance_name.as_ref(), &instance_name);
        assert!(workload_command_receiver2.is_empty());
        assert!(workload_command_receiver2.is_closed());
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-creates-log-fetcher~1]
    #[tokio::test]
    async fn utest_get_logs_successful() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, _) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::StartLogFetcher(
            LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
            Ok(Box::new(MockLogFetcher::new())),
        )]);

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .workload_id(Some(fixtures::WORKLOAD_IDS[0].into()))
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let jh = tokio::spawn(async move {
            workload_command_sender
                .start_collecting_logs(LogRequestOptions {
                    follow: true,
                    tail: None,
                    since: None,
                    until: None,
                })
                .await
                .unwrap();
        });

        timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state),
        )
        .await
        .unwrap();

        timeout(Duration::from_millis(100), jh)
            .await
            .unwrap()
            .unwrap();
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-creates-log-fetcher~1]
    #[tokio::test]
    async fn utest_get_logs_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, _) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::StartLogFetcher(
            LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
            Err(RuntimeError::CollectLog("mock".into())),
        )]);

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .workload_id(Some(fixtures::WORKLOAD_IDS[0].into()))
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let jh = tokio::spawn(async move {
            workload_command_sender
                .start_collecting_logs(LogRequestOptions {
                    follow: true,
                    tail: None,
                    since: None,
                    until: None,
                })
                .await
                .unwrap_err();
        });

        timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state),
        )
        .await
        .unwrap();

        timeout(Duration::from_millis(100), jh)
            .await
            .unwrap()
            .unwrap();
        runtime_mock.assert_all_expectations();
    }

    // [utest->swdd~agent-workload-control-loop-creates-log-fetcher~1]
    #[tokio::test]
    async fn utest_get_logs_no_workload_id_yet() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();
        let (workload_command_sender2, _) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let runtime_mock = MockRuntimeConnector::new();

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let jh = tokio::spawn(async move {
            workload_command_sender
                .start_collecting_logs(LogRequestOptions {
                    follow: true,
                    tail: None,
                    since: None,
                    until: None,
                })
                .await
                .unwrap_err();
        });

        timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state),
        )
        .await
        .unwrap();

        timeout(Duration::from_millis(100), jh)
            .await
            .unwrap()
            .unwrap();
        runtime_mock.assert_all_expectations();
    }

    #[tokio::test]
    async fn utest_get_logs_cant_send_result() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_command_sender, workload_command_receiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);
        let (workload_command_sender2, _) = WorkloadCommandSender::new();
        let (state_change_tx, _state_change_rx) = mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mut runtime_mock = MockRuntimeConnector::new();
        runtime_mock.expect(vec![RuntimeCall::StartLogFetcher(
            LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
            Ok(Box::new(MockLogFetcher::new())),
        )]);

        let control_loop_state = ControlLoopState::builder()
            .workload_named(workload)
            .workload_id(Some(fixtures::WORKLOAD_IDS[0].into()))
            .run_folder(fixtures::RUN_FOLDER.into())
            .control_interface_path(CONTROL_INTERFACE_PATH.clone())
            .workload_state_sender(state_change_tx)
            .runtime(Box::new(runtime_mock.clone()))
            .workload_command_receiver(workload_command_receiver)
            .retry_sender(workload_command_sender2)
            .build()
            .unwrap();

        let (result_sink, _) = oneshot::channel();

        let jh = tokio::spawn(async move {
            workload_command_sender
                .send(WorkloadCommand::StartLogFetcher(
                    LogRequestOptions {
                        follow: true,
                        tail: None,
                        since: None,
                        until: None,
                    },
                    result_sink,
                ))
                .await
                .unwrap();
        });

        timeout(
            Duration::from_millis(150),
            WorkloadControlLoop::run(control_loop_state),
        )
        .await
        .unwrap();

        timeout(Duration::from_millis(100), jh)
            .await
            .unwrap()
            .unwrap();
        runtime_mock.assert_all_expectations();
    }
}
