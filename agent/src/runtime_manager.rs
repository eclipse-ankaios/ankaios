use std::{collections::HashMap, path::PathBuf};

use common::objects::{AgentName, DeletedWorkload, WorkloadExecutionInstanceName, WorkloadSpec};

use crate::{runtime_facade::RuntimeFacade, workload::Workload};

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
    initial_workload_list_received: bool,
    workloads: HashMap<String, Workload>,
    runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
}

impl RuntimeManager {
    pub fn new(
        agent_name: AgentName,
        run_folder: PathBuf,
        runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
    ) -> Self {
        RuntimeManager {
            agent_name,
            run_folder,
            initial_workload_list_received: false,
            workloads: HashMap::new(),
            runtime_map,
        }
    }

    pub async fn handle_update_workload(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) {
        if !self.initial_workload_list_received {
            // [impl->swdd~agent-starts-runtimes-adapters-with-initial-workloads~1]
            self.initial_workload_list_received = true;

            if !deleted_workloads.is_empty() {
                log::error!(
                    "Received an initial workload list with delete workload commands: '{:?}'",
                    deleted_workloads
                );
            }

            self.handle_initial_update_workload(added_workloads).await;
        } else {
            self.handle_subsequent_update_workload(added_workloads, deleted_workloads)
                .await;
        }
    }

