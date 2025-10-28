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

mod ankaios_server;
mod cli;
mod server_config;

use std::fs;
use std::path::PathBuf;

use common::helpers::validate_tags;
use common::objects::CompleteState;

use common::communications_server::CommunicationsServer;
use common::objects::State;
use common::std_extensions::GracefulExitResult;

use ankaios_server::{AnkaiosServer, create_from_server_channel, create_to_server_channel};
use server_config::{DEFAULT_SERVER_CONFIG_FILE_PATH, ServerConfig};

use grpc::{security::TLSConfig, server::GRPCCommunicationsServer};

fn handle_sever_config(config_path: &Option<String>, default_path: &str) -> ServerConfig {
    match config_path {
        Some(config_path) => {
            let config_path = PathBuf::from(config_path);
            log::info!(
                "Loading server config from user provided path '{}'",
                config_path.display()
            );
            ServerConfig::from_file(config_path).unwrap_or_exit("Config file could not be parsed")
        }
        None => {
            let default_path = PathBuf::from(default_path);
            if !default_path.try_exists().unwrap_or(false) {
                log::debug!(
                    "No config file found at default path '{}'. Using cli arguments and environment variables only.",
                    default_path.display()
                );
                ServerConfig::default()
            } else {
                log::info!(
                    "Loading server config from default path '{}'",
                    default_path.display()
                );
                ServerConfig::from_file(default_path)
                    .unwrap_or_exit("Config file could not be parsed")
            }
        }
    }
}

