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

mod agent_config;
mod agent_manager;
mod cli;
mod control_interface;
mod runtime_connectors;
#[cfg(test)]
pub mod test_helper;
mod workload_operation;

mod generic_polling_state_checker;
mod resource_monitor;
mod runtime_manager;
mod subscription_store;
mod workload;
mod workload_files;
mod workload_log_facade;
mod workload_scheduler;
mod workload_state;

mod io_utils;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;

use agent_config::{AgentConfig, DEFAULT_AGENT_CONFIG_FILE_PATH};
use agent_manager::AgentManager;
use ankaios_api::{ALLOWED_CHAR_SET, ank_base::WorkloadStateSpec};

use common::{
    communications_client::CommunicationsClient,
    config::handle_config,
    from_server_interface::FromServer,
    objects::AgentName,
    std_extensions::{GracefulExitResult, IllegalStateResult},
    to_server_interface::ToServer,
};

use generic_polling_state_checker::GenericPollingStateChecker;
use runtime_connectors::{
    GenericRuntimeFacade, RuntimeConnector, RuntimeFacade, SUPPORTED_RUNTIMES,
    containerd::{self, ContainerdRuntime, ContainerdWorkloadId},
    podman::{self, PodmanRuntime, PodmanWorkloadId},
    podman_kube::{self, PodmanKubeRuntime, PodmanKubeWorkloadId},
};

use grpc::client::GRPCCommunicationsClient;
use grpc::security::TLSConfig;
use regex::Regex;
use std::collections::HashMap;

const BUFFER_SIZE: usize = 20;

// [impl->swdd~agent-naming-convention~1]
pub fn validate_agent_name(agent_name: &str) -> Result<(), String> {
    const EXPECTED_AGENT_NAME_FORMAT: &str = "It shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols '-' and '_'.";
    if agent_name.is_empty() {
        return Err(format!(
            "Empty agent name is not allowed. {EXPECTED_AGENT_NAME_FORMAT}"
        ));
    }

    let re = Regex::new(&format!(r"^{ALLOWED_CHAR_SET}+$")).unwrap_or_illegal_state();

    if re.is_match(agent_name) {
        Ok(())
    } else {
        Err(format!(
            "Agent name '{agent_name}' is invalid. {EXPECTED_AGENT_NAME_FORMAT}",
        ))
    }
}

pub fn validate_runtimes(config_runtimes: &Option<Vec<String>>) -> Result<Vec<&str>, String> {
    match config_runtimes {
        Some(configured_runtimes) => {
            let mut valid_runtimes = Vec::new();

            for runtime in configured_runtimes {
                if SUPPORTED_RUNTIMES.contains(&runtime.as_str()) {
                    valid_runtimes.push(runtime.as_str());
                } else {
                    log::warn!("Configured runtime '{runtime}' is not supported.");
                }
            }

            if valid_runtimes.is_empty() {
                Err(format!(
                    "No valid runtimes configured. Supported runtimes: {SUPPORTED_RUNTIMES:?}"
                ))
            } else {
                log::debug!("Runtimes configured: {valid_runtimes:?}");
                Ok(valid_runtimes)
            }
        }
        None => {
            log::debug!(
                "No runtimes configured. Using all supported runtimes: {SUPPORTED_RUNTIMES:?}"
            );
            Ok(SUPPORTED_RUNTIMES.to_vec())
        }
    }
}

