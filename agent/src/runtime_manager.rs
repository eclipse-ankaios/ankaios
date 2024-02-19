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

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use common::{
    commands::Response,
    objects::{
        AgentName, DeletedWorkload, ExecutionState, WorkloadExecutionInstanceName,
        WorkloadInstanceName, WorkloadSpec, WorkloadState,
    },
    request_id_prepending::detach_prefix_from_request_id,
    std_extensions::IllegalStateResult,
    to_server_interface::{ToServerInterface, ToServerSender},
};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;

#[cfg_attr(test, mockall_double::double)]
use crate::dependency_scheduler::DependencyScheduler;

#[cfg_attr(test, mockall_double::double)]
use crate::parameter_storage::ParameterStorage;
use crate::runtime_connectors::RuntimeFacade;

#[cfg_attr(test, mockall_double::double)]
use crate::workload::Workload;

#[cfg(test)]
use mockall::automock;

fn flatten(
    mut runtime_workload_map: HashMap<String, HashMap<String, WorkloadSpec>>,
) -> Vec<WorkloadSpec> {
    runtime_workload_map
        .drain()
        .flat_map(|(_, mut v)| v.drain().map(|(_, y)| y).collect::<Vec<_>>())
        .collect::<Vec<_>>()
}

pub struct RuntimeManager {
    agent_name: AgentName,
    run_folder: PathBuf,
    control_interface_tx: ToServerSender,
    initial_workload_list_received: bool,
    workloads: HashMap<String, Workload>,
    // [impl->swdd~agent-supports-multiple-runtime-connectors~1]
    runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
    update_state_tx: ToServerSender,
    parameter_storage: ParameterStorage,
    dependency_scheduler: DependencyScheduler,
}

#[cfg_attr(test, automock)]
impl RuntimeManager {
    pub fn new(
        agent_name: AgentName,
        run_folder: PathBuf,
        control_interface_tx: ToServerSender,
        runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
        update_state_tx: ToServerSender,
    ) -> Self {
        RuntimeManager {
            agent_name,
            run_folder,
            control_interface_tx,
            initial_workload_list_received: false,
            workloads: HashMap::new(),
            runtime_map,
            update_state_tx,
            parameter_storage: ParameterStorage::new(),
            dependency_scheduler: DependencyScheduler::new(),
        }
    }

    pub async fn update_workload_state(&mut self, new_workload_state: WorkloadState) {
        self.parameter_storage
            .update_workload_state(new_workload_state);

        let added_workloads = self
            .dependency_scheduler
            .next_workloads_to_start(&self.parameter_storage);

        let deleted_workloads = self
            .dependency_scheduler
            .next_workloads_to_delete(&self.parameter_storage);

        if !added_workloads.is_empty() || !deleted_workloads.is_empty() {
            self.handle_subsequent_update_workload(added_workloads, deleted_workloads)
                .await;
        }
    }

    async fn report_pending_state_for_waiting_workloads(&self, waiting_workloads: &[WorkloadSpec]) {
        for workload in waiting_workloads.iter() {
            self.update_state_tx
                .update_workload_state(vec![WorkloadState {
                    instance_name: workload.instance_name(),
                    execution_state: ExecutionState::waiting_to_start(),
                    ..Default::default()
                }])
                .await
                .unwrap_or_illegal_state();
        }
    }

    async fn report_pending_delete_state_for_waiting_workloads(
        &self,
        waiting_workloads: &[DeletedWorkload],
    ) {
        for workload in waiting_workloads.iter() {
            // todo: refactor and simplify conditions when WorkloadExecutionInstanceName is part of ParameterStorage entry
            // a transition to state Running(WaitingToStop) is only allowed if the workload was in the Running(Ok) state before
            if self
                .parameter_storage
                .get_workload_state(self.agent_name.get(), &workload.name)
                .map_or(false, |execution_state| execution_state.is_running())
            {
                if let Some(wl) = self.workloads.get(&workload.name) {
                    self.update_state_tx
                        .update_workload_state(vec![WorkloadState {
                            instance_name: wl.instance_name(),
                            execution_state: ExecutionState::waiting_to_stop(),
                            ..Default::default()
                        }])
                        .await
                        .unwrap_or_illegal_state();
                }
            }
        }
    }

    async fn enqueue_waiting_workloads(&mut self, waiting_workloads: Vec<WorkloadSpec>) {
        self.report_pending_state_for_waiting_workloads(&waiting_workloads)
            .await;

        self.dependency_scheduler
            .put_on_waiting_queue(waiting_workloads);
    }

