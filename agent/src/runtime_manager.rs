use std::{collections::HashMap, path::PathBuf};

use common::objects::{AgentName, DeletedWorkload, WorkloadExecutionInstanceName, WorkloadSpec};

use crate::{workload::Workload, workload_factory::WorkloadFactory};

pub struct RuntimeManager {
    agent_name: AgentName,
    run_folder: PathBuf,
    initial_workload_list_received: bool,
    workloads: HashMap<String, Workload>,
    workload_factory_map: HashMap<String, Box<dyn WorkloadFactory>>,
}

impl RuntimeManager {
    pub fn new(
        agent_name: AgentName,
        run_folder: PathBuf,
        workload_factory_map: HashMap<String, Box<dyn WorkloadFactory>>,
    ) -> Self {
        RuntimeManager {
            agent_name,
            run_folder,
            initial_workload_list_received: false,
            workloads: HashMap::new(),
            workload_factory_map,
        }
    }

    pub async fn handle_update_workload(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) {
        for workload in deleted_workloads {
            if let Some(x) = self.workloads.remove(&workload.name) {
                if let Err(err) = x.delete().await {
                    log::error!("Failed to delete workload '{}': '{}'", workload.name, err);
                }
            } else {
                log::warn!("Workload '{}' already gone.", workload.name);
            }
        }

        for workload_spec in added_workloads {
            let workload_name = workload_spec.workload.name.clone();
            let workload_instance_name = WorkloadExecutionInstanceName::builder()
                .workload_name(workload_name.clone())
                .agent_name(self.agent_name.get())
                .config(&workload_spec.workload.runtime_config)
                .build();
            let workload = self
                .workload_factory_map
                .get(&workload_spec.runtime)
                .unwrap() // TODO
                .create_workload(workload_instance_name, workload_spec.workload);
            self.workloads.insert(workload_name, workload);
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

        // // [impl->swdd~agent-starts-runtimes-adapters-with-initial-workloads~1]
        // async fn handle_initial_update_workload(&mut self, workload_spec_vec: Vec<WorkloadSpec>) {
        //     log::debug!("Handling initial workload list.");

        //     // create a list per runtime
        //     let mut runtime_workload_map: HashMap<String, Vec<WorkloadSpec>> = HashMap::new();
        //     for workload_spec in workload_spec_vec {
        //         if let Some(workload_vec) = runtime_workload_map.get_mut(&workload_spec.runtime) {
        //             workload_vec.push(workload_spec);
        //         } else {
        //             runtime_workload_map.insert(workload_spec.runtime.clone(), vec![workload_spec]);
        //         }
        //     }

        //     for (runtime, workload_vec) in runtime_workload_map {
        //         if let Some(runtime_adapter) = self.adapter_map.get_mut(runtime.as_str()) {
        //             // [impl->swdd~agent-manager-stores-workload-runtime-mapping~1]
        //             workload_vec.iter().for_each(|workload_spec| {
        //                 self.parameter_storage.set_workload_runtime(workload_spec)
        //             });

        //             runtime_adapter.start(&self.agent_name, workload_vec).await;
        //         } else {
        //             log::warn!(
        //                 "Could not find runtime '{}'. Workload list '{:?}' not scheduled.",
        //                 runtime,
        //                 workload_vec
        //             );
        //         }
        //     }
        // }

        // async fn handle_update_workload(
        //     &mut self,
        //     added_workloads: Vec<WorkloadSpec>,
        //     deleted_workloads: Vec<DeletedWorkload>,
        // ) {
        //     // transform into a hashmap to be able to search for updates
        //     // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
        //     let mut added_workloads: HashMap<String, WorkloadSpec> = added_workloads
        //         .into_iter()
        //         .map(|item| (item.workload.name.to_string(), item))
        //         .collect();

        //     // [impl->swdd~agent-handle-deleted-before-added-workloads~1]
        //     for deleted_workload in deleted_workloads {
        //         if let Some(updated_workload) = added_workloads.remove(&deleted_workload.name) {
        //             // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
        //             self.update_workload(updated_workload).await;
        //         } else {
        //             self.delete_workload(deleted_workload).await;
        //         }
        //     }

        //     for (_, workload) in added_workloads {
        //         let workload_name = &workload.workload.name;
        //         if self
        //             .parameter_storage
        //             .get_workload_runtime(workload_name)
        //             .is_some()
        //         {
        //             // We know this workload, seems the server is sending it again, try an update
        //             // [impl->swdd~agent-update-on-add-known-workload~1]
        //             self.update_workload(workload).await;
        //         } else {
        //             // [impl->swdd~agent-forwards-start-workload~1]
        //             self.add_workload(workload).await;
        //         }
        //     }
        // }

        // async fn add_workload(&mut self, workload_spec: WorkloadSpec) {
        //     if let Some(runtime_adapter) = self.adapter_map.get_mut(workload_spec.runtime.as_str()) {
        //         // [impl->swdd~agent-manager-stores-workload-runtime-mapping~1]
        //         self.parameter_storage.set_workload_runtime(&workload_spec);

        //         runtime_adapter.add_workload(workload_spec);
        //     } else {
        //         log::warn!(
        //             "Could not find runtime '{}'. Workload '{}' not scheduled.",
        //             workload_spec.runtime,
        //             workload_spec.workload.name
        //         );
        //     }
        // }

        // // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
        // async fn update_workload(&mut self, workload_spec: WorkloadSpec) {
        //     if let Some(runtime_adapter) = self.adapter_map.get_mut(workload_spec.runtime.as_str()) {
        //         // [impl->swdd~agent-manager-stores-workload-runtime-mapping~1]
        //         self.parameter_storage.set_workload_runtime(&workload_spec);

        //         runtime_adapter.update_workload(workload_spec).await;
        //     } else {
        //         log::warn!(
        //             "Could not find runtime '{}'. Workload '{}' not scheduled.",
        //             workload_spec.runtime,
        //             workload_spec.workload.name
        //         );
        //     }
        // }

        // async fn delete_workload(&mut self, deleted_workload: DeletedWorkload) {
        //     // [impl->swdd~agent-manager-deletes-control-interface~1]
        //     self.delete_control_interface(&deleted_workload.name);

        //     // [impl->swdd~agent-skips-unknown-runtime~1]
        //     if let Some(runtime_name) = self
        //         .parameter_storage
        //         .get_workload_runtime(&deleted_workload.name)
        //     {
        //         if let Some(runtime_adapter) = self.adapter_map.get_mut(runtime_name.as_str()) {
        //             // [impl->swdd~agent-uses-runtime-adapter~1]
        //             // [impl->swdd~agent-manager-forwards-delete-workload~2]
        //             runtime_adapter
        //                 .delete_workload(&deleted_workload.name)
        //                 .await;
        //         } else {
        //             log::error!(
        //                 "Agent '{}' is in an inconsistent state. No object found for runtime '{}'.",
        //                 self.agent_name,
        //                 runtime_name,
        //             );
        //         }

        //         // [impl->swdd~agent-manager-deletes-workload-runtime-mapping~1]
        //         self.parameter_storage
        //             .delete_workload_runtime(&deleted_workload.name);
        //     } else {
        //         log::warn!(
        //             "Agent '{}' cannot delete workload '{}'. No runtime found.",
        //             self.agent_name,
        //             deleted_workload.name
        //         );
        //     }
        // }
    }
}