macro_rules! register_runtime {
    ($map:expr, $runtime_instance:expr, $workload_id_type:ty, $run_path:expr) => {{
        let runtime = Box::new($runtime_instance);
        let runtime_name = runtime.name();
        let podman_facade = Box::new(GenericRuntimeFacade::<
            $workload_id_type,
            GenericPollingStateChecker,
        >::new(runtime, $run_path));
        $map.insert(runtime_name, podman_facade);
    }};
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    // [impl->swdd~agent-loads-config-file~1]
    let mut agent_config: AgentConfig =
        handle_config(&args.config_path, DEFAULT_AGENT_CONFIG_FILE_PATH);

    agent_config.update_with_args(&args);

    validate_agent_name(&agent_config.name)
        .unwrap_or_exit("Error encountered while checking agent name!");

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

    // [impl->swdd~agent-prepares-dedicated-run-folder~2]
    let run_directory = io_utils::prepare_agent_run_directory(
        agent_config.run_folder.as_str(),
        agent_config.name.as_str(),
    )
    .unwrap_or_exit("Run folder creation failed. Cannot continue without run folder.");

    let mut runtime_facade_map: HashMap<String, Box<dyn RuntimeFacade>> = HashMap::new();

    // [impl->swdd~agent-allows-enabled-runtimes~1]
    let runtimes_to_register: Vec<&str> =
        validate_runtimes(&agent_config.runtimes).unwrap_or_exit("Invalid runtime configuration");
    for runtime_name in runtimes_to_register {
        match runtime_name {
            podman::NAME => {
                // [impl->swdd~agent-supports-podman~2]
                register_runtime!(
                    runtime_facade_map,
                    PodmanRuntime {},
                    PodmanWorkloadId,
                    run_directory.get_path()
                );
            }
            podman_kube::NAME => {
                // [impl->swdd~agent-supports-podman-kube-runtime~1]
                register_runtime!(
                    runtime_facade_map,
                    PodmanKubeRuntime {},
                    PodmanKubeWorkloadId,
                    run_directory.get_path()
                );
            }
            containerd::NAME => {
                // [impl->swdd~agent-supports-containerd~1]
                register_runtime!(
                    runtime_facade_map,
                    ContainerdRuntime {},
                    ContainerdWorkloadId,
                    run_directory.get_path()
                );
            }
            _ => {
                log::error!("Unexpected runtime name to register: {runtime_name}");
            }
        }
    }

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
    let tls_config = TLSConfig::new(
        agent_config.insecure,
        agent_config.ca_pem_content,
        agent_config.crt_pem_content,
        agent_config.key_pem_content,
    );

    let mut communications_client = GRPCCommunicationsClient::new_agent_communication(
        agent_config.name.clone(),
        agent_config.server_url,
        agent_config.tags,
        // [impl->swdd~agent-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit("Missing certificate file"),
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::{SUPPORTED_RUNTIMES, validate_agent_name, validate_runtimes};

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_ok() {
        let name = "test_AgEnt-name1_56";
        assert!(validate_agent_name(name).is_ok());
    }

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_fail() {
        let invalid_agent_names = ["a.b", "a_b_%#", "a b"];
        for name in invalid_agent_names {
            let result = validate_agent_name(name);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .contains(&format!("Agent name '{name}' is invalid.",))
            );
        }

        let result = validate_agent_name("");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Empty agent name is not allowed.")
        );
    }

    // [utest->swdd~agent-allows-enabled-runtimes~1]
    #[test]
    fn utest_validate_runtimes() {
        let runtimes_len = SUPPORTED_RUNTIMES.len();

        assert_eq!(validate_runtimes(&None).unwrap().len(), runtimes_len);

        let empty_runtimes_list = Some(Vec::new());
        let result = validate_runtimes(&empty_runtimes_list);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No valid runtimes configured"));

        let all_valid_runtimes = Some(SUPPORTED_RUNTIMES.iter().map(|s| s.to_string()).collect());
        assert_eq!(
            validate_runtimes(&all_valid_runtimes).unwrap().len(),
            runtimes_len
        );

        let single_runtime = Some(vec![SUPPORTED_RUNTIMES[0].to_string()]);
        let result = validate_runtimes(&single_runtime).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], SUPPORTED_RUNTIMES[0]);

        let invalid_runtime = Some(vec!["invalid_runtime".to_string()]);
        let result = validate_runtimes(&invalid_runtime);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No valid runtimes configured"));

        let mixed_runtimes = Some(vec![
            SUPPORTED_RUNTIMES[0].to_string(),
            "invalid_runtime".to_string(),
            SUPPORTED_RUNTIMES[2].to_string(),
        ]);
        let result = validate_runtimes(&mixed_runtimes).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&SUPPORTED_RUNTIMES[0]));
        assert!(result.contains(&SUPPORTED_RUNTIMES[2]));
    }
}
