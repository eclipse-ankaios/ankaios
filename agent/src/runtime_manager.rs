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

use crate::{
    control_interface::ControlInterfacePath,
    runtime_connectors::{
        LogRequestOptions, log_fetcher::LogFetcher, unsupported_runtime::UnsupportedRuntime,
    },
};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::authorizer::Authorizer;

use ankaios_api::ank_base::{
    DeletedWorkload, ExecutionStateSpec, LogsRequestSpec, Response, WorkloadInstanceNameSpec,
    WorkloadNamed, WorkloadStateSpec, WorkloadStatesMapSpec,
};
use common::{
    objects::AgentName, request_id_prepending::detach_prefix_from_request_id,
    to_server_interface::ToServerSender,
};

use std::{collections::HashMap, path::PathBuf};

#[cfg_attr(test, mockall_double::double)]
use crate::control_interface::control_interface_info::ControlInterfaceInfo;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_connectors::GenericRuntimeFacade;

#[cfg_attr(test, mockall_double::double)]
use crate::workload_scheduler::scheduler::WorkloadScheduler;

use crate::{
    runtime_connectors::RuntimeFacade,
    workload_operation::{ReusableWorkload, WorkloadOperation},
    workload_state::{WorkloadStateSender, WorkloadStateSenderInterface},
};

#[cfg_attr(test, mockall_double::double)]
use crate::workload::Workload;

#[cfg(test)]
use mockall::automock;

