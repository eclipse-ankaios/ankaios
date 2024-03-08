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
    objects::{AgentName, DeletedWorkload, WorkloadInstanceName, WorkloadSpec},
    request_id_prepending::detach_prefix_from_request_id,
    to_server_interface::ToServerSender,
};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;

#[cfg_attr(test, mockall_double::double)]
use crate::workload_scheduler::scheduler::WorkloadScheduler;

#[cfg_attr(test, mockall_double::double)]
use crate::parameter_storage::ParameterStorage;
use crate::{runtime_connectors::RuntimeFacade, workload_operation::WorkloadOperation};

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
    workload_queue: WorkloadScheduler,
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
            update_state_tx: update_state_tx.clone(),
            workload_queue: WorkloadScheduler::new(update_state_tx),
        }
    }

    // [impl->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
    pub async fn update_workloads_on_fulfilled_dependencies(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) {
        let workload_operations = self
            .workload_queue
            .next_workload_operations(workload_state_db)
            .await;

        if !workload_operations.is_empty() {
            self.execute_workload_operations(workload_operations).await;
        }
    }

    // [impl->swdd~agent-handles-update-workload-requests~1]
    pub async fn handle_update_workload(
        &mut self,
        mut added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
        workload_state_db: &ParameterStorage,
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
            added_workloads = self
                .resume_and_remove_from_added_workloads(added_workloads)
                .await;
        }

        let workload_operations: Vec<WorkloadOperation> =
            self.transform_into_workload_operations(added_workloads, deleted_workloads);

        // [impl->swdd~agent-enqueues-workload-operations-with-unfulfilled-dependencies~1]
        // [impl->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
        // [impl->swdd~agent-perform-update-delete-only~1]
        let ready_workload_operations = self
            .workload_queue
            .enqueue_filtered_workload_operations(workload_operations, workload_state_db)
            .await;

        self.execute_workload_operations(ready_workload_operations)
            .await;
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
    async fn resume_and_remove_from_added_workloads(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
    ) -> Vec<WorkloadSpec> {
        log::debug!("Handling initial workload list.");

        // create a list per runtime
        let mut added_workloads_per_runtime: HashMap<String, HashMap<String, WorkloadSpec>> =
            HashMap::new();
        for workload_spec in added_workloads {
            if let Some(workload_map) = added_workloads_per_runtime.get_mut(&workload_spec.runtime)
            {
                workload_map.insert(
                    workload_spec.instance_name.workload_name().to_owned(),
                    workload_spec,
                );
            } else {
                added_workloads_per_runtime.insert(
                    workload_spec.runtime.clone(),
                    HashMap::from([(
                        workload_spec.instance_name.workload_name().to_owned(),
                        workload_spec,
                    )]),
                );
            }
        }

        let mut new_added_workloads = Vec::new();
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
                            let new_instance_name: WorkloadInstanceName =
                                new_workload_spec.instance_name.clone();

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
                                    new_instance_name.workload_name().to_owned(),
                                    runtime.resume_workload(
                                        new_workload_spec,
                                        control_interface,
                                        &self.update_state_tx,
                                    ),
                                );
                            } else {
                                // [impl->swdd~agent-existing-workloads-replace-updated~1]

                                log::info!("Deleting existing workload '{}'. It is created when its dependencies are fulfilled.",
                                    instance_name.workload_name()
                                );

                                runtime.delete_workload(instance_name);
                                new_added_workloads.push(new_workload_spec);
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

        new_added_workloads.extend(flatten(added_workloads_per_runtime));

        new_added_workloads
    }

    // [impl->swdd~agent-transforms-update-workload-message-to-workload-operations~1]
    fn transform_into_workload_operations(
        &self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Vec<WorkloadOperation> {
        let mut workload_operations: Vec<WorkloadOperation> = Vec::new();
        // transform into a hashmap to be able to search for updates
        // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
        let mut added_workloads: HashMap<String, WorkloadSpec> = added_workloads
            .into_iter()
            .map(|workload_spec| {
                (
                    workload_spec.instance_name.workload_name().to_owned(),
                    workload_spec,
                )
            })
            .collect();

        // [impl->swdd~agent-handle-deleted-before-added-workloads~1]
        for deleted_workload in deleted_workloads {
            if let Some(updated_workload) =
                added_workloads.remove(deleted_workload.instance_name.workload_name())
            {
                // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
                workload_operations.push(WorkloadOperation::Update(
                    updated_workload,
                    deleted_workload,
                ));
            } else {
                // [impl->swdd~agent-deletes-workload~1]
                workload_operations.push(WorkloadOperation::Delete(deleted_workload));
            }
        }

        for (_, workload_spec) in added_workloads {
            let workload_name = workload_spec.instance_name.workload_name();
            if self.workloads.get(workload_name).is_some() {
                log::warn!(
                    "Added workload '{}' already exists. Updating without considering delete dependencies.",
                    workload_name
                );
                // We know this workload, seems the server is sending it again, try an update
                // [impl->swdd~agent-update-on-add-known-workload~1]
                let instance_name = workload_spec.instance_name.clone();
                workload_operations.push(WorkloadOperation::Update(
                    workload_spec,
                    DeletedWorkload {
                        instance_name,
                        dependencies: HashMap::default(),
                    },
                ));
            } else {
                // [impl->swdd~agent-added-creates-workload~1]
                workload_operations.push(WorkloadOperation::Create(workload_spec));
            }
        }

        workload_operations
    }

    async fn execute_workload_operations(&mut self, workload_operations: Vec<WorkloadOperation>) {
        for wl_operation in workload_operations {
            match wl_operation {
                WorkloadOperation::Create(workload_spec) => {
                    // [impl->swdd~agent-executes-create-workload-operation~1]
                    self.add_workload(workload_spec).await
                }
                WorkloadOperation::Update(new_workload_spec, _) => {
                    // [impl->swdd~agent-executes-update-workload-operation~1]
                    self.update_workload(new_workload_spec).await
                }
                WorkloadOperation::UpdateDeleteOnly(deleted_workload) => {
                    // [impl->swdd~agent-perform-update-delete-only~1]
                    self.update_delete_only(deleted_workload).await
                }
                WorkloadOperation::Delete(deleted_workload) => {
                    // [impl->swdd~agent-executes-delete-workload-operation~1]
                    self.delete_workload(deleted_workload).await
                }
            }
        }
    }

    async fn add_workload(&mut self, workload_spec: WorkloadSpec) {
        let workload_name = workload_spec.instance_name.workload_name().to_owned();

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
        if let Some(workload) = self
            .workloads
            .remove(deleted_workload.instance_name.workload_name())
        {
            if let Err(err) = workload.delete().await {
                log::error!(
                    "Failed to delete workload '{}': '{}'",
                    deleted_workload.instance_name.workload_name(),
                    err
                );
            }
        } else {
            log::warn!(
                "Workload '{}' already gone.",
                deleted_workload.instance_name.workload_name()
            );
        }
    }

    // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
    async fn update_workload(&mut self, workload_spec: WorkloadSpec) {
        let workload_name = workload_spec.instance_name.workload_name().to_owned();
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
            let control_interface = Self::create_control_interface(
                &self.run_folder,
                self.control_interface_tx.clone(),
                &workload_spec,
            );
            if let Err(err) = workload
                .update(Some(workload_spec), control_interface)
                .await
            {
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

    // [impl->swdd~agent-perform-update-delete-only~1]
    async fn update_delete_only(&mut self, deleted_workload: DeletedWorkload) {
        let workload_name = deleted_workload.instance_name.workload_name().to_owned();
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            if let Err(err) = workload.update(None, None).await {
                log::error!("Failed to update workload '{}': '{}'", workload_name, err);
            }
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
            &workload_spec.instance_name,
            control_interface_tx,
        ) {
            Ok(pipes_channel_context) => Some(pipes_channel_context),
            Err(err) => {
                log::warn!(
                    "Could not create pipes channel context for workload '{}'. Error: '{err}'",
                    workload_spec.instance_name
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
    use crate::parameter_storage::MockParameterStorage;
    use crate::runtime_connectors::{MockRuntimeFacade, RuntimeError};
    use crate::workload::{MockWorkload, WorkloadError};
    use crate::workload_scheduler::scheduler::MockWorkloadScheduler;
    use common::commands::ResponseContent;
    use common::objects::{
        generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
        AddCondition, WorkloadInstanceNameBuilder,
    };
    use common::test_utils::{
        generate_test_complete_state, generate_test_deleted_workload,
        generate_test_deleted_workload_with_dependencies,
    };
    use common::to_server_interface::ToServerReceiver;
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
    // [utest->swdd~agent-handles-update-workload-requests~1]
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

        let new_workload_1 = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let new_workload_2 = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_2_NAME.to_string(),
            RUNTIME_NAME_2.to_string(),
        );

        let added_workloads = vec![new_workload_1.clone(), new_workload_2.clone()];
        let workload_operations = vec![
            WorkloadOperation::Create(new_workload_1),
            WorkloadOperation::Create(new_workload_2),
        ];

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
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

        let workload_with_unknown_runtime = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            "unknown_runtime1".to_string(),
        );
        let added_workloads = vec![workload_with_unknown_runtime.clone()];

        let workload_operations = vec![WorkloadOperation::Create(workload_with_unknown_runtime)];

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
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

        let workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let added_workloads = vec![workload.clone()];

        let workload_operations = vec![WorkloadOperation::Create(workload)];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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
                workload_spec.instance_name.workload_name() == WORKLOAD_1_NAME
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

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
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

        let workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let existing_workload1 = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let existing_workload_instance_name = existing_workload1.instance_name.clone();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![existing_workload_instance_name]) }));

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

        let added_workloads = vec![existing_workload1];
        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
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

        let existing_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let added_workloads = vec![existing_workload.clone()];

        let workload_operations = vec![WorkloadOperation::Create(existing_workload)];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        // create workload with different config string to simulate a replace of a existing workload
        let existing_workload_with_other_config = WorkloadInstanceNameBuilder::default()
            .workload_name(WORKLOAD_1_NAME)
            .config(&String::from("different config"))
            .agent_name(AGENT_NAME)
            .build();

        let mut sequence = mockall::Sequence::new();
        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .in_sequence(&mut sequence)
            .return_once(|_| Box::pin(async { Ok(vec![existing_workload_with_other_config]) }));

        runtime_facade_mock
            .expect_delete_workload()
            .once()
            .in_sequence(&mut sequence)
            .return_const(());

        runtime_facade_mock
            .expect_create_workload()
            .once()
            .in_sequence(&mut sequence)
            .return_once(|_, _, _| MockWorkload::default());

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-existing-workloads-delete-unneeded~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_delete_unneeded() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_workload_with_other_config = WorkloadInstanceNameBuilder::default()
            .workload_name(WORKLOAD_1_NAME)
            .config(&String::from("different config"))
            .agent_name(AGENT_NAME)
            .build();

        let existing_instance_name_clone = existing_workload_with_other_config.clone();

        let workload_operations = vec![WorkloadOperation::Delete(DeletedWorkload {
            instance_name: existing_instance_name_clone,
            ..Default::default()
        })];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| {
                Box::pin(async move { Ok(vec![existing_workload_with_other_config]) })
            });

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

        runtime_manager
            .handle_update_workload(vec![], vec![], &MockParameterStorage::default())
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.is_empty());
    }

    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_add_workload_with_dependencies() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let added_workloads = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];

        let workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));
        runtime_facade_mock.expect_create_workload().never();

        let (_server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(runtime_manager.workloads.is_empty());
    }

    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_replace_workload_with_not_fulfilled_dependencies(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock.expect().never();

        let workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let existing_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        // create workload with different config string to simulate a replace of a existing workload
        let existing_workload_with_other_config = WorkloadInstanceName::builder()
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

        runtime_facade_mock.expect_create_workload().never();

        let (_server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![existing_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
            .await;

        assert!(runtime_manager.initial_workload_list_received);
        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-updates-deleted-and-added-workloads~1]
    // [utest->swdd~agent-handles-update-workload-requests~1]
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

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let old_workload =
            generate_test_deleted_workload(AGENT_NAME.to_string(), WORKLOAD_1_NAME.to_string());

        let workload_operations = vec![WorkloadOperation::Update(
            new_workload.clone(),
            old_workload.clone(),
        )];

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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
                predicate::function(|workload_spec: &Option<WorkloadSpec>| {
                    workload_spec.is_some()
                        && workload_spec
                            .as_ref()
                            .unwrap()
                            .instance_name
                            .workload_name()
                            == WORKLOAD_1_NAME
                }),
                predicate::function(|control_interface: &Option<PipesChannelContext>| {
                    control_interface.is_some()
                }),
            )
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![old_workload];
        // workload is in added and deleted workload vec
        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &MockParameterStorage::default(),
            )
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

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_2_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_string(), WORKLOAD_1_NAME.to_string());

        let workload_operations = vec![
            WorkloadOperation::Delete(deleted_workload.clone()),
            WorkloadOperation::Create(new_workload.clone()),
        ];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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
                workload_spec.instance_name.workload_name() == WORKLOAD_2_NAME
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

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![deleted_workload];

        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &MockParameterStorage::default(),
            )
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

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_string(), WORKLOAD_1_NAME.to_string());

        let workload_operations = vec![WorkloadOperation::Create(new_workload.clone())];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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
        runtime_manager.initial_workload_list_received = true;

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![deleted_workload];
        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &MockParameterStorage::default(),
            )
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

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let old_workload = generate_test_deleted_workload_with_dependencies(
            AGENT_NAME.to_owned(),
            WORKLOAD_1_NAME.to_owned(),
            Default::default(),
        );

        let workload_operations = vec![WorkloadOperation::Update(
            new_workload.clone(),
            old_workload,
        )];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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
                workload_spec.is_some()
                    && workload_spec
                        .as_ref()
                        .unwrap()
                        .instance_name
                        .workload_name()
                        == WORKLOAD_1_NAME
                    && control_interface.is_some()
            })
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
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

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let workload_operations = vec![WorkloadOperation::Create(new_workload.clone())];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .withf(|workload_spec, control_interface, to_server| {
                workload_spec.instance_name.workload_name() == WORKLOAD_1_NAME
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

        runtime_manager.initial_workload_list_received = true;

        let added_workloads = vec![new_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_add_workload_with_not_fulfilled_dependencies()
    {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock.expect_create_workload().never();

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

        runtime_manager.initial_workload_list_received = true;

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &MockParameterStorage::default())
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-perform-update-delete-only~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_update_delete_only_with_fulfilled_delete_dependencies(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let old_workload =
            generate_test_deleted_workload(AGENT_NAME.to_string(), WORKLOAD_1_NAME.to_string());

        let workload_operations = vec![WorkloadOperation::UpdateDeleteOnly(old_workload.clone())];

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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
                predicate::eq(None), // in case of update delete only there is no new workload spec
                predicate::function(|control_interface: &Option<PipesChannelContext>| {
                    control_interface.is_none()
                }),
            )
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![old_workload];
        // workload is in added and deleted workload vec
        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &MockParameterStorage::default(),
            )
            .await;

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_deleted_workload_with_not_fulfilled_dependencies(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .once()
            .return_const(workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        runtime_manager.initial_workload_list_received = true;

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_delete().never();

        let new_deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_string(), WORKLOAD_1_NAME.to_string());

        runtime_manager.workloads.insert(
            new_deleted_workload
                .instance_name
                .workload_name()
                .to_owned(),
            workload_mock,
        );

        let deleted_workloads = vec![new_deleted_workload.clone()];
        runtime_manager
            .handle_update_workload(vec![], deleted_workloads, &MockParameterStorage::default())
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

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

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

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

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

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

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

    // [utest->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
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

        let next_workload_operations = vec![WorkloadOperation::Create(
            generate_test_workload_spec_with_dependencies(
                AGENT_NAME,
                WORKLOAD_1_NAME,
                RUNTIME_NAME,
                HashMap::from([(WORKLOAD_2_NAME.to_string(), AddCondition::AddCondRunning)]),
            ),
        )];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_next_workload_operations()
            .once()
            .return_const(next_workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

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

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&MockParameterStorage::default())
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_update_workload_state_no_create_workload_when_dependencies_not_fulfilled() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let next_workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_next_workload_operations()
            .once()
            .return_const(next_workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock.expect_create_workload().never();

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&MockParameterStorage::default())
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_update_workload_state_delete_workload_dependencies_with_fulfilled_dependencies()
    {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let next_workload_operations = vec![WorkloadOperation::Delete(deleted_workload)];

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_next_workload_operations()
            .once()
            .return_const(next_workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_delete()
            .once()
            .return_once(move || Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_owned(), workload_mock);

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&MockParameterStorage::default())
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_update_workload_state_delete_workload_dependencies_not_fulfilled() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let next_workload_operations = vec![];
        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_next_workload_operations()
            .once()
            .return_const(next_workload_operations);

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let (mut server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_delete().never();

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_owned(), workload_mock);

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&MockParameterStorage::default())
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    // [utest->swdd~agent-transforms-update-workload-message-to-workload-operations~1]
    #[tokio::test]
    async fn utest_transform_update_state_message_into_workload_operations_create() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, runtime_manager) = RuntimeManagerBuilder::default().build();

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_owned(),
            WORKLOAD_1_NAME.to_owned(),
            RUNTIME_NAME.to_owned(),
        );
        let added_workloads = vec![new_workload.clone()];
        let deleted_workloads = vec![];
        let workload_operations =
            runtime_manager.transform_into_workload_operations(added_workloads, deleted_workloads);

        assert_eq!(
            vec![WorkloadOperation::Create(new_workload)],
            workload_operations
        );
    }

    // [utest->swdd~agent-transforms-update-workload-message-to-workload-operations~1]
    #[tokio::test]
    async fn utest_transform_update_state_message_into_workload_operations_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, runtime_manager) = RuntimeManagerBuilder::default().build();
        let added_workloads = vec![];
        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());
        let deleted_workloads = vec![deleted_workload.clone()];
        let workload_operations =
            runtime_manager.transform_into_workload_operations(added_workloads, deleted_workloads);

        assert_eq!(
            vec![WorkloadOperation::Delete(deleted_workload)],
            workload_operations
        );
    }

    // [utest->swdd~agent-transforms-update-workload-message-to-workload-operations~1]
    #[tokio::test]
    async fn utest_transform_update_state_message_into_workload_operations_update() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, runtime_manager) = RuntimeManagerBuilder::default().build();

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_owned(),
            WORKLOAD_1_NAME.to_owned(),
            RUNTIME_NAME.to_owned(),
        );
        let added_workloads = vec![new_workload.clone()];
        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());
        let deleted_workloads = vec![deleted_workload.clone()];
        let workload_operations =
            runtime_manager.transform_into_workload_operations(added_workloads, deleted_workloads);

        assert_eq!(
            vec![WorkloadOperation::Update(new_workload, deleted_workload)],
            workload_operations
        );
    }

    // [utest->swdd~agent-executes-create-workload-operation~1]
    #[tokio::test]
    async fn utest_execute_workload_operations_create() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        let (_server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_owned(),
            WORKLOAD_1_NAME.to_owned(),
            RUNTIME_NAME.to_owned(),
        );
        let workload_operations = vec![WorkloadOperation::Create(new_workload)];
        runtime_manager
            .execute_workload_operations(workload_operations)
            .await;
    }

    // [utest->swdd~agent-executes-delete-workload-operation~1]
    #[tokio::test]
    async fn utest_execute_workload_operations_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_delete()
            .once()
            .return_once(move || Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());
        let workload_operations = vec![WorkloadOperation::Delete(deleted_workload)];
        runtime_manager
            .execute_workload_operations(workload_operations)
            .await;
    }

    // [utest->swdd~agent-perform-update-delete-only~1]
    #[tokio::test]
    async fn utest_execute_workload_operations_update_delete_only() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let workload_operations = vec![WorkloadOperation::UpdateDeleteOnly(deleted_workload)];
        runtime_manager
            .execute_workload_operations(workload_operations)
            .await;
    }

    // [utest->swdd~agent-executes-update-workload-operation~1]
    #[tokio::test]
    async fn utest_execute_workload_operations_update() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, mut runtime_manager) = RuntimeManagerBuilder::default().build();

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), workload_mock);

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_NAME.to_owned(),
            WORKLOAD_1_NAME.to_owned(),
            RUNTIME_NAME.to_owned(),
        );

        let deleted_workload =
            generate_test_deleted_workload(AGENT_NAME.to_owned(), WORKLOAD_1_NAME.to_owned());

        let workload_operations = vec![WorkloadOperation::Update(new_workload, deleted_workload)];
        runtime_manager
            .execute_workload_operations(workload_operations)
            .await;
    }
}
