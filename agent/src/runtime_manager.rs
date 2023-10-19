use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use common::{
    commands::CompleteState,
    objects::{
        AgentName, DeletedWorkload, WorkloadExecutionInstanceName, WorkloadInstanceName,
        WorkloadSpec,
    },
    request_id_prepending::detach_prefix_from_request_id,
    state_change_interface::StateChangeSender,
};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::PipesChannelContext;

use crate::runtime_facade::RuntimeFacade;

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
    control_interface_tx: StateChangeSender,
    initial_workload_list_received: bool,
    workloads: HashMap<String, Workload>,
    // [impl->swdd~agent-supports-multiple-runtime-connectors~1]
    runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
    update_state_tx: StateChangeSender,
}

#[cfg_attr(test, automock)]
impl RuntimeManager {
    pub fn new(
        agent_name: AgentName,
        run_folder: PathBuf,
        control_interface_tx: StateChangeSender,
        runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
        update_state_tx: StateChangeSender,
    ) -> Self {
        RuntimeManager {
            agent_name,
            run_folder,
            control_interface_tx,
            initial_workload_list_received: false,
            workloads: HashMap::new(),
            runtime_map,
            update_state_tx,
        }
    }

    pub async fn handle_update_workload(
        &mut self,
        added_workloads_vec: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) {
        if !self.initial_workload_list_received {
            self.initial_workload_list_received = true;
            if !deleted_workloads.is_empty() {
                log::error!(
                    "Received an initial workload list with delete workload commands: '{:?}'",
                    deleted_workloads
                );
            }

            // [impl->swdd~agent-initial-list-existing-workloads~1]
            self.handle_initial_update_workload(added_workloads_vec)
                .await;
        } else {
            self.handle_subsequent_update_workload(added_workloads_vec, deleted_workloads)
                .await;
        }
    }