// [impl->swdd~server-validates-startup-manifest-tags-format~1]
fn validate_tags_format_in_manifest(data: &str) -> Result<(), String> {
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(data).map_err(|e| format!("Failed to parse YAML: {e}"))?;

    let api_version = yaml_value
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if let Some(workloads) = yaml_value.get("workloads")
        && let Some(workloads_map) = workloads.as_mapping()
    {
        for (workload_name, workload_spec) in workloads_map {
            if let Some(tags_value) = workload_spec.get("tags") {
                let workload_name_str = workload_name.as_str().unwrap_or("unknown");
                validate_tags(&api_version, tags_value, workload_name_str)?;
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    // [impl->swdd~server-loads-config-file~1]
    let mut server_config = handle_sever_config(&args.config_path, DEFAULT_SERVER_CONFIG_FILE_PATH);

    server_config.update_with_args(&args);

    log::debug!(
        "Starting the Ankaios server with \n\tserver address: '{}', \n\tstartup manifest path: '{}'",
        server_config.address,
        server_config
            .startup_manifest
            .clone()
            .unwrap_or("[no manifest file provided]".to_string()),
    );

    let startup_state = match &server_config.startup_manifest {
        Some(config_path) => {
            let data =
                fs::read_to_string(config_path).unwrap_or_exit("Could not read the startup config");

            validate_tags_format_in_manifest(&data)
                .unwrap_or_exit("Invalid tags format in startup manifest");

            // [impl->swdd~server-state-in-memory~1]
            // [impl->swdd~server-loads-startup-state-file~3]
            let state: State = serde_yaml::from_str(&data)
                .unwrap_or_exit("Parsing start config failed with error");
            log::trace!(
                "The state is initialized with the following workloads: {:?}",
                state.workloads
            );
            Some(CompleteState {
                desired_state: state,
                ..Default::default()
            })
        }
        // [impl->swdd~server-starts-without-startup-config~1]
        _ => None,
    };

    let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
    let (to_agents, agents_receiver) = create_from_server_channel(common::CHANNEL_CAPACITY);

    if let Err(err_message) = TLSConfig::is_config_conflicting(
        server_config.insecure.unwrap(),
        &server_config.ca_pem_content,
        &server_config.crt_pem_content,
        &server_config.key_pem_content,
    ) {
        log::warn!("{err_message}");
    }

    // [impl->swdd~server-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
    // [impl->swdd~server-provides-file-paths-to-communication-middleware~1]
    // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
    let tls_config = TLSConfig::new(
        server_config.insecure.unwrap_or(true),
        server_config.ca_pem_content,
        server_config.crt_pem_content,
        server_config.key_pem_content,
    );

    let mut communications_server = GRPCCommunicationsServer::new(
        to_server,
        // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit("Missing certificates files"),
    );
    let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());

    tokio::select! {
        // [impl->swdd~server-default-communication-grpc~1]
        communication_result = communications_server.start(agents_receiver, server_config.address) => {
            communication_result.unwrap_or_exit("server error")
        }

        server_result = server.start(startup_state) => {
            server_result.unwrap_or_exit("server error")
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
    use crate::{
        ServerConfig, handle_sever_config, server_config::DEFAULT_SERVER_CONFIG_FILE_PATH,
        validate_tags_format_in_manifest,
    };
    use std::{io::Write, net::SocketAddr};
    use tempfile::NamedTempFile;

    const VALID_SERVER_CONFIG_CONTENT: &str = r"#
    version = 'v1'
    startup_manifest = '/workspaces/ankaios/server/resources/startConfig.yaml'
    address = '127.0.0.1:25551'
    insecure = true
    #";

    #[test]
    fn utest_handle_server_config_valid_config() {
        let mut tmp_config = NamedTempFile::new().expect("could not create temp file");
        write!(tmp_config, "{VALID_SERVER_CONFIG_CONTENT}").expect("could not write to temp file");

        let server_config = handle_sever_config(
            &Some(tmp_config.into_temp_path().to_str().unwrap().to_string()),
            DEFAULT_SERVER_CONFIG_FILE_PATH,
        );

        assert_eq!(
            server_config.startup_manifest,
            Some("/workspaces/ankaios/server/resources/startConfig.yaml".to_string())
        );
        assert_eq!(
            server_config.address,
            "127.0.0.1:25551".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(server_config.insecure, Some(true));
    }

    #[test]
    fn utest_handle_server_config_default_path() {
        let mut file = tempfile::NamedTempFile::new().expect("Failed to create file");
        writeln!(file, "{VALID_SERVER_CONFIG_CONTENT}").expect("Failed to write to file");

        let server_config = handle_sever_config(&None, file.path().to_str().unwrap());

        assert_eq!(
            server_config.startup_manifest,
            Some("/workspaces/ankaios/server/resources/startConfig.yaml".to_string())
        );
        assert_eq!(
            server_config.address,
            "127.0.0.1:25551".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(server_config.insecure, Some(true));
    }

    #[test]
    fn utest_handle_server_config_default() {
        let server_config = handle_sever_config(&None, "/a/very/invalid/path/to/config/file");

        assert_eq!(server_config, ServerConfig::default());
    }

    // [utest->swdd~server-validates-startup-manifest-tags-format~1]
    #[test]
    fn utest_validate_tags_format_current_api_version_with_mapping_ok() {
        let manifest = r#"
apiVersion: v1
workloads:
  nginx:
    agent: agent_A
    runtime: podman
    tags:
      owner: team_a
      version: "1.0"
    runtimeConfig: |
      image: nginx:latest
"#;
        assert!(validate_tags_format_in_manifest(manifest).is_ok());
    }

    // [utest->swdd~server-validates-startup-manifest-tags-format~1]
    #[test]
    fn utest_validate_tags_format_current_api_version_with_sequence_fails() {
        let manifest = r#"
apiVersion: v1
workloads:
  nginx:
    agent: agent_A
    runtime: podman
    tags:
      - key: owner
        value: team_a
      - key: version
        value: "1.0"
    runtimeConfig: |
      image: nginx:latest
"#;
        let result = validate_tags_format_in_manifest(manifest);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("tags must be specified as a mapping")
        );
    }

    // [utest->swdd~server-validates-startup-manifest-tags-format~1]
    #[test]
    fn utest_validate_tags_format_previous_api_version_with_sequence_ok() {
        let manifest = r#"
apiVersion: v0.1
workloads:
  nginx:
    agent: agent_A
    runtime: podman
    tags:
      - key: owner
        value: team_a
      - key: version
        value: "1.0"
    runtimeConfig: |
      image: nginx:latest
"#;
        assert!(validate_tags_format_in_manifest(manifest).is_ok());
    }

    // [utest->swdd~server-validates-startup-manifest-tags-format~1]
    #[test]
    fn utest_validate_tags_format_previous_api_version_with_mapping_fails() {
        let manifest = r#"
apiVersion: v0.1
workloads:
  nginx:
    agent: agent_A
    runtime: podman
    tags:
      owner: team_a
      version: "1.0"
    runtimeConfig: |
      image: nginx:latest
"#;
        let result = validate_tags_format_in_manifest(manifest);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("tags must be specified as a sequence")
        );
    }
}