    // // [impl->swdd~agent-starts-runtimes-adapters-with-initial-workloads~1]
    async fn handle_initial_update_workload(&mut self, added_workload_vec: Vec<WorkloadSpec>) {
        log::debug!("Handling initial workload list.");

        // create a list per runtime
        let mut runtime_workload_map: HashMap<String, HashMap<String, WorkloadSpec>> =
            HashMap::new();
        for workload_spec in added_workload_vec {
            if let Some(workload_map) = runtime_workload_map.get_mut(&workload_spec.runtime) {
                workload_map.insert(workload_spec.workload.name.clone(), workload_spec);
            } else {
                runtime_workload_map.insert(
                    workload_spec.runtime.clone(),
                    HashMap::from([(workload_spec.workload.name.clone(), workload_spec)]),
                );
            }
        }

        for (runtime_name, runtime) in &self.runtime_map {
            match runtime
                .get_reusable_running_workloads(&self.agent_name)
                .await
            {
                Ok(running_instance_name_vec) => {
                    for running_instance_name in running_instance_name_vec {
                        if let Some(new_workload_spec) = runtime_workload_map
                            .get_mut(runtime_name)
                            .and_then(|map| map.remove(running_instance_name.workload_name()))
                        {
                            // TODO: add control interface
                            let new_instance_name: WorkloadExecutionInstanceName =
                                (&new_workload_spec).into();
                            // We have a running workload that matches a new added workload; check if the config is updated
                            self.workloads.insert(
                                new_instance_name.workload_name().to_string(),
                                if new_instance_name == running_instance_name {
                                    runtime.resume_workload(
                                        running_instance_name,
                                        new_workload_spec.workload,
                                    )
                                } else {
                                    runtime.replace_workload(
                                        running_instance_name,
                                        new_instance_name,
                                        new_workload_spec.workload,
                                    )
                                },
                            );
                        } else {
                            // Do added workload matches the found running one => delete it
                            runtime.delete_workload(running_instance_name);
                        }
                    }
                }
                Err(err) => log::warn!("Could not get reusable running workloads: '{}'", err),
            }
        }

        // now start all workloads that did not exist
        for workload_spec in flatten(runtime_workload_map) {
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
            .map(|item| (item.workload.name.to_string(), item))
            .collect();

        // [impl->swdd~agent-handle-deleted-before-added-workloads~1]
        for deleted_workload in deleted_workloads {
            if let Some(updated_workload) = added_workloads.remove(&deleted_workload.name) {
                // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
                self.update_workload(updated_workload).await;
            } else {
                self.delete_workload(deleted_workload).await;
            }
        }

        for (_, workload) in added_workloads {
            let workload_name = &workload.workload.name;
            if self.workloads.get(workload_name).is_some() {
                log::warn!(
                    "Added workload '{}' already exists. Updating.",
                    workload_name
                );
                // We know this workload, seems the server is sending it again, try an update
                // [impl->swdd~agent-update-on-add-known-workload~1]
                self.update_workload(workload).await;
            } else {
                // [impl->swdd~agent-forwards-start-workload~1]
                self.add_workload(workload).await;
            }
        }
    }

    async fn add_workload(&mut self, added_workload: WorkloadSpec) {
        let workload_name = added_workload.workload.name.clone();
        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .workload_name(workload_name.clone())
            .agent_name(self.agent_name.get())
            .config(&added_workload.workload.runtime_config)
            .build();

        if let Some(runtime) = self.runtime_map.get(&added_workload.runtime) {
            // TODO create control interface; pipes shall be created by a different module for each workload that gets created.
            // Create a pipes channel context for each one of them
            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
            // self.create_control_interface(&method_obj.added_workloads);
            let workload = runtime.create_workload(workload_instance_name, added_workload.workload);
            self.workloads.insert(workload_name, workload);
        } else {
            log::warn!(
                "Could not find runtime '{}'. Workload '{}' not scheduled.",
                added_workload.runtime,
                workload_name
            );
        }
    }

    async fn delete_workload(&mut self, deleted_workload: DeletedWorkload) {
        //TODO
        // // [impl->swdd~agent-manager-deletes-control-interface~1]
        // self.delete_control_interface(&deleted_workload.name);

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
    async fn update_workload(&mut self, updated_workload: WorkloadSpec) {
        let workload_name = updated_workload.workload.name.clone();
        if let Some(workload) = self.workloads.get(&workload_name) {
            let workload_instance_name = WorkloadExecutionInstanceName::builder()
                .workload_name(workload_name.clone())
                .agent_name(self.agent_name.get())
                .config(&updated_workload.workload.runtime_config)
                .build();

            // TODO: control interface

            if let Err(err) = workload
                .update(workload_instance_name, updated_workload.workload)
                .await
            {
                log::error!("Failed to update workload '{}': '{}'", workload_name, err);
            }
        } else {
            log::warn!(
                "Workload for update '{}' not found. Recreating.",
                workload_name
            );
            // TODO: we need a requirement here
            self.add_workload(updated_workload).await;
        }
    }

    // // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
    // fn create_control_interface(&mut self, workload_spec_vec: &Vec<WorkloadSpec>) {
    //     log::debug!(
    //         "Creating control interface pipes for '{:?}'",
    //         workload_spec_vec
    //     );
    //     for workload_spec in workload_spec_vec {
    //         if self
    //             .adapter_map
    //             .get(workload_spec.runtime.as_str())
    //             .is_none()
    //         {
    //             log::warn!(
    //                 "Skipping Control Interface creation for workload '{}': runtime '{}' unknown.",
    //                 workload_spec.workload.name,
    //                 workload_spec.runtime
    //             );
    //             continue;
    //         }

    //         if let Some(pipes_context) = self
    //             .workload_pipes_context_map
    //             .remove(&workload_spec.workload.name)
    //         {
    //             log::debug!(
    //                 "Replacing PipesChannelContext for workload '{}', old path: '{:?}'",
    //                 workload_spec.workload.name,
    //                 pipes_context.get_api_location()
    //             );
    //         }
    //         if let Ok(pipes_channel_context) = PipesChannelContext::new(
    //             &self.run_folder,
    //             &workload_spec.instance_name(),
    //             self._to_server.clone(),
    //         ) {
    //             self.workload_pipes_context_map
    //                 .insert(workload_spec.workload.name.clone(), pipes_channel_context);
    //         } else {
    //             log::warn!(
    //                 "Could not create pipes channel context for workload '{}'.",
    //                 workload_spec.workload.name
    //             );
    //         }
    //     }
    // }

    // // [impl->swdd~agent-manager-deletes-control-interface~1]
    // fn delete_control_interface(&mut self, deleted_workload_name: &str) {
    //     if let Some(pipes_context) = self
    //         .workload_pipes_context_map
    //         .remove(deleted_workload_name)
    //     {
    //         pipes_context.abort_pipes_channel_task();
    //     } else {
    //         log::error!(
    //             "Agent '{}' is in an inconsistent state. No pipes context found for workload '{}'.",
    //             self.agent_name,
    //             deleted_workload_name,
    //         );
    //     }
    // }
}