    pub async fn forward_complete_state(&mut self, method_obj: CompleteState) {
        // [impl->swdd~agent-uses-id-prefix-forward-control-interface-response-correct-workload~1]
        // [impl->swdd~agent-remove-id-prefix-forwarding-control-interface-response~1]
        let (workload_name, request_id) = detach_prefix_from_request_id(&method_obj.request_id);
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            let payload = CompleteState {
                request_id,
                ..method_obj
            };

            if let Err(err) = workload.send_complete_state(payload).await {
                log::warn!(
                    "Could not forward complete state to workload '{}': '{}'",
                    workload_name,
                    err
                );
            }
        } else {
            log::warn!(
                "Could not forward complete state for unknown workload: '{}'",
                workload_name
            );
        }
    }

    // [impl->swdd~agent-initial-list-existing-workloads~1]
    async fn handle_initial_update_workload(&mut self, added_workload_vec: Vec<WorkloadSpec>) {
        log::debug!("Handling initial workload list.");

        // create a list per runtime
        let mut added_workloads_per_runtime: HashMap<String, HashMap<String, WorkloadSpec>> =
            HashMap::new();
        for workload_spec in added_workload_vec {
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

        // Go through each runtime and find the still running workloads
        // [impl->swdd~agent-existing-workloads-finds-list~1]
        for (runtime_name, runtime) in &self.runtime_map {
            match runtime
                .get_reusable_running_workloads(&self.agent_name)
                .await
            {
                Ok(running_instance_name_vec) => {
                    for instance_name in running_instance_name_vec {
                        if let Some(new_workload_spec) = added_workloads_per_runtime
                            .get_mut(runtime_name)
                            .and_then(|map| map.remove(instance_name.workload_name()))
                        {
                            let new_instance_name: WorkloadExecutionInstanceName =
                                new_workload_spec.instance_name();
                            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
                            let control_interface = Self::create_control_interface(
                                &self.run_folder,
                                self.control_interface_tx.clone(),
                                &new_workload_spec,
                            );
                            // We have a running workload that matches a new added workload; check if the config is updated
                            // [impl->swdd~agent-stores-running-workload~1]
                            self.workloads.insert(
                                new_workload_spec.name.to_string(),
                                if new_instance_name == instance_name {
                                    // [impl->swdd~agent-existing-workloads-resume-existing~1]
                                    runtime.resume_workload(
                                        new_workload_spec,
                                        control_interface,
                                        &self.update_state_tx,
                                    )
                                } else {
                                    // [impl->swdd~agent-existing-workloads-replace-updated~1]
                                    runtime.replace_workload(
                                        instance_name,
                                        new_workload_spec,
                                        control_interface,
                                        &self.update_state_tx,
                                    )
                                },
                            );
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

        // now start all workloads that did not exist
        for workload_spec in flatten(added_workloads_per_runtime) {
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
        control_interface_tx: StateChangeSender,
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
    use crate::runtime_facade::MockRuntimeFacade;
    use crate::workload::MockWorkload;
    use crate::{control_interface::MockPipesChannelContext, runtime::RuntimeError};
    use common::objects::WorkloadExecutionInstanceNameBuilder;
    use common::test_utils::{generate_test_complete_state, generate_test_deleted_workload};
    use common::{
        state_change_interface::StateChangeCommand,
        test_utils::generate_test_workload_spec_with_param,
    };
    use mockall::{predicate, Sequence};
    use tokio::sync::mpsc::{channel, Receiver, Sender};

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

        pub fn build(self) -> (Receiver<StateChangeCommand>, RuntimeManager) {
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
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_handle() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let pipes_channel_mock = MockPipesChannelContext::new_context();
        pipes_channel_mock
            .expect()
            .times(2)
            .returning(move |_, _, _| Ok(MockPipesChannelContext::default()));

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock
            .expect_create_workload()
            .times(2)
            .returning(move |_, _, _| MockWorkload::default());

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads_vec = vec![
            generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_1_NAME.to_string(),
                RUNTIME_NAME.to_string(),
            ),
            generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                WORKLOAD_2_NAME.to_string(),
                RUNTIME_NAME.to_string(),
            ),
        ];
        runtime_manager
            .handle_update_workload(added_workloads_vec, vec![])
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

        let added_workloads_vec = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            "unknown_runtime1".to_string(),
        )];
        runtime_manager
            .handle_update_workload(added_workloads_vec, vec![])
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

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_running_workloads()
            .once()
            .returning(|_| {
                Box::pin(async {
                    Err(RuntimeError::Update(
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

        let added_workloads_vec = vec![generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        )];
        runtime_manager
            .handle_update_workload(added_workloads_vec, vec![])
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

        runtime_facade_mock
            .expect_create_workload()
            .never();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads_vec = vec![existing_workload1];
        runtime_manager
            .handle_update_workload(added_workloads_vec, vec![])
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

        let added_workloads_vec = vec![existing_workload];
        runtime_manager
            .handle_update_workload(added_workloads_vec, vec![])
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

        // workload is in added and deleted workload vec
        runtime_manager
            .handle_update_workload(
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
                vec![generate_test_deleted_workload(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                )],
            )
            .await;

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

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

        runtime_manager
            .handle_update_workload(
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_2_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
                vec![generate_test_deleted_workload(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                )],
            )
            .await;
        server_receiver.close();

        assert!(!runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
        assert!(runtime_manager.workloads.contains_key(WORKLOAD_2_NAME));
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

        runtime_manager
            .handle_update_workload(
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
                vec![],
            )
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

        runtime_manager.initial_workload_list_received = true;

        runtime_manager
            .handle_update_workload(
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
                vec![],
            )
            .await;
        server_receiver.close();

        assert!(runtime_manager.workloads.contains_key(WORKLOAD_1_NAME));
    }

    #[tokio::test]
    async fn utest_forward_complete_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let runtime_facade_mock = MockRuntimeFacade::new();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_send_complete_state()
            .once()
            .withf(|complete_state| {
                complete_state.request_id == REQUEST_ID
                    && complete_state
                        .workload_states
                        .first()
                        .unwrap()
                        .workload_name
                        == WORKLOAD_1_NAME
            })
            .return_once(move |_| Ok(()));

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), mock_workload);

        runtime_manager
            .forward_complete_state(generate_test_complete_state(
                format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
            ))
            .await;
    }

    #[tokio::test]
    async fn utest_forward_complete_state_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let runtime_facade_mock = MockRuntimeFacade::new();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_send_complete_state()
            .once()
            .withf(|complete_state| {
                complete_state.request_id == REQUEST_ID
                    && complete_state
                        .workload_states
                        .first()
                        .unwrap()
                        .workload_name
                        == WORKLOAD_1_NAME
            })
            .return_once(move |_| {
                Err(RuntimeError::CompleteState(
                    "failed to send complete state".to_string(),
                ))
            });

        runtime_manager
            .workloads
            .insert(WORKLOAD_1_NAME.to_string(), mock_workload);

        runtime_manager
            .forward_complete_state(generate_test_complete_state(
                format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
            ))
            .await;
    }

    #[tokio::test]
    async fn utest_forward_complete_state_not_called_because_workload_not_found() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let runtime_facade_mock = MockRuntimeFacade::new();

        let (_, mut runtime_manager) = RuntimeManagerBuilder::default()
            .with_runtime(
                RUNTIME_NAME,
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload.expect_send_complete_state().never();

        runtime_manager
            .forward_complete_state(generate_test_complete_state(
                format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
                vec![generate_test_workload_spec_with_param(
                    AGENT_NAME.to_string(),
                    WORKLOAD_1_NAME.to_string(),
                    RUNTIME_NAME.to_string(),
                )],
            ))
            .await;
    }
}