    async fn enqueue_waiting_deleted_workloads(
        &mut self,
        waiting_deleted_workloads: Vec<DeletedWorkload>,
    ) {
        self.report_pending_delete_state_for_waiting_workloads(&waiting_deleted_workloads)
            .await;

        self.dependency_scheduler
            .put_on_delete_waiting_queue(waiting_deleted_workloads);
    }

    pub async fn handle_update_workload(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) {
        log::info!(
            "Received a new desired state with '{}' added and '{}' deleted workloads.",
            added_workloads.len(),
            deleted_workloads.len()
        );

        if !self.initial_workload_list_received {
            self.initial_workload_list_received = true;
            if !deleted_workloads.is_empty() {
                log::error!(
                    "Received an initial workload list with delete workload commands: '{:?}'",
                    deleted_workloads
                );
            }

            // [impl->swdd~agent-initial-list-existing-workloads~1]
            self.handle_initial_update_workload(added_workloads).await;
        } else {
            let (ready_workloads, waiting_workloads) =
                DependencyScheduler::split_workloads_to_ready_and_waiting(added_workloads);

            self.enqueue_waiting_workloads(waiting_workloads).await;

            let (ready_deleted_workloads, waiting_deleted_workloads) =
                DependencyScheduler::split_deleted_workloads_to_ready_and_waiting(
                    deleted_workloads,
                    &self.parameter_storage,
                );

            self.enqueue_waiting_deleted_workloads(waiting_deleted_workloads)
                .await;

            self.handle_subsequent_update_workload(ready_workloads, ready_deleted_workloads)
                .await;
        }
    }

