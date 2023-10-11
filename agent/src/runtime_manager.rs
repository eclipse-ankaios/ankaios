use std::{collections::HashMap, path::PathBuf};

use common::{
    commands::CompleteState,
    objects::{
        AgentName, DeletedWorkload, WorkloadExecutionInstanceName, WorkloadInstanceName,
        WorkloadSpec,
    },
    request_id_prepending::detach_prefix_from_request_id,
    state_change_interface::StateChangeSender,
};

use crate::{
    control_interface::PipesChannelContext, runtime_facade::RuntimeFacade, workload::Workload,
};

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

    // // [impl->swdd~agent-starts-runtimes-adapters-with-initial-workloads~1]
    async fn handle_initial_update_workload(&mut self, added_workload_vec: Vec<WorkloadSpec>) {
        log::debug!("Handling initial workload list.");

        // create a list per runtime
        let mut runtime_workload_map: HashMap<String, HashMap<String, WorkloadSpec>> =
            HashMap::new();
        for workload_spec in added_workload_vec {
            if let Some(workload_map) = runtime_workload_map.get_mut(&workload_spec.runtime) {
                workload_map.insert(workload_spec.name.clone(), workload_spec);
            } else {
                runtime_workload_map.insert(
                    workload_spec.runtime.clone(),
                    HashMap::from([(workload_spec.name.clone(), workload_spec)]),
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
                            let new_instance_name: WorkloadExecutionInstanceName =
                                new_workload_spec.instance_name();
                            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
                            let control_interface =
                                self.create_control_interface(&new_workload_spec);
                            // We have a running workload that matches a new added workload; check if the config is updated
                            self.workloads.insert(
                                new_workload_spec.name.to_string(),
                                if new_instance_name == running_instance_name {
                                    runtime.resume_workload(
                                        new_workload_spec,
                                        control_interface,
                                        &self.update_state_tx,
                                    )
                                } else {
                                    runtime.replace_workload(
                                        running_instance_name,
                                        new_workload_spec,
                                        control_interface,
                                        &self.update_state_tx,
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
            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
            let control_interface = self.create_control_interface(&workload_spec);
            self.add_workload(workload_spec, control_interface).await;
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
            .map(|item| (item.name.to_string(), item))
            .collect();

        // [impl->swdd~agent-handle-deleted-before-added-workloads~1]
        for deleted_workload in deleted_workloads {
            if let Some(updated_workload) = added_workloads.remove(&deleted_workload.name) {
                // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
                let control_interface = self.create_control_interface(&updated_workload);
                // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
                self.update_workload(updated_workload, control_interface)
                    .await;
            } else {
                self.delete_workload(deleted_workload).await;
            }
        }

        for (_, workload_spec) in added_workloads {
            let workload_name = &workload_spec.name;
            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
            let control_interface = self.create_control_interface(&workload_spec);
            if self.workloads.get(workload_name).is_some() {
                log::warn!(
                    "Added workload '{}' already exists. Updating.",
                    workload_name
                );
                // We know this workload, seems the server is sending it again, try an update
                // [impl->swdd~agent-update-on-add-known-workload~1]
                self.update_workload(workload_spec, control_interface).await;
            } else {
                // [impl->swdd~agent-forwards-start-workload~1]
                self.add_workload(workload_spec, control_interface).await;
            }
        }
    }

    async fn add_workload(
        &mut self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
    ) {
        let workload_name = workload_spec.name.clone();

        // [impl->swdd~agent-skips-unknown-runtime~1]
        if let Some(runtime) = self.runtime_map.get(&workload_spec.runtime) {
            let workload =
                runtime.create_workload(workload_spec, control_interface, &self.update_state_tx);
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
    async fn update_workload(
        &mut self,
        workload_spec: WorkloadSpec,
        control_interface: Option<PipesChannelContext>,
    ) {
        let workload_name = workload_spec.name.clone();
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
            if let Err(err) = workload.update(workload_spec, control_interface).await {
                log::error!("Failed to update workload '{}': '{}'", workload_name, err);
            }
        } else {
            log::warn!(
                "Workload for update '{}' not found. Recreating.",
                workload_name
            );
            // TODO: we need a requirement here
            self.add_workload(workload_spec, control_interface).await;
        }
    }

    // [impl->swdd~agent-create-control-interface-pipes-per-workload~1]
    fn create_control_interface(
        &self,
        workload_spec: &WorkloadSpec,
    ) -> Option<PipesChannelContext> {
        log::debug!("Creating control interface pipes for '{:?}'", workload_spec);

        match PipesChannelContext::new(
            &self.run_folder,
            &workload_spec.instance_name(),
            self.control_interface_tx.clone(),
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
