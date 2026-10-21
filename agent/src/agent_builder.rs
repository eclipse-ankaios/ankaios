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
use std::collections::HashMap;

pub(crate) const BUFFER_SIZE: usize = 20;

/// Builder for assembling and running an Ankaios agent with a chosen set of runtimes.
///
/// Config is resolved by the caller (e.g. via `AgentConfig::load()`) before building runtimes or
/// a communications client, so both can be constructed from the resolved `AgentConfig`.
pub struct AgentBuilder {
    available_runtimes: RuntimeRegistry,
    communications_client: Box<dyn CommunicationsClient>,
}

impl AgentBuilder {
    pub fn new(communications_client: Box<dyn CommunicationsClient>) -> Self {
        Self {
            available_runtimes: RuntimeRegistry::default(),
            communications_client,
        }
    }

    pub fn with_runtime(mut self, runtime: Box<dyn OwnableRuntime>) -> Self {
        self.available_runtimes.register(runtime);
        self
    }

    // [impl->swdd~agent-default-communication-grpc~1]
    pub fn default_communications_client(
        agent_config: &AgentConfig,
    ) -> Box<dyn CommunicationsClient> {
        let tls_config = agent_config.build_tls_config();
        Box::new(
            GRPCCommunicationsClient::new_agent_communication(
                agent_config.name.clone(),
                agent_config.server_url.clone(),
                agent_config.tags.clone(),
                tls_config,
            )
            .unwrap_or_exit("Failed to create communications client."),
        )
    }

    pub async fn run(mut self, agent_config: AgentConfig) {
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

        let mut agent_manager = AgentManager::new(
            agent_config.name.clone(),
            manager_receiver,
            runtime_manager,
            to_server,
            workload_state_receiver,
        );

        tokio::select! {
            // [impl->swdd~agent-sends-hello~1]
            communication_result = self.communications_client.run(server_receiver, to_manager) => {
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
    use common::communications_client::CommunicationsClient;
    use common::communications_error::CommunicationMiddlewareError;
    use common::from_server_interface::FromServerSender;
    use common::to_server_interface::ToServerReceiver;

    #[test]
    fn utest_with_runtime_registers_runtime() {
        let builder =
            AgentBuilder::new(Box::new(FakeCommunicationsClient) as Box<dyn CommunicationsClient>)
                .with_runtime(Box::new(MockRuntimeConnector::new()) as Box<dyn OwnableRuntime>);

        let selected = builder
            .available_runtimes
            .select(&["mock-runtime".to_string()]);
        assert_eq!(selected.len(), 1);
    }

    #[derive(Debug, Default)]
    struct FakeCommunicationsClient;

    #[async_trait::async_trait]
    impl CommunicationsClient for FakeCommunicationsClient {
        async fn run(
            &mut self,
            _receiver: ToServerReceiver,
            _manager_interface: FromServerSender,
        ) -> Result<(), CommunicationMiddlewareError> {
            Ok(())
        }
    }
}