    // [impl->swdd~agent-forward-responses-to-control-interface-pipe~1]
    pub async fn forward_response(&mut self, response: Response) {
        // [impl->swdd~agent-uses-id-prefix-forward-control-interface-response-correct-workload~1]
        // [impl->swdd~agent-remove-id-prefix-forwarding-control-interface-response~1]
        let (workload_name, request_id) = detach_prefix_from_request_id(&response.request_id);
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            if let Err(err) = workload
                .forward_response(request_id, response.response_content)
                .await
            {
                log::warn!(
                    "Could not forward response to workload '{}': '{}'",
                    workload_name,
                    err
                );
            }
        } else {
            log::warn!(
                "Could not forward response for unknown workload: '{}'",
                workload_name
            );
        }
    }

    // [impl->swdd~agent-initial-list-existing-workloads~1]
    async fn handle_initial_update_workload(&mut self, added_workloads: Vec<WorkloadSpec>) {
        log::debug!("Handling initial workload list.");

        // create a list per runtime
        let mut added_workloads_per_runtime: HashMap<String, HashMap<String, WorkloadSpec>> =
            HashMap::new();
        for workload_spec in added_workloads {
            if let Some(workload_map) = added_workloads_per_runtime.get_mut(&workload_spec.runtime)
            {
                workload_map.insert(workload_spec.name.clone(), workload_spec);
            } else {
                added_workloads_per_runtime.insert(
                    workload_spec.runtime.clone(),
                    HashMap::from([(workload_spec.name.clone(), workload_spec)]),
                );
            }
        }

        let mut waiting_reused_workloads = Vec::new();
        // Go through each runtime and find the still running workloads
        // [impl->swdd~agent-existing-workloads-finds-list~1]
        for (runtime_name, runtime) in &self.runtime_map {
            match runtime
                .get_reusable_running_workloads(&self.agent_name)
                .await
            {
                Ok(running_instance_names) => {
                    log::info!(
                        "Found '{}' reusable '{}' workload(s).",
                        running_instance_names.len(),
                        runtime_name,
                    );

                    for instance_name in running_instance_names {
                        if let Some(new_workload_spec) = added_workloads_per_runtime
                            .get_mut(runtime_name)
                            .and_then(|map| map.remove(instance_name.workload_name()))
                        {
                            let new_instance_name: WorkloadExecutionInstanceName =
                                new_workload_spec.instance_name();

                            // We have a running workload that matches a new added workload; check if the config is updated
                            // [impl->swdd~agent-stores-running-workload~1]
                            if new_instance_name == instance_name {
                                // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
                                let control_interface = Self::create_control_interface(
                                    &self.run_folder,
                                    self.control_interface_tx.clone(),
                                    &new_workload_spec,
                                );

                                log::info!(
                                    "Resuming workload '{}'",
                                    new_instance_name.workload_name()
                                );
                                // [impl->swdd~agent-existing-workloads-resume-existing~1]
                                self.workloads.insert(
                                    new_workload_spec.name.to_string(),
                                    runtime.resume_workload(
                                        new_workload_spec,
                                        control_interface,
                                        &self.update_state_tx,
                                    ),
                                );
                            } else {
                                // [impl->swdd~agent-existing-workloads-replace-updated~1]

                                /* If all the dependencies for this workload are on another agent and fulfilled,
                                 the workload could be replaced immediately.
                                If the workload has dependencies on this agent,
                                the workload is put on the waiting queue and started
                                when the execution states of this agent are received and all dependencies are fulfilled.
                                Reason: get_reusable_running_workloads(..) does not return the execution states for existing workloads */
                                if DependencyScheduler::dependency_states_for_start_fulfilled(
                                    &new_workload_spec,
                                    &self.parameter_storage,
                                ) {
                                    // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
                                    let control_interface = Self::create_control_interface(
                                        &self.run_folder,
                                        self.control_interface_tx.clone(),
                                        &new_workload_spec,
                                    );

                                    log::info!(
                                        "Replacing workload '{}'",
                                        new_instance_name.workload_name()
                                    );

                                    self.workloads.insert(
                                        new_workload_spec.name.to_string(),
                                        runtime.replace_workload(
                                            new_instance_name,
                                            new_workload_spec,
                                            control_interface,
                                            &self.update_state_tx,
                                        ),
                                    );
                                } else {
                                    /* prevent that the workload that shall be replaced
                                    exists with the old state on the runtime until it is replaced. */

                                    log::info!("Deleting existing workload '{}'. It is created when its dependencies are fulfilled."
                                        , instance_name.workload_name()
                                    );

                                    runtime.delete_workload(instance_name);
                                    waiting_reused_workloads.push(new_workload_spec);
                                }
                            }
                        } else {
                            // No added workload matches the found running one => delete it
                            // [impl->swdd~agent-existing-workloads-delete-unneeded~1]
                            runtime.delete_workload(instance_name);
                        }
                    }
                }
                Err(err) => log::warn!("Could not get reusable running workloads: '{}'", err),
            }
        }

        self.enqueue_waiting_workloads(waiting_reused_workloads)
            .await;

        let (ready_workloads, new_waiting_workloads) =
            DependencyScheduler::split_workloads_to_ready_and_waiting(flatten(
                added_workloads_per_runtime,
            ));

        self.enqueue_waiting_workloads(new_waiting_workloads).await;

        // now start all workloads that did not exist and do not have to wait for dependencies
        for workload_spec in ready_workloads {
            // [impl->swdd~agent-existing-workloads-starts-new-if-not-found~1]
            self.add_workload(workload_spec).await;
        }
    }

    async fn handle_subsequent_update_workload(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) {
        // transform into a hashmap to be able to search for updates
        // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
        let mut added_workloads: HashMap<String, WorkloadSpec> = added_workloads
            .into_iter()
            .map(|workload_spec| (workload_spec.name.to_string(), workload_spec))
            .collect();

        // [impl->swdd~agent-handle-deleted-before-added-workloads~1]
        for deleted_workload in deleted_workloads {
            if let Some(updated_workload) = added_workloads.remove(&deleted_workload.name) {
                // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
                self.update_workload(updated_workload).await;
            } else {
                // [impl->swdd~agent-deletes-workload~1]
                self.delete_workload(deleted_workload).await;
            }
        }

        for (_, workload_spec) in added_workloads {
            let workload_name = &workload_spec.name;
            if self.workloads.get(workload_name).is_some() {
                log::warn!(
                    "Added workload '{}' already exists. Updating.",
                    workload_name
                );
                // We know this workload, seems the server is sending it again, try an update
                // [impl->swdd~agent-update-on-add-known-workload~1]
                self.update_workload(workload_spec).await;
            } else {
                // [impl->swdd~agent-added-creates-workload~1]
                self.add_workload(workload_spec).await;
            }
        }
    }

    async fn add_workload(&mut self, workload_spec: WorkloadSpec) {
        let workload_name = workload_spec.name.clone();

        // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
        let control_interface = Self::create_control_interface(
            &self.run_folder,
            self.control_interface_tx.clone(),
            &workload_spec,
        );

        // [impl->swdd~agent-uses-specified-runtime~1]
        // [impl->swdd~agent-skips-unknown-runtime~1]
        if let Some(runtime) = self.runtime_map.get(&workload_spec.runtime) {
            let workload =
                runtime.create_workload(workload_spec, control_interface, &self.update_state_tx);
            // [impl->swdd~agent-stores-running-workload~1]
            self.workloads.insert(workload_name, workload);
        } else {
            log::warn!(
                "Could not find runtime '{}'. Workload '{}' not scheduled.",
                workload_spec.runtime,
                workload_name
            );
        }
    }

    async fn delete_workload(&mut self, deleted_workload: DeletedWorkload) {
        if let Some(workload) = self.workloads.remove(&deleted_workload.name) {
            if let Err(err) = workload.delete().await {
                log::error!(
                    "Failed to delete workload '{}': '{}'",
                    deleted_workload.name,
                    err
                );
            }
        } else {
            log::warn!("Workload '{}' already gone.", deleted_workload.name);
        }
    }

    // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
    async fn update_workload(&mut self, workload_spec: WorkloadSpec) {
        let workload_name = workload_spec.name.clone();
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
            let control_interface = Self::create_control_interface(
                &self.run_folder,
                self.control_interface_tx.clone(),
                &workload_spec,
            );
            if let Err(err) = workload.update(workload_spec, control_interface).await {
                log::error!("Failed to update workload '{}': '{}'", workload_name, err);
            }
        } else {
            log::warn!(
                "Workload for update '{}' not found. Recreating.",
                workload_name
            );
            // [impl->swdd~agent-add-on-update-missing-workload~1]
            self.add_workload(workload_spec).await;
        }
    }

    // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
    fn create_control_interface(
        run_folder: &Path,
        control_interface_tx: ToServerSender,
        workload_spec: &WorkloadSpec,
    ) -> Option<PipesChannelContext> {
        log::debug!("Creating control interface pipes for '{:?}'", workload_spec);

        match PipesChannelContext::new(
            run_folder,
            &workload_spec.instance_name(),
            control_interface_tx,
        ) {
            Ok(pipes_channel_context) => Some(pipes_channel_context),
            Err(err) => {
                log::warn!(
                    "Could not create pipes channel context for workload '{}'. Error: '{err}'",
                    workload_spec.name
                );
                None
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
    use super::*;
    use crate::control_interface::MockPipesChannelContext;
    use crate::dependency_scheduler::MockDependencyScheduler;
    use crate::parameter_storage::MockParameterStorage;
    use crate::runtime_connectors::{MockRuntimeFacade, RuntimeError};
    use crate::workload::{MockWorkload, WorkloadError};
    use common::commands::{ResponseContent, UpdateWorkloadState};
    use common::objects::{
        generate_test_workload_state, AddCondition, WorkloadExecutionInstanceNameBuilder,
    };
    use common::test_utils::{
        generate_test_complete_state, generate_test_deleted_workload,
        generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
    };
    use common::to_server_interface::{ToServer, ToServerReceiver};
    use mockall::{predicate, Sequence};
    use tokio::sync::mpsc::channel;

    const BUFFER_SIZE: usize = 20;
    const RUNTIME_NAME: &str = "runtime1";
    const RUNTIME_NAME_2: &str = "runtime2";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_2_NAME: &str = "workload2";
    const REQUEST_ID: &str = "request_id";
    const RUN_FOLDER: &str = "run/folder";

    #[derive(Default)]
    pub struct RuntimeManagerBuilder {
        runtime_facade_map: HashMap<String, Box<dyn RuntimeFacade>>,
    }

    impl RuntimeManagerBuilder {
        pub fn with_runtime(
            mut self,
            runtime_name: &str,
            runtime_facade: Box<dyn RuntimeFacade>,
        ) -> Self {
            self.runtime_facade_map
                .insert(runtime_name.to_string(), runtime_facade);
            self
        }

        pub fn build(self) -> (ToServerReceiver, RuntimeManager) {
            let (to_server, server_receiver) = channel(BUFFER_SIZE);
            let runtime_manager = RuntimeManager::new(
                AGENT_NAME.into(),
                Path::new(RUN_FOLDER).into(),
                to_server.clone(),
                self.runtime_facade_map,
                to_server.clone(),
            );
            (server_receiver, runtime_manager)
        }
    }

    // [utest->swdd~agent-initial-list-existing-workloads~1]
    // [utest->swdd~agent-supports-multiple-runtime-connectors~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_handle() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .times(2)
            .returning(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .times(2)
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock
            .expect_create_workload()
            .once()
            .returning(move |_, _, _| MockWorkload::default());

        let mut runtime_facade_mock_2 = MockRuntimeFacade::new();
        runtime_facade_mock_2
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock_2
            .expect_create_workload()
            .once()
            .returning(move |_, _, _| MockWorkload::default());

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .with_runtime(
                RUNTIME_NAME_2,
                Box::new(runtime_facade_mock_2) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![
            generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_1_NAME.to_string(),
                RUNTIME_NAME.to_string(),
            ),
            generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_2_NAME.to_string(),
                RUNTIME_NAME_2.to_string(),
            ),
        ];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_2_NAME));
    }

    // [utest->swdd~agent-skips-unknown-runtime~1]
    #[tokio::test]
    async fn utest_handle_update_workload_no_workload_with_unknown_runtime() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .times(2)
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock.expect_create_workload().never(); // workload shall not be created due to unknown runtime

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            "unknown_runtime1".to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.is_empty());
    }

    // [utest->swdd~agent-existing-workloads-finds-list~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_failed_to_get_reusable_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .times(2)
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .returning(|_| {
                Box::pin(async {
                    Err(RuntimeError::List(
                        "failed to get reusable workloads".to_string(),
                    ))
                })
            });

        runtime_facade_mock
            .expect_create_workload()
            .once()
            .withf(|workload_spec, control_interface, to_server| {
                workload_spec.name == *WORKLOAD_1_NAME
                    && control_interface.is_some()
                    && !to_server.is_closed()
            })
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;
        server_receiver.close();

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-existing-workloads-resume-existing~1]
    // [utest->swdd~agent-existing-workloads-starts-new-if-not-found~1]
    // [utest->swdd~agent-stores-running-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_resume_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .returning(move |_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .times(2)
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let existing_workload1 = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let existing_workload1_name = existing_workload1.instance_name();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![existing_workload1_name]) }));

        runtime_facade_mock
            .expect_resume_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        runtime_facade_mock.expect_create_workload().never();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((vec![], vec![]));

        let added_workloads = vec![existing_workload1];
        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-existing-workloads-replace-updated~1]
    // [utest->swdd~agent-stores-running-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_replace_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .times(2)
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let existing_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        // create workload with different config string to simulate a replace of a existing workload
        let existing_workload_with_other_config = WorkloadExecutionInstanceNameBuilder::default()
            .workload_name(WORKLOAD_1_NAME)
            .config(&String::from("different config"))
            .agent_name(AGENT_NAME)
            .build();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![existing_workload_with_other_config]) }));

        runtime_facade_mock
            .expect_replace_workload()
            .once()
            .return_once(|_, _, _, _| MockWorkload::default());

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((vec![], vec![]));

        let dependency_scheduler_start_context =
            MockDependencyScheduler::dependency_states_for_start_fulfilled_context();
        dependency_scheduler_start_context
            .expect()
            .once()
            .return_const(true);

        let added_workloads = vec![existing_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // todo linkage
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_replace_workload_with_not_fulfilled_dependencies(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let existing_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        let reused_waiting_workloads = vec![existing_workload.clone()];
        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .with(predicate::eq(reused_waiting_workloads))
            .return_const(());

        let waiting_new_workloads = vec![];
        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .with(predicate::eq(waiting_new_workloads))
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((vec![], vec![]));

        let dependency_scheduler_start_context =
            MockDependencyScheduler::dependency_states_for_start_fulfilled_context();
        dependency_scheduler_start_context
            .expect()
            .once()
            .return_const(false);

        // create workload with different config string to simulate a replace of a existing workload
        let existing_workload_with_other_config = WorkloadExecutionInstanceNameBuilder::default()
            .workload_name(WORKLOAD_1_NAME)
            .config(&String::from("different config"))
            .agent_name(AGENT_NAME)
            .build();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        let existing_instance_name = existing_workload_with_other_config.clone();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![existing_workload_with_other_config]) }));

        runtime_facade_mock
            .expect_delete_workload()
            .once()
            .with(predicate::eq(existing_instance_name))
            .return_const(());

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![existing_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;

        server_receiver.close();

        assert!(runtime_manager.initial_workload_list_received);
        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-existing-workloads-delete-unneeded~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_delete_unneeded() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .times(2)
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let existing_workload_with_other_config = WorkloadExecutionInstanceNameBuilder::default()
            .workload_name(WORKLOAD_1_NAME)
            .config(&String::from("different config"))
            .agent_name(AGENT_NAME)
            .build();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![existing_workload_with_other_config]) }));

        runtime_facade_mock
            .expect_delete_workload()
            .once()
            .return_const(());

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((vec![], vec![]));

        runtime_manager.handle_update_workload(vec![], vec![]).await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.is_empty());
    }

    // [utest->swdd~agent-updates-deleted-and-added-workloads~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_update_on_add_and_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .return_const(());

        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let runtime_facade_mock = MockRuntimeFacade::new();
        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager.initial_workload_list_received = true;

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .with(
                predicate::function(|workload_spec: &WorkloadSpec| {
                    workload_spec.name == *WORKLOAD_1_NAME
                }),
                predicate::function(|control_interface: &Option<PipesChannelContext>| {
                    control_interface.is_some()
                }),
            )
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        let deleted_workloads = vec![generate_test_deleted_workload(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
        )];

        let dependency_scheduler_delete_context =
            MockDependencyScheduler::split_deleted_workloads_to_ready_and_waiting_context();
        dependency_scheduler_delete_context
            .expect()
            .once()
            .return_const((deleted_workloads.clone(), vec![]));

        // workload is in added and deleted workload vec
        runtime_manager
            .handle_update_workload(added_workloads, deleted_workloads)
            .await;

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-deletes-workload~1]
    // [utest->swdd~agent-handle-deleted-before-added-workloads~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_delete_before_adding() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .return_const(());

        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut delete_before_add_seq = Sequence::new();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_delete()
            .once()
            .in_sequence(&mut delete_before_add_seq)
            .return_once(move || Ok(()));

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .withf(|workload_spec, control_interface, to_server| {
                workload_spec.name == *WORKLOAD_2_NAME
                    && control_interface.is_some()
                    && !to_server.is_closed()
            })
            .in_sequence(&mut delete_before_add_seq)
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager.initial_workload_list_received = true;

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_2_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        let deleted_workloads = vec![generate_test_deleted_workload(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
        )];

        let dependency_scheduler_delete_context =
            MockDependencyScheduler::split_deleted_workloads_to_ready_and_waiting_context();
        dependency_scheduler_delete_context
            .expect()
            .once()
            .return_const((deleted_workloads.clone(), vec![]));

        runtime_manager
            .handle_update_workload(added_workloads, deleted_workloads)
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_2_NAME));
    }

    // [utest->swdd~agent-add-on-update-missing-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_add_on_update_missing() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .return_const(());

        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .returning(move |_, _, _| MockWorkload::default());

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        let deleted_workloads = vec![generate_test_deleted_workload(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
        )];

        let dependency_scheduler_delete_context =
            MockDependencyScheduler::split_deleted_workloads_to_ready_and_waiting_context();
        dependency_scheduler_delete_context
            .expect()
            .once()
            .return_const((deleted_workloads.clone(), vec![]));

        runtime_manager.initial_workload_list_received = true;

        runtime_manager
            .handle_update_workload(added_workloads, deleted_workloads)
            .await;

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-update-on-add-known-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_update_known_added() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .return_const(());

        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let runtime_facade_mock = MockRuntimeFacade::new();
        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager.initial_workload_list_received = true;

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .withf(|workload_spec, control_interface| {
                workload_spec.name == *WORKLOAD_1_NAME && control_interface.is_some()
            })
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        let dependency_scheduler_delete_context =
            MockDependencyScheduler::split_deleted_workloads_to_ready_and_waiting_context();
        dependency_scheduler_delete_context
            .expect()
            .once()
            .return_const((vec![], vec![]));

        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-added-creates-workload~1]
    // [utest->swdd~agent-uses-specified-runtime~1]
    // [utest->swdd~agent-stores-running-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_add_new_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();

        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .return_const(());

        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .withf(|workload_spec, control_interface, to_server| {
                workload_spec.name == *WORKLOAD_1_NAME
                    && control_interface.is_some()
                    && !to_server.is_closed()
            })
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let dependency_scheduler_context =
            MockDependencyScheduler::split_workloads_to_ready_and_waiting_context();
        dependency_scheduler_context
            .expect()
            .once()
            .return_const((added_workloads.clone(), vec![]));

        let dependency_scheduler_delete_context =
            MockDependencyScheduler::split_deleted_workloads_to_ready_and_waiting_context();
        dependency_scheduler_delete_context
            .expect()
            .once()
            .return_const((vec![], vec![]));

        runtime_manager.initial_workload_list_received = true;

        runtime_manager
            .handle_update_workload(added_workloads, vec![])
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    // [utest->swdd~agent-uses-id-prefix-forward-control-interface-response-correct-workload~1]
    // [utest->swdd~agent-remove-id-prefix-forwarding-control-interface-response~1]
    #[tokio::test]
    async fn utest_forward_complete_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(MockDependencyScheduler::default);

        let runtime_facade_mock = MockRuntimeFacade::new();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_forward_response()
            .once()
            .withf(|request_id, response_content| {
                request_id == REQUEST_ID
                    && matches!(response_content, ResponseContent::CompleteState(complete_state) if complete_state
                        .workload_states
                        .first()
                        .unwrap()
                        .instance_name.workload_name()
                        == WORKLOAD_1_NAME)
            })
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), mock_workload);

        runtime_manager
            .forward_response(Response {
                request_id: format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
                response_content: ResponseContent::CompleteState(Box::new(
                    generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                        AGENT_NAME.to_string(),
                        WORKLOAD_1_NAME.to_string(),
                        RUNTIME_NAME.to_string(),
                    )]),
                )),
            })
            .await;
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_forward_complete_state_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(MockDependencyScheduler::default);

        let runtime_facade_mock = MockRuntimeFacade::new();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_forward_response()
            .once()
            .withf(|request_id, response_content| {
                request_id == REQUEST_ID
                    && matches!(response_content, ResponseContent::CompleteState(complete_state) if complete_state
                    .workload_states
                    .first()
                    .unwrap()
                    .instance_name.workload_name()
                    == WORKLOAD_1_NAME)
            })
            .return_once(move |_, _| {
                Err(WorkloadError::CompleteState(
                    "failed to send complete state".to_string(),
                ))
            });

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), mock_workload);

        runtime_manager
            .forward_response(Response {
                request_id: format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
                response_content: ResponseContent::CompleteState(Box::new(
                    generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                        AGENT_NAME.to_string(),
                        WORKLOAD_1_NAME.to_string(),
                        RUNTIME_NAME.to_string(),
                    )]),
                )),
            })
            .await;
    }

    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_forward_complete_state_not_called_because_workload_not_found() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let parameter_storage_mock = MockParameterStorage::new_context();
        parameter_storage_mock
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(MockDependencyScheduler::default);

        let runtime_facade_mock = MockRuntimeFacade::new();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload.expect_forward_response().never();

        runtime_manager
            .forward_response(Response {
                request_id: format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
                response_content: ResponseContent::CompleteState(Box::new(
                    generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                        AGENT_NAME.to_string(),
                        WORKLOAD_1_NAME.to_string(),
                        RUNTIME_NAME.to_string(),
                    )]),
                )),
            })
            .await;
    }

    #[tokio::test]
    async fn utest_update_workload_state_create_workload_with_fulfilled_dependencies() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_update_workload_state()
            .once()
            .return_const(());

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let ready_workloads = vec![generate_test_workload_spec_with_dependencies(
            AGENT_NAME,
            WORKLOAD_1_NAME,
            RUNTIME_NAME,
            HashMap::from([(WORKLOAD_2_NAME.to_string(), AddCondition::AddCondRunning)]),
        )];

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_next_workloads_to_start()
            .once()
            .return_const(ready_workloads);

        mock_dependency_scheduler
            .expect_next_workloads_to_delete()
            .once()
            .return_const(vec![]);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let new_workload_state =
            generate_test_workload_state(WORKLOAD_2_NAME, ExecutionState::running());

        runtime_manager
            .update_workload_state(new_workload_state)
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_update_workload_state_no_create_workload_when_dependencies_not_fulfilled() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_update_workload_state()
            .once()
            .return_const(());

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_next_workloads_to_start()
            .once()
            .return_const(vec![]);

        mock_dependency_scheduler
            .expect_next_workloads_to_delete()
            .once()
            .return_const(vec![]);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock.expect_create_workload().never();

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let new_workload_state =
            generate_test_workload_state(WORKLOAD_2_NAME, ExecutionState::succeeded());

        runtime_manager
            .update_workload_state(new_workload_state)
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_update_workload_state_delete_workload_dependencies_with_fulfilled_dependencies()
    {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_update_workload_state()
            .once()
            .return_const(());

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_next_workloads_to_start()
            .once()
            .return_const(vec![]);

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        mock_dependency_scheduler
            .expect_next_workloads_to_delete()
            .once()
            .return_const(vec![deleted_workload]);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_delete()
            .once()
            .return_once(move || Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_owned(), workload_mock);

        let new_workload_state =
            generate_test_workload_state("workload A", ExecutionState::succeeded());

        runtime_manager
            .update_workload_state(new_workload_state)
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_update_workload_state_delete_workload_dependencies_not_fulfilled() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_update_workload_state()
            .once()
            .return_const(());

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_next_workloads_to_start()
            .once()
            .return_const(vec![]);

        mock_dependency_scheduler
            .expect_next_workloads_to_delete()
            .once()
            .return_const(vec![]);

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_delete().never();

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_owned(), workload_mock);

        let new_workload_state =
            generate_test_workload_state("workload A", ExecutionState::running());

        runtime_manager
            .update_workload_state(new_workload_state)
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_report_workload_state_waiting_to_start_for_waiting_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_put_on_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_owned(),
            WORKLOAD_1_NAME.to_owned(),
            RUNTIME_NAME.to_owned(),
        );
        let instance_name = workload.instance_name();

        let waiting_workloads = vec![workload];

        runtime_manager
            .enqueue_waiting_workloads(waiting_workloads)
            .await;

        let expected_update_workload_state_msg =
            ToServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name,
                    execution_state: ExecutionState::waiting_to_start(),
                    ..Default::default()
                }],
            });

        let to_server_msg = server_receiver.recv().await;
        assert_eq!(Some(expected_update_workload_state_msg), to_server_msg);
    }

    #[tokio::test]
    async fn utest_report_workload_state_waiting_to_stop_for_waiting_deleted_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_workload_state()
            .once()
            .with(
                predicate::eq(AGENT_NAME.to_owned()),
                predicate::eq(WORKLOAD_1_NAME.to_owned()),
            )
            .return_once(|_, _| Some(ExecutionState::running()));

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(deleted_workload.agent.clone())
            .workload_name(deleted_workload.name.clone())
            .config(&String::from("config"))
            .build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_instance_name()
            .once()
            .return_const(instance_name.clone());

        runtime_manager
            .workloads
            .insert(deleted_workload.name.clone(), workload_mock);

        let waiting_workloads = vec![deleted_workload];

        runtime_manager
            .enqueue_waiting_deleted_workloads(waiting_workloads)
            .await;

        let expected_update_workload_state_msg =
            ToServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name,
                    execution_state: ExecutionState::waiting_to_stop(),
                    ..Default::default()
                }],
            });

        let to_server_msg = server_receiver.recv().await;

        assert_eq!(Some(expected_update_workload_state_msg), to_server_msg);
    }

    #[tokio::test]
    async fn utest_report_no_workload_state_waiting_to_stop_for_not_running_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_workload_state()
            .once()
            .return_once(|_, _| Some(ExecutionState::succeeded()));

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_instance_name().never();

        runtime_manager
            .workloads
            .insert(deleted_workload.name.clone(), workload_mock);

        let waiting_workloads = vec![deleted_workload];

        runtime_manager
            .enqueue_waiting_deleted_workloads(waiting_workloads)
            .await;

        assert!(server_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn utest_report_no_workload_state_waiting_to_stop_for_not_existing_workload_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_workload_state()
            .once()
            .return_const(None);

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_instance_name().never();

        runtime_manager
            .workloads
            .insert(deleted_workload.name.clone(), workload_mock);

        let waiting_workloads = vec![deleted_workload];

        runtime_manager
            .enqueue_waiting_deleted_workloads(waiting_workloads)
            .await;

        assert!(server_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn utest_report_no_workload_state_waiting_to_stop_for_not_existing_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_workload_state()
            .once()
            .return_once(|_, _| Some(ExecutionState::running()));

        let parameter_storage_mock_context = MockParameterStorage::new_context();
        parameter_storage_mock_context
            .expect()
            .once()
            .return_once(|| parameter_storage_mock);

        let mut mock_dependency_scheduler = MockDependencyScheduler::default();
        mock_dependency_scheduler
            .expect_put_on_delete_waiting_queue()
            .once()
            .return_const(());

        let mock_dependency_scheduler_context = MockDependencyScheduler::new_context();
        mock_dependency_scheduler_context
            .expect()
            .once()
            .return_once(|| mock_dependency_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let waiting_workloads = vec![deleted_workload];

        runtime_manager
            .enqueue_waiting_deleted_workloads(waiting_workloads)
            .await;

        assert!(server_receiver.try_recv().is_err());
    }
}
