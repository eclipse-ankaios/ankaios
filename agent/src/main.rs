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

use common::communications_client::CommunicationsClient;
use common::objects::{AgentName, WorkloadState, STR_RE_AGENT};
use common::to_server_interface::ToServer;
use generic_polling_state_checker::GenericPollingStateChecker;
use grpc::security::TLSConfig;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;

mod agent_config;
mod agent_manager;
mod cli;
mod control_interface;
mod runtime_connectors;
#[cfg(test)]
pub mod test_helper;
mod workload_operation;

mod generic_polling_state_checker;
mod runtime_manager;
mod subscription_store;
mod workload;
mod workload_files;
mod workload_log_facade;
mod workload_scheduler;
mod workload_state;

mod io_utils;

use common::from_server_interface::FromServer;
use common::std_extensions::GracefulExitResult;
use grpc::client::GRPCCommunicationsClient;

use agent_config::{AgentConfig, DEFAULT_AGENT_CONFIG_FILE_PATH};
use agent_manager::AgentManager;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
use runtime_connectors::{
    podman::{PodmanRuntime, PodmanWorkloadId},
    podman_kube::{PodmanKubeRuntime, PodmanKubeWorkloadId},
    GenericRuntimeFacade, RuntimeConnector, RuntimeFacade,
};

const BUFFER_SIZE: usize = 20;

// [impl->swdd~agent-loads-config-file~1]
fn handle_agent_config(config_path: &Option<String>, default_path: &str) -> AgentConfig {
    match config_path {
        Some(config_path) => {
            let config_path = PathBuf::from(config_path);
            log::info!(
                "Loading agent config from user provided path '{}'",
                config_path.display()
            );
            AgentConfig::from_file(config_path).unwrap_or_exit("Config file could not be parsed")
        }
        None => {
            let default_path = PathBuf::from(default_path);
            if !default_path.try_exists().unwrap_or(false) {
                log::debug!("No config file found at default path '{}'. Using cli arguments and environment variables only.", default_path.display());
                AgentConfig::default()
            } else {
                log::info!(
                    "Loading agent config from default path '{}'",
                    default_path.display()
                );
                AgentConfig::from_file(default_path)
                    .unwrap_or_exit("Config file could not be parsed")
            }
        }
    }
}

// [impl->swdd~agent-naming-convention~1]
pub fn validate_agent_name(agent_name: &str) -> Result<(), String> {
    const EXPECTED_AGENT_NAME_FORMAT: &str = "It shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols '-' and '_'.";
    if agent_name.is_empty() {
        return Err(format!(
            "Empty agent name is not allowed. {}",
            EXPECTED_AGENT_NAME_FORMAT
        ));
    }

    let re = Regex::new(STR_RE_AGENT).unwrap();

    if re.is_match(agent_name) {
        Ok(())
    } else {
        Err(format!(
            "Agent name '{}' is invalid. {}",
            agent_name, EXPECTED_AGENT_NAME_FORMAT,
        ))
    }
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    // [impl->swdd~agent-loads-config-file~1]
    let mut agent_config = handle_agent_config(&args.config_path, DEFAULT_AGENT_CONFIG_FILE_PATH);

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
        tokio::sync::mpsc::channel::<WorkloadState>(BUFFER_SIZE);

    // [impl->swdd~agent-prepares-dedicated-run-folder~1]
    let run_directory = io_utils::prepare_agent_run_directory(
        agent_config.run_folder.as_str(),
        agent_config.name.as_str(),
    )
    .unwrap_or_exit("Run folder creation failed. Cannot continue without run folder.");

    // [impl->swdd~agent-supports-podman~2]
    let podman_runtime = Box::new(PodmanRuntime {});
    let podman_runtime_name = podman_runtime.name();
    let podman_facade = Box::new(GenericRuntimeFacade::<
        PodmanWorkloadId,
        GenericPollingStateChecker,
    >::new(podman_runtime, run_directory.get_path()));
    let mut runtime_facade_map: HashMap<String, Box<dyn RuntimeFacade>> = HashMap::new();
    runtime_facade_map.insert(podman_runtime_name, podman_facade);

    // [impl->swdd~agent-supports-podman-kube-runtime~1]
    let podman_kube_runtime = Box::new(PodmanKubeRuntime {});
    let podman_kube_runtime_name = podman_kube_runtime.name();
    let podman_kube_facade = Box::new(GenericRuntimeFacade::<
        PodmanKubeWorkloadId,
        GenericPollingStateChecker,
    >::new(podman_kube_runtime, run_directory.get_path()));
    runtime_facade_map.insert(podman_kube_runtime_name, podman_kube_facade);

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
        log::warn!("{}", err_message);
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
    use crate::{agent_config::DEFAULT_AGENT_CONFIG_FILE_PATH, handle_agent_config, AgentConfig};
    use std::io::Write;
    use tempfile::NamedTempFile;

    const VALID_AGENT_CONFIG_CONTENT: &str = r"#
    version = 'v1'
    name = 'agent_1'
    server_url = 'https://127.0.0.1:25551'
    run_folder = '/tmp/ankaios/'
    insecure = true
    #";

    #[test]
    fn utest_handle_agent_config_valid_config() {
        let mut tmp_config = NamedTempFile::new().expect("could not create temp file");
        write!(tmp_config, "{}", VALID_AGENT_CONFIG_CONTENT).expect("could not write to temp file");

        let agent_config = handle_agent_config(
            &Some(tmp_config.into_temp_path().to_str().unwrap().to_string()),
            DEFAULT_AGENT_CONFIG_FILE_PATH,
        );

        assert_eq!(agent_config.name, "agent_1".to_string());
        assert_eq!(
            agent_config.server_url,
            "https://127.0.0.1:25551".to_string()
        );
        assert_eq!(agent_config.run_folder, "/tmp/ankaios/".to_string());
        assert!(agent_config.insecure);
    }

    #[test]
    fn utest_handle_agent_config_default_path() {
        let mut file = tempfile::NamedTempFile::new().expect("Failed to create file");
        writeln!(file, "{}", VALID_AGENT_CONFIG_CONTENT).expect("Failed to write to file");

        let agent_config = handle_agent_config(&None, file.path().to_str().unwrap());

        assert_eq!(agent_config.name, "agent_1".to_string());
        assert_eq!(
            agent_config.server_url,
            "https://127.0.0.1:25551".to_string()
        );
        assert_eq!(agent_config.run_folder, "/tmp/ankaios/".to_string());
        assert!(agent_config.insecure);
    }

    #[test]
    fn utest_handle_agent_config_default() {
        let agent_config = handle_agent_config(&None, "/a/very/invalid/path/to/config/file");

        assert_eq!(agent_config, AgentConfig::default());
    }

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_ok() {
        let name = "test_AgEnt-name1_56";
        assert!(super::validate_agent_name(name).is_ok());
    }

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_fail() {
        let invalid_agent_names = ["a.b", "a_b_%#", "a b"];
        for name in invalid_agent_names {
            let result = super::validate_agent_name(name);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .contains(&format!("Agent name '{name}' is invalid.",)));
        }

        let result = super::validate_agent_name("");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Empty agent name is not allowed."));
    }
}
