// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use crate::runtime_connectors::OwnableRuntime;
#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
use crate::runtime_registry::RuntimeRegistry;

use crate::agent_config::AgentConfig;
use crate::agent_manager::AgentManager;
use crate::io_utils;
use ankaios_api::ank_base::WorkloadStateSpec;

use common::{
    communications_client::CommunicationsClient, from_server_interface::FromServer,
    objects::AgentName, std_extensions::GracefulExitResult, to_server_interface::ToServer,
};

use crate::runtime_connectors::{GenericRuntimeFacade, RuntimeFacade};

use grpc::client::GRPCCommunicationsClient;
use grpc::security::TLSConfig;
use std::collections::HashMap;

pub(crate) const BUFFER_SIZE: usize = 20;

/// Builder for assembling and running an Ankaios agent with a chosen set of runtimes.
///
/// Config is resolved by the caller (e.g. via `AgentConfig::load()`) before building runtimes or
/// a communications client, so both can be constructed from the resolved `AgentConfig`.
#[derive(Default)]
pub struct AgentBuilder {
    available_runtimes: RuntimeRegistry,
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runtime(mut self, runtime: Box<dyn OwnableRuntime>) -> Self {
        self.available_runtimes.register(runtime);
        self
    }

    pub async fn run(self, agent_config: AgentConfig) {
        log::debug!(
            "Starting the Ankaios agent with \n\tname: '{}', \n\tserver url: '{}', \n\trun directory: '{}'",
            agent_config.name,
            agent_config.server_url,
            agent_config.run_folder,
        );

        // [impl->swdd~agent-uses-async-channels~1]
        let (to_manager, manager_receiver) = tokio::sync::mpsc::channel::<FromServer>(BUFFER_SIZE);
        let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);
        let (workload_state_sender, workload_state_receiver) =
            tokio::sync::mpsc::channel::<WorkloadStateSpec>(BUFFER_SIZE);

        // [impl->swdd~agent-prepares-dedicated-run-folder~3]
        let run_directory = io_utils::prepare_agent_run_directory(
            agent_config.run_folder.as_str(),
            agent_config.name.as_str(),
        )
        .unwrap_or_exit("Run folder creation failed. Cannot continue without run folder.");

        // [impl->swdd~agent-allows-enabled-runtimes~1]
        let selected_runtimes: HashMap<String, Box<dyn OwnableRuntime>> =
            match &agent_config.runtimes {
                Some(enabled_runtimes) => {
                    let selected = self.available_runtimes.select(enabled_runtimes);
                    if selected.is_empty() {
                        Err(format!(
                            "No valid runtimes configured. Enabled runtimes: {enabled_runtimes:?}"
                        ))
                    } else {
                        Ok(selected)
                    }
                    .unwrap_or_exit("Invalid runtime configuration")
                }
                None => self.available_runtimes.into(),
            };

        let run_folder_path = run_directory.get_path();
        let runtime_facade_map: HashMap<String, Box<dyn RuntimeFacade>> = selected_runtimes
            .into_iter()
            .map(|(name, runtime)| {
                let facade = Box::new(GenericRuntimeFacade::new(runtime, run_folder_path.clone()))
                    as Box<dyn RuntimeFacade>;
                (name, facade)
            })
            .collect();

        // The RuntimeManager currently directly gets the server ToServerInterface, but it shall get the agent manager interface
        // This is needed to be able to filter/authorize the commands towards the Ankaios server
        // The pipe connecting the workload to Ankaios must be in the runtime adapter
        let runtime_manager = RuntimeManager::new(
            AgentName::from(agent_config.name.as_str()),
            run_directory.get_path(),
            to_server.clone(),
            runtime_facade_map,
            workload_state_sender,
        );

        let tls_config = if agent_config.server_url.is_unix() {
            None
        } else {
            if let Err(err_message) = TLSConfig::is_config_conflicting(
                agent_config.insecure,
                &agent_config.ca_pem_content,
                &agent_config.crt_pem_content,
                &agent_config.key_pem_content,
            ) {
                log::warn!("{err_message}");
            }

            // [impl->swdd~agent-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
            // [impl->swdd~agent-provides-file-paths-to-communication-middleware~1]
            // [impl->swdd~agent-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
            TLSConfig::new(
                agent_config.insecure,
                agent_config.ca_pem_content,
                agent_config.crt_pem_content,
                agent_config.key_pem_content,
            )
            .unwrap_or_exit("Missing certificate files")
        };

        let mut communications_client = GRPCCommunicationsClient::new_agent_communication(
            agent_config.name.clone(),
            agent_config.server_url,
            agent_config.tags,
            tls_config,
        )
        .unwrap_or_exit("Failed to create communications client.");

        let mut agent_manager = AgentManager::new(
            agent_config.name.clone(),
            manager_receiver,
            runtime_manager,
            to_server,
            workload_state_receiver,
        );

        tokio::select! {
            // [impl->swdd~agent-sends-hello~1]
            // [impl->swdd~agent-default-communication-grpc~1]
            communication_result = communications_client.run(server_receiver, to_manager) => {
                communication_result.unwrap_or_exit("agent error")
            }
            _agent_mgr_result = agent_manager.start() => {
                log::info!("AgentManager exited.");
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
    use super::{AgentBuilder, OwnableRuntime};
    use crate::runtime_connectors::test::MockRuntimeConnector;

    #[test]
    fn utest_with_runtime_registers_runtime() {
        let builder = AgentBuilder::new()
            .with_runtime(Box::new(MockRuntimeConnector::new()) as Box<dyn OwnableRuntime>);

        let selected = builder
            .available_runtimes
            .select(&["mock-runtime".to_string()]);
        assert_eq!(selected.len(), 1);
    }
}