fn flatten(mut runtime_workload_map: HashMap<String, Vec<WorkloadNamed>>) -> Vec<ReusableWorkload> {
    runtime_workload_map
        .drain()
        .flat_map(|(_, v)| {
            v.into_iter()
                .map(|y| ReusableWorkload::new(y, None))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub trait ToReusableWorkloadSpecs {
    fn into_reusable_workloads(self) -> Vec<ReusableWorkload>;
}

impl ToReusableWorkloadSpecs for Vec<WorkloadNamed> {
    fn into_reusable_workloads(self) -> Vec<ReusableWorkload> {
        self.into_iter()
            .map(|w| ReusableWorkload::new(w, None))
            .collect()
    }
}

pub struct RuntimeManager {
    agent_name: AgentName,
    run_folder: PathBuf,
    control_interface_tx: ToServerSender,
    workloads: HashMap<String, Workload>,
    // [impl->swdd~agent-supports-multiple-runtime-connectors~1]
    runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
    update_state_tx: WorkloadStateSender,
    workload_queue: WorkloadScheduler,
}

#[cfg_attr(test, automock)]
impl RuntimeManager {
    pub fn new(
        agent_name: AgentName,
        run_folder: PathBuf,
        control_interface_tx: ToServerSender,
        runtime_map: HashMap<String, Box<dyn RuntimeFacade>>,
        update_state_tx: WorkloadStateSender,
    ) -> Self {
        RuntimeManager {
            agent_name,
            run_folder,
            control_interface_tx,
            workloads: HashMap::new(),
            runtime_map,
            update_state_tx: update_state_tx.clone(),
            workload_queue: WorkloadScheduler::new(update_state_tx),
        }
    }

    // [impl->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    pub async fn update_workloads_on_fulfilled_dependencies(
        &mut self,
        workload_state_db: &WorkloadStatesMapSpec,
    ) {
        let workload_operations = self
            .workload_queue
            .next_workload_operations(workload_state_db)
            .await;

        if !workload_operations.is_empty() {
            self.execute_workload_operations(workload_operations).await;
        }
    }

    pub async fn execute_workloads(
        &mut self,
        added_workloads: Vec<ReusableWorkload>,
        deleted_workloads: Vec<DeletedWorkload>,
        workload_state_db: &WorkloadStatesMapSpec,
    ) {
        let workload_operations: Vec<WorkloadOperation> =
            self.transform_into_workload_operations(added_workloads, deleted_workloads);

        // [impl->swdd~agent-handles-new-workload-operations~1]
        // [impl->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
        let ready_workload_operations = self
            .workload_queue
            .enqueue_filtered_workload_operations(workload_operations, workload_state_db)
            .await;

        self.execute_workload_operations(ready_workload_operations)
            .await;
    }

    // [impl->swdd~agent-initial-list-existing-workloads~1]
    pub async fn handle_server_hello(
        &mut self,
        added_workloads: Vec<WorkloadNamed>,
        workload_state_db: &WorkloadStatesMapSpec,
    ) {
        log::info!(
            "Received the server hello with '{}' added workloads.",
            added_workloads.len()
        );

        let (new_added_workloads, deleted_workloads) = self
            .resume_and_remove_from_added_workloads(added_workloads)
            .await;

        self.execute_workloads(new_added_workloads, deleted_workloads, workload_state_db)
            .await;
    }

    // [impl->swdd~agent-handles-update-workload-requests~1]
    pub async fn handle_update_workload(
        &mut self,
        added_workloads: Vec<WorkloadNamed>,
        deleted_workloads: Vec<DeletedWorkload>,
        workload_state_db: &WorkloadStatesMapSpec,
    ) {
        log::info!(
            "Received a new desired state with '{}' added and '{}' deleted workloads.",
            added_workloads.len(),
            deleted_workloads.len()
        );

        let new_added_workloads: Vec<ReusableWorkload> = added_workloads.into_reusable_workloads();

        self.execute_workloads(new_added_workloads, deleted_workloads, workload_state_db)
            .await;
    }

    // [impl->swdd~agent-forward-responses-to-control-interface-pipe~1]
    pub async fn forward_response(&mut self, mut response: Response) {
        // [impl->swdd~agent-uses-id-prefix-forward-control-interface-response-correct-workload~1]
        // [impl->swdd~agent-remove-id-prefix-forwarding-control-interface-response~1]
        let (workload_name, request_id) = detach_prefix_from_request_id(&response.request_id);
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            response.request_id = request_id;
            if let Err(err) = workload.forward_response(response).await {
                log::warn!("Could not forward response to workload '{workload_name}': '{err}'");
            }
        } else {
            log::warn!("Could not forward response for unknown workload: '{workload_name}'");
        }
    }

    // [impl->swdd~agent-initial-list-existing-workloads~1]
    async fn resume_and_remove_from_added_workloads(
        &mut self,
        added_workloads: Vec<WorkloadNamed>,
    ) -> (Vec<ReusableWorkload>, Vec<DeletedWorkload>) {
        log::debug!("Handling initial workload list.");

        // create a list per runtime
        let mut added_workloads_per_runtime: HashMap<String, Vec<WorkloadNamed>> = HashMap::new();
        for workload_named in added_workloads {
            if let Some(workload_vec) =
                added_workloads_per_runtime.get_mut(&workload_named.workload.runtime)
            {
                workload_vec.push(workload_named);
            } else {
                added_workloads_per_runtime.insert(
                    workload_named.workload.runtime.clone(),
                    vec![workload_named],
                );
            }
        }

        let mut new_added_workloads = Vec::new();
        let mut deleted_workloads = Vec::new();
        // Go through each runtime and find existing workloads
        // [impl->swdd~agent-existing-workloads-finds-list~1]
        for (runtime_name, runtime) in &self.runtime_map {
            match runtime.get_reusable_workloads(&self.agent_name).await {
                Ok(workload_states) => {
                    log::info!(
                        "Found '{}' existing '{}' workload(s).",
                        workload_states.len(),
                        runtime_name,
                    );

                    for reusable_workload_state in workload_states {
                        let workload_state = reusable_workload_state.workload_state;
                        let workload_id = reusable_workload_state.workload_id;
                        if let Some(new_workload_named) = added_workloads_per_runtime
                            .get_mut(runtime_name)
                            .and_then(|workload_vec| {
                                // Remove and get the workload named from the workload_vec if name matches
                                workload_vec
                                    .iter()
                                    .position(|w| {
                                        w.instance_name
                                            .workload_name()
                                            .eq(workload_state.instance_name.workload_name())
                                    })
                                    .map(|index| workload_vec.remove(index))
                            })
                        {
                            let new_instance_name: WorkloadInstanceNameSpec =
                                new_workload_named.instance_name.clone();

                            // [impl->swdd~agent-existing-workloads-resume-existing~2]
                            if Self::is_resumable_workload(&workload_state, &new_instance_name) {
                                // [impl->swdd~agent-control-interface-created-for-eligible-workloads~1]
                                let control_interface_info = if new_workload_named
                                    .workload
                                    .needs_control_interface()
                                {
                                    Some(ControlInterfaceInfo::new(
                                        ControlInterfacePath::from((
                                            &self.run_folder,
                                            &new_instance_name,
                                        )),
                                        self.control_interface_tx.clone(),
                                        &new_instance_name,
                                        Authorizer::from(
                                            &new_workload_named.workload.control_interface_access,
                                        ),
                                    ))
                                } else {
                                    None
                                };

                                log::info!(
                                    "Resuming workload '{}'",
                                    new_instance_name.workload_name()
                                );

                                // [impl->swdd~agent-stores-running-workload~1]
                                self.workloads.insert(
                                    new_instance_name.workload_name().to_owned(),
                                    runtime.resume_workload(
                                        new_workload_named,
                                        control_interface_info,
                                        &self.update_state_tx,
                                    ),
                                );
                            } else if Self::is_reusable_workload(
                                &workload_state,
                                &workload_id,
                                &new_instance_name,
                            ) {
                                // [impl->swdd~agent-existing-workloads-reuse-unmodified~1]

                                log::info!(
                                    "Re-starting workload '{}'",
                                    new_instance_name.workload_name()
                                );

                                new_added_workloads
                                    .push(ReusableWorkload::new(new_workload_named, workload_id));
                            } else {
                                // [impl->swdd~agent-existing-workloads-replace-updated~4]

                                log::info!(
                                    "Replacing existing workload '{}'.",
                                    workload_state.instance_name.workload_name()
                                );

                                if !self
                                    .workloads
                                    .contains_key(workload_state.instance_name.workload_name())
                                {
                                    // Replace workload when agent was restarted.
                                    let old_workload_named = WorkloadNamed {
                                        instance_name: workload_state.instance_name.clone(),
                                        ..Default::default()
                                    };

                                    /* Resume the workload and update it to ensure the correct order
                                    and synchronization between the update steps. */
                                    let resumed_workload = runtime.resume_workload(
                                        old_workload_named,
                                        None,
                                        &self.update_state_tx,
                                    );

                                    self.workloads.insert(
                                        new_instance_name.workload_name().to_owned(),
                                        resumed_workload,
                                    );

                                    new_added_workloads
                                        .push(ReusableWorkload::new(new_workload_named, None));

                                    deleted_workloads.push(DeletedWorkload {
                                        instance_name: workload_state.instance_name,
                                        dependencies: HashMap::new(),
                                    });
                                } else {
                                    /* Replace workload when server was restarted.
                                    The runtime manager will request an update of the workload
                                    when putting the workload into both added and deleted ones.*/
                                    new_added_workloads
                                        .push(ReusableWorkload::new(new_workload_named, None));

                                    deleted_workloads.push(DeletedWorkload {
                                        instance_name: workload_state.instance_name,
                                        dependencies: HashMap::new(),
                                    });
                                }
                            }
                        } else {
                            let workload_name = workload_state.instance_name.workload_name();
                            log::info!("Found existing workload '{workload_name}' is not needed.");
                            // [impl->swdd~agent-existing-workloads-delete-unneeded~2]
                            if let Some(workload) = self.workloads.remove(workload_name) {
                                if let Err(err) = workload.delete().await {
                                    log::error!(
                                        "Failed to delete unneeded workload '{workload_name}': '{err}'"
                                    );
                                }
                            } else {
                                log::info!("Directly deleting workload {workload_name}");
                                runtime.delete_workload(
                                    workload_state.instance_name,
                                    &self.update_state_tx,
                                );
                            }
                        }
                    }
                }
                Err(err) => log::warn!("Could not get reusable running workloads: '{err}'"),
            }
        }

        // [impl->swdd~agent-existing-workloads-starts-new-if-not-found~1]
        new_added_workloads.extend(flatten(added_workloads_per_runtime));

        (new_added_workloads, deleted_workloads)
    }

    fn is_resumable_workload(
        workload_state_existing_workload: &WorkloadStateSpec,
        new_instance_name: &WorkloadInstanceNameSpec,
    ) -> bool {
        workload_state_existing_workload
            .execution_state
            .is_running()
            && workload_state_existing_workload
                .instance_name
                .eq(new_instance_name)
    }

    fn is_reusable_workload(
        workload_state_existing_workload: &WorkloadStateSpec,
        workload_id_existing_workload: &Option<String>,
        new_instance_name: &WorkloadInstanceNameSpec,
    ) -> bool {
        workload_state_existing_workload
            .execution_state
            .is_succeeded()
            && workload_id_existing_workload.is_some()
            && workload_state_existing_workload
                .instance_name
                .eq(new_instance_name)
    }

    // [impl->swdd~agent-transforms-update-workload-message-to-workload-operations~1]
    fn transform_into_workload_operations(
        &self,
        added_workloads: Vec<ReusableWorkload>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Vec<WorkloadOperation> {
        let mut workload_operations: Vec<WorkloadOperation> = Vec::new();
        // transform into a hashmap to be able to search for updates
        // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
        let mut added_workloads: HashMap<String, ReusableWorkload> = added_workloads
            .into_iter()
            .map(|reusable_workload| {
                (
                    reusable_workload
                        .workload_named
                        .instance_name
                        .workload_name()
                        .to_owned(),
                    reusable_workload,
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
                    updated_workload.workload_named,
                    deleted_workload,
                ));
            } else {
                // [impl->swdd~agent-deletes-workload~1]
                workload_operations.push(WorkloadOperation::Delete(deleted_workload));
            }
        }

        for (_, reusable_workload) in added_workloads {
            let workload_name = reusable_workload
                .workload_named
                .instance_name
                .workload_name();
            if self.workloads.contains_key(workload_name) {
                log::warn!(
                    "Added workload '{workload_name}' already exists. Updating without considering delete dependencies."
                );
                // We know this workload, seems the server is sending it again, try an update
                // [impl->swdd~agent-update-on-add-known-workload~1]
                let workload_named = reusable_workload.workload_named;
                let instance_name = workload_named.instance_name.clone();
                workload_operations.push(WorkloadOperation::Update(
                    workload_named,
                    DeletedWorkload {
                        instance_name,
                        dependencies: HashMap::default(),
                    },
                ));
            } else {
                // [impl->swdd~agent-added-creates-workload~1]
                workload_operations.push(WorkloadOperation::Create(reusable_workload));
            }
        }

        workload_operations
    }

    async fn execute_workload_operations(&mut self, workload_operations: Vec<WorkloadOperation>) {
        for wl_operation in workload_operations {
            match wl_operation {
                WorkloadOperation::Create(reusable_workload) => {
                    // [impl->swdd~agent-executes-create-workload-operation~1]
                    self.add_workload(reusable_workload).await
                }
                WorkloadOperation::Update(new_workload_named, _) => {
                    // [impl->swdd~agent-executes-update-workload-operation~1]
                    self.update_workload(new_workload_named).await
                }
                WorkloadOperation::UpdateDeleteOnly(deleted_workload) => {
                    // [impl->swdd~agent-executes-update-delete-only-workload-operation~1]
                    self.update_delete_only(deleted_workload).await
                }
                WorkloadOperation::Delete(deleted_workload) => {
                    // [impl->swdd~agent-executes-delete-workload-operation~1]
                    self.delete_workload(deleted_workload).await
                }
            }
        }
    }

    async fn add_workload(&mut self, reusable_workload: ReusableWorkload) {
        let workload_named = &reusable_workload.workload_named;
        let workload_name = workload_named.instance_name.workload_name().to_owned();
        // [impl->swdd~agent-control-interface-created-for-eligible-workloads~1]
        let control_interface_info = if workload_named.workload.needs_control_interface() {
            Some(ControlInterfaceInfo::new(
                ControlInterfacePath::from((&self.run_folder, &workload_named.instance_name)),
                self.control_interface_tx.clone(),
                &workload_named.instance_name,
                Authorizer::from(&workload_named.workload.control_interface_access),
            ))
        } else {
            log::info!("No control interface access specified for workload '{workload_name}'");
            None
        };

        let unsupported_runtime: Box<dyn RuntimeFacade>;

        // [impl->swdd~agent-uses-specified-runtime~1]
        // [impl->swdd~agent-skips-unknown-runtime~2]
        let runtime = if let Some(runtime) = self.runtime_map.get(&workload_named.workload.runtime)
        {
            runtime
        } else {
            log::warn!(
                "Could not find runtime '{}'. Workload '{}' not scheduled.",
                workload_named.workload.runtime,
                workload_name
            );
            unsupported_runtime = Box::new(GenericRuntimeFacade::new(
                Box::new(UnsupportedRuntime(workload_named.workload.runtime.clone())),
                PathBuf::new(),
            ));
            &unsupported_runtime
        };
        // [impl->swdd~agent-executes-create-workload-operation~1]
        let workload = runtime.create_workload(
            reusable_workload,
            control_interface_info,
            &self.update_state_tx,
        );
        // [impl->swdd~agent-stores-running-workload~1]
        self.workloads.insert(workload_name, workload);
    }

    async fn delete_workload(&mut self, deleted_workload: DeletedWorkload) {
        if let Some(workload) = self
            .workloads
            .remove(deleted_workload.instance_name.workload_name())
        {
            // [impl->swdd~agent-executes-delete-workload-operation~1]
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
                &deleted_workload.instance_name.workload_name()
            );

            // As the sender of this delete workload command expects a response,
            // report the execution state as 'Removed'
            self.update_state_tx
                .report_workload_execution_state(
                    &deleted_workload.instance_name,
                    ExecutionStateSpec::removed(),
                )
                .await;
        }
    }

    // [impl->swdd~agent-updates-deleted-and-added-workloads~1]
    async fn update_workload(&mut self, workload_named: WorkloadNamed) {
        let workload_name = workload_named.instance_name.workload_name().to_owned();

        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            // [impl->swdd~agent-control-interface-created-for-eligible-workloads~1]
            let control_interface_info = if workload_named.workload.needs_control_interface() {
                Some(ControlInterfaceInfo::new(
                    ControlInterfacePath::from((&self.run_folder, &workload_named.instance_name)),
                    self.control_interface_tx.clone(),
                    &workload_named.instance_name,
                    Authorizer::from(&workload_named.workload.control_interface_access),
                ))
            } else {
                log::info!(
                    "No control interface access specified for updated workload '{workload_name}'"
                );
                None
            };
            // [impl->swdd~agent-executes-update-workload-operation~1]
            if let Err(err) = workload
                .update(Some(workload_named), control_interface_info)
                .await
            {
                log::error!("Failed to update workload '{workload_name}': '{err}'");
            }
        } else {
            log::warn!("Workload for update '{workload_name}' not found. Recreating.");
            // [impl->swdd~agent-add-on-update-missing-workload~1]
            self.add_workload(ReusableWorkload::new(workload_named, None))
                .await;
        }
    }

    // [impl->swdd~agent-executes-update-delete-only-workload-operation~1]
    async fn update_delete_only(&mut self, deleted_workload: DeletedWorkload) {
        let workload_name = deleted_workload.instance_name.workload_name().to_owned();
        if let Some(workload) = self.workloads.get_mut(&workload_name) {
            if let Err(err) = workload.update(None, None).await {
                log::error!("Failed to update workload '{workload_name}': '{err}'");
            }
        }
    }

    // [impl->swdd~agent-runtime-manager-creates-log-fetchers~1]
    pub async fn get_log_fetchers(
        &self,
        log_request: LogsRequestSpec,
    ) -> Vec<(WorkloadInstanceNameSpec, Box<dyn LogFetcher>)> {
        let mut res = Vec::new();
        let log_request_options: LogRequestOptions = log_request.clone().into();
        for workload in log_request.workload_names {
            let Some(workload_instance) = self.workloads.get(workload.workload_name()) else {
                log::info!("Could not find workload '{}'", workload.workload_name());
                continue;
            };

            match workload_instance
                .start_collecting_logs(log_request_options.clone())
                .await
            {
                Ok(log_fetcher) => res.push((workload, log_fetcher)),
                Err(err) => log::info!(
                    "Did not get log fetcher for '{}': '{}'.",
                    workload.workload_name(),
                    err
                ),
            };
        }

        res
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
    use super::{
        ControlInterfaceInfo, DeletedWorkload, RuntimeFacade, RuntimeManager, WorkloadOperation,
    };
    use crate::control_interface::{
        MockControlInterface, authorizer::MockAuthorizer,
        control_interface_info::MockControlInterfaceInfo,
    };
    use crate::runtime_connectors::log_fetcher::MockLogFetcher;
    use crate::runtime_connectors::{
        LogRequestOptions, MockGenericRuntimeFacade, MockRuntimeFacade, ReusableWorkloadState,
        RuntimeError,
    };
    use crate::runtime_manager::ToReusableWorkloadSpecs;
    use crate::workload::{MockWorkload, WorkloadError};
    use crate::workload_operation::ReusableWorkload;
    use crate::workload_scheduler::scheduler::MockWorkloadScheduler;
    use crate::workload_state::WorkloadStateReceiver;

    use ankaios_api::ank_base::{
        self, CompleteStateResponse, ExecutionStateSpec, LogsRequestSpec, Response,
        ResponseContent, WorkloadInstanceNameBuilder, WorkloadInstanceNameSpec, WorkloadNamed,
        WorkloadStateSpec, WorkloadStatesMapSpec,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_agent_tags, generate_test_complete_state,
        generate_test_control_interface_access, generate_test_deleted_workload_with_dependencies,
        generate_test_deleted_workload_with_params, generate_test_proto_complete_state,
        generate_test_workload_named, generate_test_workload_named_with_params,
        generate_test_workload_with_params,
    };
    use common::to_server_interface::ToServerReceiver;

    use mockall::{Sequence, predicate};
    use std::collections::HashMap;
    use std::error::Error;
    use std::fmt::Display;
    use std::{any::Any, path::Path};
    use tokio::sync::mpsc::channel;

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

        pub fn build(self) -> (ToServerReceiver, RuntimeManager, WorkloadStateReceiver) {
            let (to_server, server_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
            let (wl_state_sender, wl_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
            let runtime_manager = RuntimeManager::new(
                fixtures::AGENT_NAMES[0].into(),
                Path::new(fixtures::RUN_FOLDER).into(),
                to_server.clone(),
                self.runtime_facade_map,
                wl_state_sender.clone(),
            );
            (server_receiver, runtime_manager, wl_state_receiver)
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
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .times(1)
            .returning(|_, _, _, _| MockControlInterfaceInfo::default());

        let new_workload_access = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut new_workload_no_access = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[1],
        );
        new_workload_no_access.workload.control_interface_access = Default::default();

        let added_workloads = vec![new_workload_access.clone(), new_workload_no_access.clone()];
        let workload_operations = vec![
            WorkloadOperation::Create(ReusableWorkload::new(new_workload_access, None)),
            WorkloadOperation::Create(ReusableWorkload::new(new_workload_no_access, None)),
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
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock
            .expect_create_workload()
            .once()
            .returning(move |_, _, _| MockWorkload::default());

        let mut runtime_facade_mock_2 = MockRuntimeFacade::new();
        runtime_facade_mock_2
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock_2
            .expect_create_workload()
            .once()
            .returning(move |_, _, _| MockWorkload::default());

        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .with_runtime(
                fixtures::RUNTIME_NAMES[1],
                Box::new(runtime_facade_mock_2) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager
            .handle_server_hello(added_workloads, &WorkloadStatesMapSpec::default())
            .await;

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[1])
        );
    }

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_handle_update_workload_no_workload_with_unknown_runtime() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let workload_with_unknown_runtime = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            "unknown_runtime",
        );
        let added_workloads = vec![workload_with_unknown_runtime.clone()];

        let workload_operations = vec![WorkloadOperation::Create(ReusableWorkload::new(
            workload_with_unknown_runtime,
            None,
        ))];

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
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));

        runtime_facade_mock.expect_create_workload().never(); // workload shall not be created due to unknown runtime

        let mock_workload = MockWorkload::default();

        let mut mock_generic_runtime_facade = MockGenericRuntimeFacade::default();
        mock_generic_runtime_facade
            .expect_create_workload()
            .return_once(|_, _, _| mock_workload);

        let runtime_facade_mock_new_context = MockGenericRuntimeFacade::new_context();
        runtime_facade_mock_new_context
            .expect()
            .return_once(|_, _| mock_generic_runtime_facade);

        let (_server_recv, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        runtime_manager
            .handle_server_hello(added_workloads, &WorkloadStatesMapSpec::default())
            .await;
    }

    // [utest->swdd~agent-existing-workloads-finds-list~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_failed_to_get_reusable_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let added_workloads = vec![workload.clone()];

        let workload_operations = vec![WorkloadOperation::Create(ReusableWorkload::new(
            workload, None,
        ))];
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
            .expect_get_reusable_workloads()
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
            .withf(|reusable_workload, control_interface, to_server| {
                reusable_workload
                    .workload_named
                    .instance_name
                    .workload_name()
                    == fixtures::WORKLOAD_NAMES[0]
                    && control_interface.is_some()
                    && !to_server.is_closed()
            })
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        runtime_manager
            .handle_server_hello(added_workloads, &WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-control-interface-created-for-eligible-workloads~1]
    #[tokio::test]
    async fn utest_update_workload_test_control_interface_creation() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .never();

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let authorizer_mock = MockAuthorizer::from_context();
        authorizer_mock
            .expect()
            .once()
            .returning(|_| MockAuthorizer::new());

        let control_interface_info_new_context = MockControlInterfaceInfo::new_context();

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_update()
            .once()
            .returning(|_, _| Ok(()));

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .return_once(move |_, _, _| mock_workload);

        let (_server_recv, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        control_interface_info_new_context
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());
        let mut workload_no_access = generate_test_workload_named();
        workload_no_access.workload.control_interface_access = Default::default();
        runtime_manager.update_workload(workload_no_access).await;

        control_interface_info_new_context.expect().never();
        let workload_has_access = generate_test_workload_named();
        runtime_manager.update_workload(workload_has_access).await;
    }

    // [utest->swdd~agent-existing-workloads-resume-existing~2]
    // [utest->swdd~agent-existing-workloads-starts-new-if-not-found~1]
    // [utest->swdd~agent-stores-running-workload~1]
    #[tokio::test]
    async fn utest_resume_existing_running_workload_with_equal_config() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_new_context = MockControlInterfaceInfo::new_context();
        control_interface_info_new_context
            .expect()
            .once()
            .returning(move |_, _, _, _| MockControlInterfaceInfo::default());

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

        let existing_workload = generate_test_workload_named();

        let existing_workload_instance_name = existing_workload.instance_name.clone();
        let reusable_workload_state_running = ReusableWorkloadState::new(
            existing_workload_instance_name,
            ExecutionStateSpec::running(),
            None,
        );

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![reusable_workload_state_running]) }));

        runtime_facade_mock
            .expect_resume_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        runtime_facade_mock.expect_create_workload().never();

        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let added_workloads = vec![existing_workload];
        runtime_manager
            .handle_server_hello(added_workloads, &WorkloadStatesMapSpec::default())
            .await;

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-existing-workloads-replace-updated~4]
    #[tokio::test]
    async fn utest_replace_existing_workload_with_different_config() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_workload = generate_test_workload_named();

        let added_workloads = vec![existing_workload.clone()];

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        // create workload with different config string to simulate a replace of a existing workload
        let existing_workload_with_other_config = WorkloadInstanceNameBuilder::default()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .config(&String::from("different config"))
            .agent_name(fixtures::AGENT_NAMES[0])
            .build();

        let reusable_workload_state_running = ReusableWorkloadState::new(
            existing_workload_with_other_config.clone(),
            ExecutionStateSpec::running(),
            None,
        );

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![reusable_workload_state_running]) }));

        runtime_facade_mock
            .expect_resume_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let expected_new_added_workloads: Vec<ReusableWorkload> =
            added_workloads.clone().into_reusable_workloads();
        let (new_added_workloads, deleted_workloads) = runtime_manager
            .resume_and_remove_from_added_workloads(added_workloads)
            .await;

        assert_eq!(expected_new_added_workloads, new_added_workloads);
        assert_eq!(
            vec![DeletedWorkload {
                instance_name: existing_workload_with_other_config,
                ..Default::default()
            }],
            deleted_workloads
        );
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        ); // existing workload is resumed
    }

    // [utest->swdd~agent-existing-workloads-replace-updated~4]
    #[tokio::test]
    async fn utest_replace_existing_not_running_workload_after_agent_restart() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_workload = generate_test_workload_named();
        let added_workloads = vec![existing_workload.clone()];

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let reusable_workload_state_succeeded = ReusableWorkloadState::new(
            existing_workload.instance_name.clone(),
            ExecutionStateSpec::failed("some error"),
            None,
        );

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![reusable_workload_state_succeeded]) }));
        runtime_facade_mock
            .expect_resume_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let expected_added_workloads: Vec<ReusableWorkload> =
            added_workloads.clone().into_reusable_workloads();
        let (new_added_workloads, deleted_workloads) = runtime_manager
            .resume_and_remove_from_added_workloads(added_workloads)
            .await;

        assert_eq!(expected_added_workloads, new_added_workloads);
        assert_eq!(
            vec![DeletedWorkload {
                instance_name: existing_workload.instance_name,
                ..Default::default()
            }],
            deleted_workloads
        );
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        ); // existing workload is resumed
    }

    // [utest->swdd~agent-existing-workloads-replace-updated~4]
    #[tokio::test]
    async fn utest_replace_existing_not_running_workload_after_server_restart() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_workload = generate_test_workload_named();

        let added_workloads = vec![existing_workload.clone()];

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let reusable_workload_state_succeeded = ReusableWorkloadState::new(
            existing_workload.instance_name.clone(),
            ExecutionStateSpec::failed("some error"),
            None,
        );

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![reusable_workload_state_succeeded]) }));
        runtime_facade_mock.expect_resume_workload().never();

        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager.workloads.insert(
            fixtures::WORKLOAD_NAMES[0].to_owned(),
            MockWorkload::default(),
        ); // workload is known

        let expected_added_workloads: Vec<ReusableWorkload> =
            added_workloads.clone().into_reusable_workloads();
        let (new_added_workloads, deleted_workloads) = runtime_manager
            .resume_and_remove_from_added_workloads(added_workloads)
            .await;

        // workload is updated through putting it into added and deleted workloads
        assert_eq!(expected_added_workloads, new_added_workloads);
        assert_eq!(
            vec![DeletedWorkload {
                instance_name: existing_workload.instance_name,
                ..Default::default()
            }],
            deleted_workloads
        );
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-existing-workloads-reuse-unmodified~1]
    #[tokio::test]
    async fn utest_reuse_existing_succeeded_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_workload = generate_test_workload_named();

        let added_workloads = vec![existing_workload.clone()];

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let reusable_workload_state_succeeded = ReusableWorkloadState::new(
            existing_workload.instance_name.clone(),
            ExecutionStateSpec::succeeded(),
            Some(fixtures::WORKLOAD_IDS[0].to_string()),
        );

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![reusable_workload_state_succeeded]) }));

        runtime_facade_mock.expect_delete_workload().never();

        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let expected_new_added_workloads: Vec<ReusableWorkload> = added_workloads
            .clone()
            .into_iter()
            .map(|w| ReusableWorkload::new(w, Some(fixtures::WORKLOAD_IDS[0].to_string())))
            .collect();
        let (new_added_workloads, deleted_workloads) = runtime_manager
            .resume_and_remove_from_added_workloads(added_workloads)
            .await;

        assert_eq!(expected_new_added_workloads, new_added_workloads);
        assert!(deleted_workloads.is_empty());
        assert!(
            !runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-existing-workloads-delete-unneeded~2]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_delete_unneeded_after_agent_restart() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_unneeded_workload = WorkloadInstanceNameBuilder::default()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .config(&String::from("different config"))
            .agent_name(fixtures::AGENT_NAMES[0])
            .build();

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
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| {
                Box::pin(async move {
                    Ok(vec![ReusableWorkloadState::new(
                        existing_unneeded_workload,
                        ExecutionStateSpec::default(),
                        None,
                    )])
                })
            });

        runtime_facade_mock
            .expect_delete_workload()
            .once()
            .return_const(());

        let (_, mut runtime_manager, _wl_state_receiver) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        runtime_manager
            .handle_server_hello(vec![], &WorkloadStatesMapSpec::default())
            .await;

        assert!(runtime_manager.workloads.is_empty());
    }

    // [utest->swdd~agent-existing-workloads-delete-unneeded~2]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_delete_unneeded_after_server_restart() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let existing_unneeded_workload = WorkloadInstanceNameBuilder::default()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .config(&String::from("different config"))
            .agent_name(fixtures::AGENT_NAMES[0])
            .build();

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
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| {
                Box::pin(async move {
                    Ok(vec![ReusableWorkloadState::new(
                        existing_unneeded_workload,
                        ExecutionStateSpec::default(),
                        None,
                    )])
                })
            });

        runtime_facade_mock.expect_delete_workload().never();

        let (_, mut runtime_manager, _wl_state_receiver) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_delete().once().return_once(|| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        runtime_manager
            .handle_server_hello(vec![], &WorkloadStatesMapSpec::default())
            .await;

        assert!(runtime_manager.workloads.is_empty());
    }

    // [utest->swdd~agent-handles-new-workload-operations~1]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_add_workload_with_unfulfilled_dependencies()
    {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let added_workloads: Vec<WorkloadNamed> = vec![generate_test_workload_named()];

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
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| Box::pin(async { Ok(vec![]) }));
        runtime_facade_mock.expect_create_workload().never();

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        runtime_manager
            .handle_server_hello(added_workloads, &WorkloadStatesMapSpec::default())
            .await;

        assert!(runtime_manager.workloads.is_empty());
    }

    // [utest->swdd~agent-control-interface-created-for-eligible-workloads~1]
    #[tokio::test]
    async fn utest_add_workload_test_control_interface_creation() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_workload_scheduler = MockWorkloadScheduler::default();
        mock_workload_scheduler
            .expect_enqueue_filtered_workload_operations()
            .never();

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let authorizer_mock = MockAuthorizer::from_context();
        authorizer_mock
            .expect()
            .once()
            .returning(|_| MockAuthorizer::new());

        let control_interface_info_new_context = MockControlInterfaceInfo::new_context();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .returning(move |_, _, _| MockWorkload::default());

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        control_interface_info_new_context
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());
        let mut workload_no_access = generate_test_workload_named();
        workload_no_access.workload.control_interface_access = Default::default();
        runtime_manager
            .add_workload(ReusableWorkload::new(workload_no_access, None))
            .await;

        control_interface_info_new_context.expect().never();
        let workload_has_access = generate_test_workload_named();
        runtime_manager
            .add_workload(ReusableWorkload::new(workload_has_access, None))
            .await;
    }

    // [utest->swdd~agent-existing-workloads-replace-updated~4]
    #[tokio::test]
    async fn utest_handle_update_workload_initial_call_replace_workload_with_unfulfilled_dependencies()
     {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let control_interface_mock = MockControlInterface::new_context();
        control_interface_mock.expect().never();

        // create workload with different config string to simulate a replace of a existing workload
        let existing_workload_with_other_config = WorkloadInstanceNameSpec::builder()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .config(&String::from("different config"))
            .agent_name(fixtures::AGENT_NAMES[0])
            .build();

        let workload_operations = vec![WorkloadOperation::UpdateDeleteOnly(DeletedWorkload {
            instance_name: existing_workload_with_other_config.clone(),
            dependencies: HashMap::new(),
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

        let existing_workload = generate_test_workload_named();

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_get_reusable_workloads()
            .once()
            .return_once(|_| {
                Box::pin(async {
                    Ok(vec![ReusableWorkloadState::new(
                        existing_workload_with_other_config,
                        ExecutionStateSpec::default(),
                        None,
                    )])
                })
            });

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_update()
            .once()
            .returning(|_, _| Ok(()));
        runtime_facade_mock
            .expect_resume_workload()
            .once()
            .return_once(|_, _, _| mock_workload);

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let added_workloads = vec![existing_workload];
        runtime_manager
            .handle_server_hello(added_workloads, &WorkloadStatesMapSpec::default())
            .await;

        // the old workload is resumed followed by an update delete only
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-updates-deleted-and-added-workloads~1]
    // [utest->swdd~agent-handles-update-workload-requests~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_update_on_add_and_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let old_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0].to_string(),
            fixtures::WORKLOAD_NAMES[0].to_string(),
        );

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
        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .with(
                predicate::function(|workload: &Option<WorkloadNamed>| {
                    workload.is_some()
                        && workload.as_ref().unwrap().instance_name.workload_name()
                            == fixtures::WORKLOAD_NAMES[0]
                }),
                predicate::function(|control_interface: &Option<ControlInterfaceInfo>| {
                    control_interface.is_some()
                }),
            )
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![old_workload];
        // workload is in added and deleted workload vec
        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &WorkloadStatesMapSpec::default(),
            )
            .await;

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-deletes-workload~1]
    // [utest->swdd~agent-handle-deleted-before-added-workloads~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_delete_before_adding() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

        let workload_operations = vec![
            WorkloadOperation::Delete(deleted_workload.clone()),
            WorkloadOperation::Create(ReusableWorkload::new(new_workload.clone(), None)),
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
            .withf(|reusable_workload, control_interface, to_server| {
                reusable_workload
                    .workload_named
                    .instance_name
                    .workload_name()
                    == fixtures::WORKLOAD_NAMES[1]
                    && control_interface.is_some()
                    && !to_server.is_closed()
            })
            .in_sequence(&mut delete_before_add_seq)
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![deleted_workload];

        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &WorkloadStatesMapSpec::default(),
            )
            .await;
        server_receiver.close();

        assert!(
            !runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[1])
        );
    }

    // [utest->swdd~agent-add-on-update-missing-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_add_on_update_missing() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

        let workload_operations = vec![WorkloadOperation::Create(ReusableWorkload::new(
            new_workload.clone(),
            None,
        ))];
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

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![deleted_workload];
        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &WorkloadStatesMapSpec::default(),
            )
            .await;

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-update-on-add-known-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_update_known_added() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let new_workload = generate_test_workload_named();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let old_workload = generate_test_deleted_workload_with_dependencies(
            fixtures::AGENT_NAMES[0].to_owned(),
            fixtures::WORKLOAD_NAMES[0].to_owned(),
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
        let (_, mut runtime_manager, _) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .withf(|workload, control_interface| {
                workload.is_some()
                    && workload.as_ref().unwrap().instance_name.workload_name()
                        == fixtures::WORKLOAD_NAMES[0]
                    && control_interface.is_some()
            })
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![], &WorkloadStatesMapSpec::default())
            .await;

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-added-creates-workload~1]
    // [utest->swdd~agent-uses-specified-runtime~1]
    // [utest->swdd~agent-stores-running-workload~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_add_new_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let new_workload = generate_test_workload_named();

        let workload_operations = vec![WorkloadOperation::Create(ReusableWorkload::new(
            new_workload.clone(),
            None,
        ))];
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
            .withf(|reusable_workload, control_interface, to_server| {
                reusable_workload
                    .workload_named
                    .instance_name
                    .workload_name()
                    == fixtures::WORKLOAD_NAMES[0]
                    && control_interface.is_some()
                    && !to_server.is_closed()
            })
            .return_once(|_, _, _| MockWorkload::default());

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let added_workloads = vec![new_workload];
        runtime_manager
            .handle_update_workload(added_workloads, vec![], &WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-handles-new-workload-operations~1]
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

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let added_workloads: Vec<WorkloadNamed> = vec![generate_test_workload_named()];

        runtime_manager
            .handle_update_workload(added_workloads, vec![], &WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            !runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-executes-update-delete-only-workload-operation~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_update_delete_only_with_fulfilled_delete_dependencies()
     {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let old_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

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
        let (_, mut runtime_manager, _wl_state_receiver) = RuntimeManagerBuilder::default()
            .with_runtime(
                fixtures::RUNTIME_NAMES[0],
                Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
            )
            .build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .with(
                predicate::eq(None), // in case of update delete only there is no new workload
                predicate::function(|control_interface: &Option<ControlInterfaceInfo>| {
                    control_interface.is_none()
                }),
            )
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let added_workloads = vec![new_workload];
        let deleted_workloads = vec![old_workload];
        // workload is in added and deleted workload vec
        runtime_manager
            .handle_update_workload(
                added_workloads,
                deleted_workloads,
                &WorkloadStatesMapSpec::default(),
            )
            .await;
        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-handles-new-workload-operations~1]
    #[tokio::test]
    async fn utest_handle_update_workload_subsequent_deleted_workload_with_not_fulfilled_dependencies()
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

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_delete().never();

        let new_deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

        runtime_manager.workloads.insert(
            new_deleted_workload
                .instance_name
                .workload_name()
                .to_owned(),
            workload_mock,
        );

        let deleted_workloads = vec![new_deleted_workload.clone()];
        runtime_manager
            .handle_update_workload(vec![], deleted_workloads, &WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
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

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let request_id: String = fixtures::REQUEST_ID.to_string();
        let complete_state_response = CompleteStateResponse::default();

        let expected_response = Response {
            request_id,
            response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                complete_state_response.clone(),
            ))),
        };
        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_forward_response()
            .once()
            .with(predicate::eq(expected_response))
            .return_once(move |_| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), mock_workload);

        runtime_manager
            .forward_response(Response {
                request_id: format!("{}@{}", fixtures::WORKLOAD_NAMES[0], fixtures::REQUEST_ID),
                response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                    complete_state_response,
                ))),
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

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();
        let request_id: String = fixtures::REQUEST_ID.to_string();
        let workloads = [(
            fixtures::WORKLOAD_NAMES[0],
            generate_test_workload_with_params(
                fixtures::AGENT_NAMES[0],
                fixtures::RUNTIME_NAMES[0],
            )
            .into(),
        )];
        let mut complete_state = generate_test_proto_complete_state(&workloads);
        complete_state.workload_states = Some(ank_base::WorkloadStatesMap {
            agent_state_map: HashMap::from([(
                fixtures::AGENT_NAMES[0].to_string(),
                ank_base::ExecutionsStatesOfWorkload {
                    wl_name_state_map: HashMap::from([(
                        fixtures::WORKLOAD_NAMES[0].to_string(),
                        ank_base::ExecutionsStatesForId {
                            id_state_map: HashMap::from([(
                                fixtures::WORKLOAD_IDS[0].to_string(),
                                ank_base::ExecutionState {
                                    additional_info: Some("".to_string()),
                                    execution_state_enum: Some(
                                        ank_base::execution_state::ExecutionStateEnum::Running(0),
                                    ),
                                },
                            )]),
                        },
                    )]),
                },
            )]),
        });

        complete_state.agents = Some(ank_base::AgentMap {
            agents: HashMap::from([(
                fixtures::AGENT_NAMES[0].to_owned(),
                ank_base::AgentAttributes {
                    status: Some(ank_base::AgentStatus {
                        cpu_usage: Some(ank_base::CpuUsage::from(fixtures::CPU_USAGE_SPEC)),
                        free_memory: Some(ank_base::FreeMemory::from(fixtures::FREE_MEMORY_SPEC)),
                    }),
                    tags: Some(generate_test_agent_tags().into()),
                },
            )]),
        });

        let expected_response = Response {
            request_id,
            response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                ank_base::CompleteStateResponse {
                    complete_state: Some(complete_state),
                    ..Default::default()
                },
            ))),
        };
        let forwarded_response = Response {
            request_id: format!("{}@{}", fixtures::WORKLOAD_NAMES[0], fixtures::REQUEST_ID),
            response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                ank_base::CompleteStateResponse {
                    complete_state: Some(
                        generate_test_complete_state(vec![
                            generate_test_workload_named_with_params(
                                fixtures::WORKLOAD_NAMES[0],
                                fixtures::AGENT_NAMES[0],
                                fixtures::RUNTIME_NAMES[0],
                            ),
                        ])
                        .into(),
                    ),
                    ..Default::default()
                },
            ))),
        };

        let mut mock_workload = MockWorkload::default();
        mock_workload
            .expect_forward_response()
            .once()
            .with(predicate::eq(expected_response))
            .return_once(move |_| {
                Err(WorkloadError::CompleteState(
                    "failed to send complete state".to_string(),
                ))
            });

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), mock_workload);

        // runtime_manager
        //     .forward_response(ank_base::Response {
        //         request_id: format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
        //         response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
        //             generate_test_complete_state(vec![generate_test_workload_spec_with_param(
        //                 AGENT_NAME.to_string(),
        //                 WORKLOAD_1_NAME.to_string(),
        //                 RUNTIME_NAME.to_string(),
        //             )])
        //             .into(),
        //         ))),
        //     })
        //     .await;
        runtime_manager.forward_response(forwarded_response).await;
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

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let mut mock_workload = MockWorkload::default();
        mock_workload.expect_forward_response().never();

        runtime_manager
            .forward_response(Response {
                request_id: format!("{}@{}", fixtures::WORKLOAD_NAMES[0], fixtures::REQUEST_ID),
                response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                    generate_test_complete_state(vec![generate_test_workload_named()]).into(),
                ))),
            })
            .await;
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_update_workload_state_create_workload_with_fulfilled_dependencies() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let mut workload = generate_test_workload_named();
        workload.workload.control_interface_access = generate_test_control_interface_access();

        let next_workload_operations = vec![WorkloadOperation::Create(ReusableWorkload::new(
            workload, None,
        ))];
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

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
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

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            !runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_update_workload_state_delete_workload_dependencies_with_fulfilled_dependencies()
    {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

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

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_delete()
            .once()
            .return_once(move || Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_owned(), workload_mock);

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            !runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
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

        let (mut server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock.expect_delete().never();

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_owned(), workload_mock);

        runtime_manager
            .update_workloads_on_fulfilled_dependencies(&WorkloadStatesMapSpec::default())
            .await;
        server_receiver.close();

        assert!(
            runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
    }

    #[tokio::test]
    async fn utest_delete_workload_on_already_removed_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let instance_name = WorkloadInstanceNameBuilder::default()
            .workload_name(fixtures::WORKLOAD_NAMES[0])
            .config(&String::from("some config"))
            .agent_name(fixtures::AGENT_NAMES[0])
            .build();

        let mock_workload_scheduler = MockWorkloadScheduler::default();
        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| mock_workload_scheduler);

        let (mut server_receiver, mut runtime_manager, mut wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        runtime_manager
            .delete_workload(DeletedWorkload {
                instance_name,
                dependencies: HashMap::new(),
            })
            .await;
        server_receiver.close();
        let wl_state_msg = wl_state_receiver.recv().await;

        assert!(
            !runtime_manager
                .workloads
                .contains_key(fixtures::WORKLOAD_NAMES[0])
        );
        assert_ne!(wl_state_msg, None);

        let WorkloadStateSpec {
            instance_name: actual_instance_name,
            execution_state: actual_execution_state,
        } = wl_state_msg.unwrap();

        assert_eq!(
            actual_instance_name.workload_name(),
            fixtures::WORKLOAD_NAMES[0]
        );
        assert_eq!(actual_execution_state, ExecutionStateSpec::removed());
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

        let (_server_receiver, runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let new_workload = generate_test_workload_named();
        let added_workloads = vec![ReusableWorkload::new(new_workload.clone(), None)];
        let deleted_workloads = vec![];
        let workload_operations =
            runtime_manager.transform_into_workload_operations(added_workloads, deleted_workloads);

        assert_eq!(
            vec![WorkloadOperation::Create(ReusableWorkload::new(
                new_workload,
                None
            ))],
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

        let (_server_receiver, runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();
        let added_workloads = vec![];
        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );
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

        let (_server_receiver, runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let added_workloads = vec![ReusableWorkload::new(new_workload.clone(), None)];
        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );
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
        let _from_authorizer_context = setup_from_authorizer();

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let control_interface_info_mock = MockControlInterfaceInfo::new_context();
        control_interface_info_mock
            .expect()
            .once()
            .return_once(|_, _, _, _| MockControlInterfaceInfo::default());

        let mut runtime_facade_mock = MockRuntimeFacade::new();
        runtime_facade_mock
            .expect_create_workload()
            .once()
            .return_once(|_, _, _| MockWorkload::default());

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default()
                .with_runtime(
                    fixtures::RUNTIME_NAMES[0],
                    Box::new(runtime_facade_mock) as Box<dyn RuntimeFacade>,
                )
                .build();

        let new_workload = generate_test_workload_named();
        let workload_operations = vec![WorkloadOperation::Create(ReusableWorkload::new(
            new_workload,
            None,
        ))];
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

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_delete()
            .once()
            .return_once(move || Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );
        let workload_operations = vec![WorkloadOperation::Delete(deleted_workload)];
        runtime_manager
            .execute_workload_operations(workload_operations)
            .await;
    }

    // [utest->swdd~agent-executes-update-delete-only-workload-operation~1]
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

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

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
        let _from_authorizer_context = setup_from_authorizer();

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        new_workload.workload.control_interface_access = Default::default();

        let mut workload_mock = MockWorkload::default();
        workload_mock
            .expect_update()
            .once()
            .return_once(move |_, _| Ok(()));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_mock);

        let deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

        let workload_operations = vec![WorkloadOperation::Update(new_workload, deleted_workload)];
        runtime_manager
            .execute_workload_operations(workload_operations)
            .await;
    }

    // [utest->swdd~agent-runtime-manager-creates-log-fetchers~1]
    #[tokio::test]
    async fn utest_execute_workload_operations_get_log_fetchers() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _from_authorizer_context = setup_from_authorizer();

        let mock_workload_scheduler_context = MockWorkloadScheduler::new_context();
        mock_workload_scheduler_context
            .expect()
            .once()
            .return_once(|_| MockWorkloadScheduler::default());

        let (_server_receiver, mut runtime_manager, _wl_state_receiver) =
            RuntimeManagerBuilder::default().build();

        let mut workload_1_mock = MockWorkload::default();
        workload_1_mock
            .expect_start_collecting_logs()
            .with(mockall::predicate::eq(LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            }))
            .once()
            .returning(|_| Ok(Box::new(MockLogFetcher::new())));
        let mut workload_2_mock = MockWorkload::default();
        workload_2_mock
            .expect_start_collecting_logs()
            .with(mockall::predicate::eq(LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            }))
            .once()
            .returning(|_| Err(Box::new(MockError())));

        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_string(), workload_1_mock);
        runtime_manager
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[1].to_string(), workload_2_mock);

        let res = runtime_manager
            .get_log_fetchers(LogsRequestSpec {
                workload_names: vec![
                    WorkloadInstanceNameSpec::new(
                        fixtures::AGENT_NAMES[0],
                        fixtures::WORKLOAD_NAMES[0],
                        fixtures::WORKLOAD_IDS[0],
                    ),
                    WorkloadInstanceNameSpec::new(
                        fixtures::AGENT_NAMES[0],
                        fixtures::WORKLOAD_NAMES[1],
                        fixtures::WORKLOAD_IDS[0],
                    ),
                    WorkloadInstanceNameSpec::new(
                        fixtures::AGENT_NAMES[0],
                        fixtures::WORKLOAD_NAMES[2],
                        fixtures::WORKLOAD_IDS[0],
                    ),
                ],
                follow: true,
                tail: -1,
                since: None,
                until: None,
            })
            .await;

        assert_eq!(res.len(), 1);
        assert_eq!(
            &res[0].0,
            &WorkloadInstanceNameSpec::new(
                fixtures::AGENT_NAMES[0],
                fixtures::WORKLOAD_NAMES[0],
                fixtures::WORKLOAD_IDS[0]
            )
        );
    }

    fn setup_from_authorizer() -> Box<dyn Any> {
        let authorizer_from_context_mock = MockAuthorizer::from_context();
        authorizer_from_context_mock
            .expect()
            .returning(|_| MockAuthorizer::new());
        Box::new(authorizer_from_context_mock)
    }

    struct MockError();

    impl Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MockError")
        }
    }

    impl std::fmt::Debug for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Display::fmt(self, f)
        }
    }

    impl Error for MockError {}
}
