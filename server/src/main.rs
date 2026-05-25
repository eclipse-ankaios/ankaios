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
mod signature_validator;

use ankaios_api::ank_base::{CompleteStateSpec, StateSpec, validate_tags};
use ankaios_server::{AnkaiosServer, create_from_server_channel, create_to_server_channel};

use common::communications_server::CommunicationsServer;
use common::config::handle_config;
use common::std_extensions::GracefulExitResult;

use grpc::{security::TLSConfig, server::GRPCCommunicationsServer};
use server_config::{DEFAULT_SERVER_CONFIG_FILE_PATH, ServerConfig};
use signature_validator::{SignaturePolicy, SignatureValidator};

use prost::Message;
use std::fs;

#[cfg(test)]
pub mod test_helper;

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
        for (workload_name, workload) in workloads_map {
            if let Some(tags_value) = workload.get("tags") {
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

    // [impl->swdd~server-loads-config-file~2]
    let mut server_config: ServerConfig =
        handle_config(&args.config_path, &DEFAULT_SERVER_CONFIG_FILE_PATH);

    server_config
        .update_with_args(&args)
        .unwrap_or_exit("Failed to load certificate files!");

    log::debug!(
        "Starting the Ankaios server with \n\tserver address: '{}', \n\tstartup manifest path: '{}'",
        server_config.address,
        server_config
            .startup_manifest
            .clone()
            .unwrap_or("[no manifest file provided]".to_string()),
    );

    // Initialize signature validator early if enabled (needed for startup manifest verification)
    let mut signature_validator = if server_config.signature_verification.enabled {
        let policy = SignaturePolicy {
            require_signature: server_config.signature_verification.require_signature,
            require_counter: server_config.signature_verification.require_counter,
            allowed_key_ids: server_config.signature_verification.allowed_key_ids.clone(),
            min_counter: server_config.signature_verification.min_counter,
            allowed_restoration_plugins: server_config
                .signature_verification
                .allowed_restoration_plugins
                .clone(),
            restoration_window_seconds: server_config.signature_verification.restoration_window_seconds,
        };

        match SignatureValidator::from_keys_directory(
            &server_config.signature_verification.keys_directory,
            policy,
        ) {
            Ok(validator) => {
                log::info!("✅ Signature verification enabled");
                Some(validator)
            }
            Err(e) => {
                log::error!("Failed to initialize signature validator: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        log::info!("Signature verification disabled");
        None
    };

    let startup_state = match &server_config.startup_manifest {
        Some(config_path) => {
            // Check if file is binary protobuf (.pb) or text YAML
            let is_binary = config_path.ends_with(".pb");

            let state_yaml = if is_binary {
                // Binary protobuf format (.pb file created by ank sign)
                let data = fs::read(config_path)
                    .unwrap_or_exit("Could not read the startup config");

                match ankaios_api::ank_base::UpdateStateRequest::decode(&data[..]) {
                    Ok(update_request) => {
                        log::debug!("Successfully decoded binary protobuf startup manifest ({} bytes)", data.len());

                        if let Some(ref metadata) = update_request.signature_metadata {
                            log::debug!("Signature metadata present: key_id={}, counter={}, timestamp={}, sig_len={}",
                                metadata.key_id, metadata.counter, metadata.timestamp, metadata.signature.len());
                        } else {
                            log::debug!("No signature metadata in startup manifest");
                        }

                        // Verify signature if validator is enabled
                        if let Some(ref mut validator) = signature_validator {
                            log::debug!("Verifying signature for startup manifest...");
                            match validator.verify_update_request(&update_request, "startup-manifest") {
                                Ok(()) => {
                                    if let Some(ref metadata) = update_request.signature_metadata {
                                        log::info!(
                                            "✅ Startup manifest signature verified: key_id={}, counter={}",
                                            metadata.key_id,
                                            metadata.counter
                                        );
                                    }
                                }
                                Err(e) => {
                                    log::error!("❌ Startup manifest signature verification failed: {:?}", e);
                                    if server_config.signature_verification.require_signature {
                                        std::process::exit(1);
                                    } else {
                                        log::warn!("⚠️  Accepting unsigned manifest (policy allows)");
                                    }
                                }
                            }
                        }

                        // Extract State from UpdateStateRequest
                        if let Some(complete_state) = update_request.new_state {
                            if let Some(desired_state) = complete_state.desired_state {
                                serde_yaml::to_string(&desired_state)
                                    .unwrap_or_exit("Failed to serialize state from UpdateStateRequest")
                            } else {
                                log::error!("Startup manifest UpdateStateRequest missing desiredState");
                                std::process::exit(1);
                            }
                        } else {
                            log::error!("Startup manifest UpdateStateRequest missing newState");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to decode binary protobuf startup manifest: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // Text YAML format - must be unsigned
                // Signed manifests MUST use binary .pb format to avoid YAML parsing ambiguity
                let data = fs::read_to_string(config_path)
                    .unwrap_or_exit("Could not read the startup config");

                if signature_validator.is_some() && server_config.signature_verification.require_signature {
                    log::error!("❌ Startup manifest is YAML but require_signature is enabled");
                    log::error!("💡 Signed manifests must be binary .pb format (use 'ank sign')");
                    log::error!("💡 YAML format is only supported for unsigned manifests");
                    std::process::exit(1);
                }

                if signature_validator.is_some() {
                    log::warn!("⚠️  Accepting unsigned YAML manifest (require_signature is false)");
                }

                data
            };

            validate_tags_format_in_manifest(&state_yaml)
                .unwrap_or_exit("Invalid tags format in startup manifest");

            // [impl->swdd~server-state-in-memory~1]
            // [impl->swdd~server-loads-startup-state-file~3]
            let state: StateSpec = serde_yaml::from_str(&state_yaml)
                .unwrap_or_exit("Parsing start config failed with error");
            log::trace!(
                "The state is initialized with the following workloads: {:?}",
                state.workloads
            );
            Some(CompleteStateSpec {
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
        server_config.insecure,
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
        server_config.insecure,
        server_config.ca_pem_content,
        server_config.crt_pem_content,
        server_config.key_pem_content,
    );

    let mut communications_server = GRPCCommunicationsServer::new(
        to_server,
        // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit("Missing certificate files"),
    );

    let mut server = AnkaiosServer::new_with_validator(
        server_receiver,
        to_agents.clone(),
        signature_validator,
    );

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
    use crate::validate_tags_format_in_manifest;

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
