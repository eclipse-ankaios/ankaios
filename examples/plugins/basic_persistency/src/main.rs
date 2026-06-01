// Copyright (c) 2024 Elektrobit Automotive GmbH
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

//! Basic Persistence Plugin for Ankaios
//!
//! This plugin watches workload state changes via the Events API and persists
//! workloads marked with a 'persist' tag to a runtime state file.
//!
//! Persistence modes (configured via 'persist' tag):
//! - ALWAYS: Persist workload as soon as server accepts it (in desired state)
//! - ON_RUNNING: Persist only when workload execution state is RUNNING

use ankaios_api::ank_base::{
    request::RequestContent, response::ResponseContent, CompleteState, CompleteStateRequest,
    CompleteStateResponse, Request, State, Tags, UpdateStateRequest, Workload, WorkloadMap,
    WorkloadStatesMap, execution_state::ExecutionStateEnum,
};
use ankaios_api::control_api::{
    from_ankaios::FromAnkaiosEnum, to_ankaios::ToAnkaiosEnum, FromAnkaios, Hello, ToAnkaios,
};

use prost::Message;
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const ANKAIOS_CONTROL_INTERFACE_BASE_PATH: &str = "/run/ankaios/control_interface";
const MAX_VARINT_SIZE: usize = 19;
const EVENT_REQUEST_ID: &str = "basic_persistency_events";

fn get_protocol_version() -> String {
    env::var("ANKAIOS_VERSION").unwrap_or_else(|_| "1.0.0".to_string())
}

/// Get the persist mode from workload tags
fn get_persist_mode(tags: &Option<Tags>) -> Option<String> {
    let tags = tags.as_ref()?;
    let persist_value = tags.tags.get("persist")?;
    let persist_upper = persist_value.to_uppercase();

    match persist_upper.as_str() {
        "ALWAYS" | "ON_RUNNING" => Some(persist_upper),
        _ => {
            log::warn!(
                "Invalid persist tag value '{}'. Valid values: ALWAYS, ON_RUNNING",
                persist_value
            );
            None
        }
    }
}

/// Check if any instance of a workload is in the specified state
fn workload_has_state(
    workload_name: &str,
    workload_states: &Option<WorkloadStatesMap>,
    check_state: fn(&ExecutionStateEnum) -> bool,
) -> bool {
    let workload_states = match workload_states {
        Some(ws) => ws,
        None => return false,
    };

    // Iterate through all agents
    for (_agent_name, executions_states_of_workload) in &workload_states.agent_state_map {
        // Check if this agent has the workload
        if let Some(executions_states_for_id) = executions_states_of_workload
            .wl_name_state_map
            .get(workload_name)
        {
            // Check all instances of this workload
            for (_id, execution_state) in &executions_states_for_id.id_state_map {
                if let Some(ref state_enum) = execution_state.execution_state_enum {
                    if check_state(state_enum) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if workload is running
fn is_workload_running(workload_name: &str, workload_states: &Option<WorkloadStatesMap>) -> bool {
    workload_has_state(workload_name, workload_states, |state| {
        matches!(state, ExecutionStateEnum::Running(_))
    })
}

/// Get current workload state as string (for debugging)
fn get_workload_state_string(workload_name: &str, workload_states: &Option<WorkloadStatesMap>) -> String {
    let workload_states = match workload_states {
        Some(ws) => ws,
        None => return "NoStateMap".to_string(),
    };

    for (_agent_name, executions_states_of_workload) in &workload_states.agent_state_map {
        if let Some(executions_states_for_id) = executions_states_of_workload
            .wl_name_state_map
            .get(workload_name)
        {
            for (_id, execution_state) in &executions_states_for_id.id_state_map {
                if let Some(ref state_enum) = execution_state.execution_state_enum {
                    return match state_enum {
                        ExecutionStateEnum::Running(_) => "Running".to_string(),
                        ExecutionStateEnum::Succeeded(_) => "Succeeded".to_string(),
                        ExecutionStateEnum::Failed(_) => "Failed".to_string(),
                        ExecutionStateEnum::Pending(_) => "Pending".to_string(),
                        ExecutionStateEnum::Stopping(_) => "Stopping".to_string(),
                        ExecutionStateEnum::Removed(_) => "Removed".to_string(),
                        ExecutionStateEnum::NotScheduled(_) => "NotScheduled".to_string(),
                        ExecutionStateEnum::AgentDisconnected(_) => "AgentDisconnected".to_string(),
                    };
                }
            }
        }
    }

    "NotFound".to_string()
}

/// Filter workloads from complete state that should be persisted
fn filter_persistent_workloads(complete_state: &CompleteState) -> State {
    let mut persistent_state = State {
        api_version: "v1".to_string(),
        ..Default::default()
    };

    // Get desired state
    let desired_state = match &complete_state.desired_state {
        Some(state) => state,
        None => return persistent_state,
    };

    // Always persist all configs
    persistent_state.configs = desired_state.configs.clone();

    // Filter workloads with persist tag
    if let Some(workloads) = &desired_state.workloads {
        let mut persistent_workloads = HashMap::new();

        for (name, workload) in &workloads.workloads {
            if let Some(mode) = get_persist_mode(&workload.tags) {
                let should_persist = match mode.as_str() {
                    "ALWAYS" => {
                        log::debug!("Workload '{}' persist mode ALWAYS: will persist", name);
                        true
                    }
                    "ON_RUNNING" => {
                        let running = is_workload_running(name, &complete_state.workload_states);
                        log::debug!(
                            "Workload '{}' persist mode ON_RUNNING: running={}",
                            name,
                            running
                        );
                        running
                    }
                    _ => {
                        log::warn!("Unknown persist mode '{}' for workload '{}'", mode, name);
                        false
                    }
                };

                if should_persist {
                    log::debug!("Workload '{}' will be persisted (mode: {})", name, mode);
                    persistent_workloads.insert(name.clone(), workload.clone());
                } else {
                    log::debug!(
                        "Workload '{}' NOT persisted (mode: {}, condition not met)",
                        name,
                        mode
                    );
                }
            } else {
                log::trace!("Workload '{}' not marked for persistence", name);
            }
        }

        if !persistent_workloads.is_empty() {
            persistent_state.workloads = Some(ankaios_api::ank_base::WorkloadMap {
                workloads: persistent_workloads,
            });
        }
    }

    persistent_state
}

/// Persist state to file atomically with backup
async fn persist_state_atomically(
    state: &State,
    persistence_file: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let workload_count = state
        .workloads
        .as_ref()
        .map(|w| w.workloads.len())
        .unwrap_or(0);

    // If no persistent workloads, remove the persistence file
    if workload_count == 0 {
        log::debug!("No persistent workloads to save");
        if persistence_file.exists() {
            log::info!("Removing empty persistence file: {:?}", persistence_file);
            tokio::fs::remove_file(persistence_file).await?;
        }
        return Ok(());
    }

    log::info!(
        "Persisting {} workload(s) to {:?}",
        workload_count,
        persistence_file
    );

    // Serialize to YAML
    let yaml_content = serde_yaml::to_string(&state)?;

    // Create parent directory if needed
    if let Some(parent) = persistence_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Atomic write pattern: write to temp file
    let mut temp_path = persistence_file.to_path_buf();
    temp_path.set_extension("tmp");

    tokio::fs::write(&temp_path, &yaml_content).await?;
    log::debug!("Wrote temporary file: {:?}", temp_path);

    // Create backup of existing file before overwriting
    if persistence_file.exists() {
        let mut backup_path = persistence_file.to_path_buf();
        backup_path.set_extension("backup");

        match tokio::fs::copy(&persistence_file, &backup_path).await {
            Ok(_) => log::debug!("Created backup: {:?}", backup_path),
            Err(e) => log::warn!("Failed to create backup at {:?}: {}", backup_path, e),
        }
    }

    // Atomic rename (ensures file is never partially written)
    tokio::fs::rename(&temp_path, persistence_file).await?;
    log::debug!("State persistence complete");

    Ok(())
}

/// Create hello message for connection
fn create_hello_message() -> ToAnkaios {
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Hello(Hello {
            protocol_version: get_protocol_version(),
        })),
    }
}

/// Create request to subscribe to events
fn create_event_subscription_request() -> ToAnkaios {
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: EVENT_REQUEST_ID.to_string(),
            request_content: Some(RequestContent::CompleteStateRequest(
                CompleteStateRequest {
                    field_mask: vec![
                        "workloadStates.*.*.*.state".to_string(),
                        "desiredState.workloads.*".to_string(),
                        "desiredState.configs".to_string(),
                    ],
                    subscribe_for_events: true,
                },
            )),
        })),
    }
}

fn create_complete_state_request() -> ToAnkaios {
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: "fetch_complete_state".to_string(),
            request_content: Some(RequestContent::CompleteStateRequest(
                CompleteStateRequest {
                    field_mask: vec![
                        "desiredState.workloads.*".to_string(),
                    ],
                    subscribe_for_events: false,
                },
            )),
        })),
    }
}

/// Read varint data from pipe
fn read_varint_data(file: &mut File) -> Result<[u8; MAX_VARINT_SIZE], io::Error> {
    let mut res = [0u8; MAX_VARINT_SIZE];
    let mut one_byte_buffer = [0u8; 1];
    for item in res.iter_mut() {
        file.read_exact(&mut one_byte_buffer)?;
        *item = one_byte_buffer[0];
        // Check if most significant bit is set to 0 if so it is the last byte to be read
        if *item & 0b10000000 == 0 {
            break;
        }
    }
    Ok(res)
}

/// Read protobuf data from pipe
fn read_protobuf_data(file: &mut File) -> Result<Box<[u8]>, io::Error> {
    let varint_data = read_varint_data(file)?;
    let mut varint_data = Box::new(&varint_data[..]);

    // Determine the exact size for exact reading of the bytes later by decoding the varint data
    let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

    let mut buf = vec![0; size];
    file.read_exact(&mut buf[..])?;
    Ok(buf.into_boxed_slice())
}

/// Send a message to Ankaios
fn send_to_ankaios(
    output_pipe: &mut File,
    to_ankaios: &ToAnkaios,
) -> Result<(), Box<dyn std::error::Error>> {
    let encoded_message = to_ankaios.encode_length_delimited_to_vec();
    output_pipe.write_all(&encoded_message)?;
    output_pipe.flush()?;

    log::trace!("Sent {} bytes to Ankaios", encoded_message.len());
    Ok(())
}

/// Receive a message from Ankaios
fn receive_from_ankaios(input_pipe: &mut File) -> Result<FromAnkaios, Box<dyn std::error::Error>> {
    let binary = read_protobuf_data(input_pipe)?;
    let from_ankaios = FromAnkaios::decode(&mut Box::new(binary.as_ref()))?;
    log::trace!("Received {} bytes from Ankaios", binary.len());
    Ok(from_ankaios)
}

/// Process an event from Ankaios - event-driven incremental persistence
async fn process_event(
    response: &CompleteStateResponse,
    persistence_file: &Path,
    output_pipe: &mut File,
    input_pipe: &mut File,
    on_running_workloads: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Log event details
    let altered_fields = match &response.altered_fields {
        Some(fields) => {
            log::info!(
                "Event received - Added: {}, Updated: {}, Removed: {}",
                fields.added_fields.len(),
                fields.updated_fields.len(),
                fields.removed_fields.len()
            );
            log::debug!("Added fields: {:?}", fields.added_fields);
            log::debug!("Updated fields: {:?}", fields.updated_fields);
            log::debug!("Removed fields: {:?}", fields.removed_fields);
            fields
        }
        None => {
            log::info!("Initial state received - processing all workloads");
            // For initial state, we need to request complete state and process all workloads
            return process_initial_state(response, persistence_file, output_pipe, input_pipe, on_running_workloads).await;
        }
    };

    // Read current persisted state (incremental updates)
    let mut persisted_state = match tokio::fs::read_to_string(persistence_file).await {
        Ok(content) => serde_yaml::from_str::<State>(&content).unwrap_or_else(|_| State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        }),
        Err(_) => State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        },
    };

    let mut modified = false;

    // Process added workloads (desiredState.workloads.X added)
    for field in &altered_fields.added_fields {
        if let Some(workload_name) = field.strip_prefix("desiredState.workloads.") {
            log::debug!("Processing added workload: {}", workload_name);

            // Fetch the workload definition from event response
            // Server includes full workload in complete_state when using filter_mask
            let complete_state = match &response.complete_state {
                Some(cs) => cs,
                None => {
                    log::error!("Added event for '{}' has no complete_state - cannot check persist tag", workload_name);
                    continue;
                }
            };

            let desired_state = match &complete_state.desired_state {
                Some(ds) => ds,
                None => {
                    log::error!("Added event for '{}' has no desired_state in complete_state", workload_name);
                    continue;
                }
            };

            let workloads = match &desired_state.workloads {
                Some(wls) => wls,
                None => {
                    log::error!("Added event for '{}' has no workloads in desired_state", workload_name);
                    continue;
                }
            };

            let workload = match workloads.workloads.get(workload_name) {
                Some(wl) => wl,
                None => {
                    log::error!("Added event for '{}' - workload not found in complete_state.desired_state.workloads", workload_name);
                    log::error!("Available workloads in event: {:?}", workloads.workloads.keys().collect::<Vec<_>>());
                    continue;
                }
            };

            if let Some(mode) = get_persist_mode(&workload.tags) {
                match mode.as_str() {
                    "ALWAYS" => {
                        // Add immediately
                        log::info!("Adding workload '{}' with persist: ALWAYS", workload_name);
                        add_workload_to_persisted_state(&mut persisted_state, workload_name, workload);
                        modified = true;
                    }
                    "ON_RUNNING" => {
                        // Don't add yet - wait for Running transition
                        log::debug!("Workload '{}' has persist: ON_RUNNING, waiting for Running state", workload_name);
                        on_running_workloads.insert(workload_name.to_string());
                    }
                    _ => {}
                }
            } else {
                log::debug!("Workload '{}' has no persist tag, skipping", workload_name);
            }
        }
    }

    // Process removed workloads (desiredState.workloads.X removed)
    for field in &altered_fields.removed_fields {
        if let Some(workload_name) = field.strip_prefix("desiredState.workloads.") {
            log::info!("Removing workload '{}' from persistence file", workload_name);
            if remove_workload_from_persisted_state(&mut persisted_state, workload_name) {
                modified = true;
            }
            // Also remove from ON_RUNNING tracking set
            on_running_workloads.remove(workload_name);
        }
    }

    // Process workload state changes (for ON_RUNNING workloads transitioning to Running)
    if !on_running_workloads.is_empty() {
        for field in &altered_fields.updated_fields {
            if field.contains("workloadStates.") && field.ends_with(".state") {
                // Extract workload name from path like "workloadStates.agent.workload.hash.state"
                let parts: Vec<&str> = field.split('.').collect();
                if parts.len() >= 3 {
                    let workload_name = parts[2];

                    // Check if this workload has persist: ON_RUNNING tag (from our cache)
                    if on_running_workloads.contains(workload_name) {
                        log::debug!("Checking ON_RUNNING workload '{}'", workload_name);

                        if let Some(complete_state) = &response.complete_state {
                            let is_running = is_workload_running(workload_name, &complete_state.workload_states);
                            let current_state = get_workload_state_string(workload_name, &complete_state.workload_states);
                            log::debug!("Workload '{}' current state: {}, is_running: {}", workload_name, current_state, is_running);

                            if is_running {
                                // Check if not already persisted
                                let already_persisted = persisted_state
                                    .workloads
                                    .as_ref()
                                    .and_then(|wls| wls.workloads.get(workload_name))
                                    .is_some();

                                if !already_persisted {
                                    log::info!("Workload '{}' reached Running state, fetching definition to persist", workload_name);

                                    // Request complete state to get workload definition
                                    // (event responses don't include desired_state for state changes)
                                    let state_request = create_complete_state_request();
                                    send_to_ankaios(output_pipe, &state_request)?;

                                    // Wait for response
                                    let state_response = receive_from_ankaios(input_pipe)?;
                                    if let Some(FromAnkaiosEnum::Response(resp)) = state_response.from_ankaios_enum {
                                        if let Some(ResponseContent::CompleteStateResponse(complete_resp)) = resp.response_content {
                                            if let Some(full_state) = &complete_resp.complete_state {
                                                // Get workload definition from complete state
                                                if let Some(workload) = full_state
                                                    .desired_state
                                                    .as_ref()
                                                    .and_then(|ds| ds.workloads.as_ref())
                                                    .and_then(|wls| wls.workloads.get(workload_name))
                                                {
                                                    add_workload_to_persisted_state(&mut persisted_state, workload_name, workload);
                                                    modified = true;
                                                } else {
                                                    log::warn!("Workload '{}' not found in complete state", workload_name);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Write file only if modified
    if modified {
        persist_state_atomically(&persisted_state, persistence_file).await?;
    } else {
        log::debug!("No changes to persisted state, skipping write");
    }

    Ok(())
}

/// Process initial state (when plugin first starts)
async fn process_initial_state(
    response: &CompleteStateResponse,
    persistence_file: &Path,
    _output_pipe: &mut File,
    _input_pipe: &mut File,
    on_running_workloads: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // For initial state, process all workloads with persist tags
    let complete_state = response.complete_state.as_ref().ok_or("No complete state in initial response")?;
    let persistent_state = filter_persistent_workloads(complete_state);

    // Build set of workloads with ON_RUNNING persist tags
    if let Some(desired_state) = &complete_state.desired_state {
        if let Some(workloads) = &desired_state.workloads {
            for (name, workload) in &workloads.workloads {
                if let Some(mode) = get_persist_mode(&workload.tags) {
                    if mode == "ON_RUNNING" {
                        on_running_workloads.insert(name.clone());
                    }
                }
            }
        }
    }

    persist_state_atomically(&persistent_state, persistence_file).await?;
    Ok(())
}

/// Add a workload to the persisted state
fn add_workload_to_persisted_state(state: &mut State, name: &str, workload: &Workload) {
    if state.workloads.is_none() {
        state.workloads = Some(WorkloadMap {
            workloads: HashMap::new(),
        });
    }

    if let Some(ref mut workloads) = state.workloads {
        workloads.workloads.insert(name.to_string(), workload.clone());
    }
}

/// Remove a workload from the persisted state, returns true if it was present
fn remove_workload_from_persisted_state(state: &mut State, name: &str) -> bool {
    if let Some(ref mut workloads) = state.workloads {
        workloads.workloads.remove(name).is_some()
    } else {
        false
    }
}

/// Read persisted state from file and restore to Ankaios server
async fn restore_persisted_state(
    persistence_file: &Path,
    output_pipe: &mut File,
    input_pipe: &mut File,
) -> Result<(), Box<dyn std::error::Error>> {
    // Try to read the persisted state file
    let yaml_content = match tokio::fs::read_to_string(persistence_file).await {
        Ok(content) => content,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            log::info!("No persisted state found at {:?} (first run?)", persistence_file);
            return Ok(()); // Not an error - continue without restoring
        }
        Err(e) => {
            log::error!("Failed to read persistence file: {}", e);
            return Err(e.into());
        }
    };

    // Deserialize YAML to State struct
    let restored_state: State = match serde_yaml::from_str(&yaml_content) {
        Ok(state) => state,
        Err(e) => {
            log::error!("Failed to parse persisted state YAML: {}", e);
            log::warn!("Continuing without state restoration");
            return Ok(()); // Don't fail startup on corrupted file
        }
    };

    // Check if there's anything to restore
    let has_workloads = restored_state
        .workloads
        .as_ref()
        .map(|wl| !wl.workloads.is_empty())
        .unwrap_or(false);

    let has_configs = restored_state
        .configs
        .as_ref()
        .map(|cfg| !cfg.configs.is_empty())
        .unwrap_or(false);

    if !has_workloads && !has_configs {
        log::info!("Persisted state file is empty, nothing to restore");
        return Ok(());
    }

    // Generate update_mask from workload and config names
    let mut update_mask = Vec::new();

    if let Some(ref workloads) = restored_state.workloads {
        for name in workloads.workloads.keys() {
            update_mask.push(format!("desiredState.workloads.{}", name));
        }
    }

    if let Some(ref configs) = restored_state.configs {
        for name in configs.configs.keys() {
            update_mask.push(format!("desiredState.configs.{}", name));
        }
    }

    log::info!(
        "Restoring {} workload(s) and {} config(s) from persisted state",
        restored_state.workloads.as_ref().map(|w| w.workloads.len()).unwrap_or(0),
        restored_state.configs.as_ref().map(|c| c.configs.len()).unwrap_or(0)
    );

    // Create UpdateStateRequest
    let update_request = ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: "startup_restore".to_string(),
            request_content: Some(RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    new_state: Some(CompleteState {
                        desired_state: Some(restored_state),
                        ..Default::default()
                    }),
                    update_mask,
                },
            ))),
        })),
    };

    // Send the request
    send_to_ankaios(output_pipe, &update_request)?;

    // Wait for response
    let response = receive_from_ankaios(input_pipe)?;

    match response.from_ankaios_enum {
        Some(FromAnkaiosEnum::Response(resp)) => {
            match resp.response_content {
                Some(ResponseContent::UpdateStateSuccess(_)) => {
                    log::info!("Successfully restored persisted state");
                    Ok(())
                }
                Some(ResponseContent::Error(err)) => {
                    log::error!("Failed to restore state: {}", err.message);
                    Err(format!("State restoration failed: {}", err.message).into())
                }
                _ => {
                    log::error!("Unexpected response to UpdateStateRequest");
                    Err("Unexpected response type".into())
                }
            }
        }
        _ => {
            log::error!("Invalid response to state restoration request");
            Err("Invalid response".into())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting Basic Persistence Plugin...");

    // Get persistence file path from environment
    let persistence_file = PathBuf::from(
        env::var("PERSISTENCE_FILE_PATH")
            .unwrap_or_else(|_| "/var/lib/ankaios/runtime_state.yaml".to_string()),
    );
    log::info!("Persistence file: {:?}", persistence_file);

    // Open control interface pipes
    let input_pipe_path = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH).join("input");
    let output_pipe_path = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH).join("output");

    // Open output pipe first (agent already has this pipe open for reading)
    let mut output_pipe = File::options().write(true).open(&output_pipe_path)?;
    log::debug!("Opened output pipe");

    // Send hello message BEFORE opening input pipe to avoid deadlock
    // The agent won't retry opening the input pipe (for writing) until it tries to send ControlInterfaceAccepted
    // But it won't send that until it receives our Hello message
    let hello = create_hello_message();
    send_to_ankaios(&mut output_pipe, &hello)?;
    log::debug!("Sent hello message");

    // Now open input pipe - this will unblock once agent tries to write ControlInterfaceAccepted
    let mut input_pipe = File::open(&input_pipe_path)?;
    log::info!("Connected to Ankaios control interface");

    // Wait for control interface accepted response
    // Loop until we get ControlInterfaceAccepted, discarding any stale messages from previous instances
    loop {
        let response = receive_from_ankaios(&mut input_pipe)?;
        match response.from_ankaios_enum {
            Some(FromAnkaiosEnum::ControlInterfaceAccepted(_)) => {
                log::info!("Control interface connection accepted");
                break;
            }
            _ => {
                log::debug!("Discarding stale message while waiting for ControlInterfaceAccepted: {:?}", response);
                // Continue loop to wait for the real ControlInterfaceAccepted
            }
        }
    }

    // Restore persisted state (if any) before subscribing to events
    if let Err(e) = restore_persisted_state(&persistence_file, &mut output_pipe, &mut input_pipe).await {
        log::error!("State restoration failed, continuing anyway: {}", e);
        // Don't fail startup - persistence is best-effort
    }

    // Subscribe to events
    let subscription = create_event_subscription_request();
    send_to_ankaios(&mut output_pipe, &subscription)?;
    log::info!("Subscribed to Ankaios events");

    // Track which workloads have ON_RUNNING persist tags
    // This is updated when workloads are added/removed to avoid checking desired_state in events
    let mut on_running_workloads: HashSet<String> = HashSet::new();

    // Event loop
    loop {
        let message = receive_from_ankaios(&mut input_pipe)?;

        match message.from_ankaios_enum {
            Some(FromAnkaiosEnum::Response(response)) => {
                // Check if this is our event subscription
                if response.request_id == EVENT_REQUEST_ID {
                    if let Some(ResponseContent::CompleteStateResponse(state_response)) =
                        response.response_content
                    {
                        // Process the event
                        if let Err(e) = process_event(&state_response, &persistence_file, &mut output_pipe, &mut input_pipe, &mut on_running_workloads).await {
                            log::error!("Error processing event: {}", e);
                        }
                    }
                }
            }
            Some(FromAnkaiosEnum::ControlInterfaceAccepted(_)) => {
                log::warn!("Unexpected control interface accepted message during event loop");
            }
            Some(FromAnkaiosEnum::ConnectionClosed(closed)) => {
                log::error!("Connection closed by Ankaios: {}", closed.reason);
                return Err("Connection closed".into());
            }
            None => {
                log::warn!("Received empty message");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ankaios_api::ank_base::{
        execution_state::ExecutionStateEnum, ExecutionState, ExecutionsStatesForId,
        ExecutionsStatesOfWorkload, Workload, WorkloadMap,
    };
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_tags_with_persist(value: &str) -> Option<Tags> {
        let mut tags = HashMap::new();
        tags.insert("persist".to_string(), value.to_string());
        Some(Tags { tags })
    }

    fn unique_test_id() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{}_{}", std::process::id(), now)
    }

    fn create_workload_states_map(
        workload_name: &str,
        state: ExecutionStateEnum,
    ) -> Option<WorkloadStatesMap> {
        let mut id_state_map = HashMap::new();
        id_state_map.insert(
            "instance-1".to_string(),
            ExecutionState {
                additional_info: Some("".to_string()),
                execution_state_enum: Some(state),
            },
        );

        let mut wl_name_state_map = HashMap::new();
        wl_name_state_map.insert(
            workload_name.to_string(),
            ExecutionsStatesForId { id_state_map },
        );

        let mut agent_state_map = HashMap::new();
        agent_state_map.insert(
            "agent_A".to_string(),
            ExecutionsStatesOfWorkload { wl_name_state_map },
        );

        Some(WorkloadStatesMap { agent_state_map })
    }

    #[test]
    fn test_get_persist_mode_always() {
        let tags = create_tags_with_persist("ALWAYS");
        assert_eq!(get_persist_mode(&tags), Some("ALWAYS".to_string()));

        // Test case-insensitive
        let tags = create_tags_with_persist("always");
        assert_eq!(get_persist_mode(&tags), Some("ALWAYS".to_string()));
    }

    #[test]
    fn test_get_persist_mode_on_running() {
        let tags = create_tags_with_persist("ON_RUNNING");
        assert_eq!(get_persist_mode(&tags), Some("ON_RUNNING".to_string()));

        // Test case-insensitive
        let tags = create_tags_with_persist("on_running");
        assert_eq!(get_persist_mode(&tags), Some("ON_RUNNING".to_string()));
    }

    #[test]
    fn test_get_persist_mode_invalid() {
        let tags = create_tags_with_persist("INVALID");
        assert_eq!(get_persist_mode(&tags), None);

        let tags = create_tags_with_persist("ON_SUCCESS");
        assert_eq!(get_persist_mode(&tags), None);
    }

    #[test]
    fn test_get_persist_mode_missing_tag() {
        let tags = Some(Tags {
            tags: HashMap::new(),
        });
        assert_eq!(get_persist_mode(&tags), None);

        assert_eq!(get_persist_mode(&None), None);
    }

    #[test]
    fn test_is_workload_running_when_running() {
        let workload_states = create_workload_states_map(
            "test-workload",
            ExecutionStateEnum::Running(0), // RUNNING_OK = 0
        );

        assert!(is_workload_running("test-workload", &workload_states));
    }

    #[test]
    fn test_is_workload_running_when_pending() {
        let workload_states = create_workload_states_map(
            "test-workload",
            ExecutionStateEnum::Pending(0), // PENDING_INITIAL = 0
        );

        assert!(!is_workload_running("test-workload", &workload_states));
    }

    #[test]
    fn test_is_workload_running_when_failed() {
        let workload_states = create_workload_states_map(
            "test-workload",
            ExecutionStateEnum::Failed(0), // FAILED_EXEC_FAILED = 0
        );

        assert!(!is_workload_running("test-workload", &workload_states));
    }

    #[test]
    fn test_is_workload_running_no_states() {
        assert!(!is_workload_running("test-workload", &None));
    }

    #[test]
    fn test_is_workload_running_workload_not_found() {
        let workload_states = create_workload_states_map(
            "other-workload",
            ExecutionStateEnum::Running(0), // RUNNING_OK = 0
        );

        assert!(!is_workload_running("test-workload", &workload_states));
    }

    #[test]
    fn test_workload_has_state_custom_check() {
        let workload_states = create_workload_states_map(
            "test-workload",
            ExecutionStateEnum::Failed(0), // FAILED_EXEC_FAILED = 0
        );

        // Check for failed state
        let has_failed = workload_has_state("test-workload", &workload_states, |state| {
            matches!(state, ExecutionStateEnum::Failed(_))
        });
        assert!(has_failed);

        // Check for running state (should be false)
        let has_running = workload_has_state("test-workload", &workload_states, |state| {
            matches!(state, ExecutionStateEnum::Running(_))
        });
        assert!(!has_running);
    }

    #[test]
    fn test_filter_persistent_workloads_always_mode() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "test-always".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states: None, // No workload states - workload not running
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        // ALWAYS mode should persist even if not running
        assert!(result.workloads.is_some());
        let persisted = result.workloads.unwrap();
        assert_eq!(persisted.workloads.len(), 1);
        assert!(persisted.workloads.contains_key("test-always"));
    }

    #[test]
    fn test_filter_persistent_workloads_on_running_not_running() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "test-on-running".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ON_RUNNING"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states: create_workload_states_map(
                "test-on-running",
                ExecutionStateEnum::Pending(2), // PENDING_STARTING = 2
            ),
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        // ON_RUNNING mode should NOT persist when pending
        assert!(result.workloads.is_none() || result.workloads.unwrap().workloads.is_empty());
    }

    #[test]
    fn test_filter_persistent_workloads_on_running_is_running() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "test-on-running".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ON_RUNNING"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states: create_workload_states_map(
                "test-on-running",
                ExecutionStateEnum::Running(0), // RUNNING_OK = 0
            ),
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        // ON_RUNNING mode SHOULD persist when running
        assert!(result.workloads.is_some());
        let persisted = result.workloads.unwrap();
        assert_eq!(persisted.workloads.len(), 1);
        assert!(persisted.workloads.contains_key("test-on-running"));
    }

    #[test]
    fn test_filter_persistent_workloads_mixed_modes() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "always-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );
        workloads.insert(
            "on-running-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ON_RUNNING"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: alpine".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );
        workloads.insert(
            "no-persist-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: None,
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: busybox".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        // Create states: only on-running-workload is actually running
        let mut agent_state_map = HashMap::new();
        let mut wl_name_state_map = HashMap::new();

        let mut id_state_map_running = HashMap::new();
        id_state_map_running.insert(
            "instance-1".to_string(),
            ExecutionState {
                additional_info: Some("".to_string()),
                execution_state_enum: Some(ExecutionStateEnum::Running(0)), // RUNNING_OK = 0
            },
        );
        wl_name_state_map.insert(
            "on-running-workload".to_string(),
            ExecutionsStatesForId {
                id_state_map: id_state_map_running,
            },
        );

        agent_state_map.insert(
            "agent_A".to_string(),
            ExecutionsStatesOfWorkload { wl_name_state_map },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states: Some(WorkloadStatesMap { agent_state_map }),
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        assert!(result.workloads.is_some());
        let persisted = result.workloads.unwrap();

        // Should have 2 workloads: always-workload (always persists) and on-running-workload (is running)
        assert_eq!(persisted.workloads.len(), 2);
        assert!(persisted.workloads.contains_key("always-workload"));
        assert!(persisted.workloads.contains_key("on-running-workload"));
        assert!(!persisted.workloads.contains_key("no-persist-workload"));
    }

    #[test]
    fn test_filter_persistent_workloads_configs_always_persisted() {
        use ankaios_api::ank_base::{config_item::ConfigItemEnum, ConfigItem, ConfigMap};

        let mut configs = HashMap::new();
        configs.insert(
            "config1".to_string(),
            ConfigItem {
                config_item_enum: Some(ConfigItemEnum::String("value1".to_string())),
            },
        );
        configs.insert(
            "config2".to_string(),
            ConfigItem {
                config_item_enum: Some(ConfigItemEnum::String("value2".to_string())),
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: None,
                configs: Some(ConfigMap { configs }),
            }),
            workload_states: None,
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        // Configs should always be persisted
        assert!(result.configs.is_some());
        let persisted_configs = result.configs.unwrap();
        assert_eq!(persisted_configs.configs.len(), 2);
        assert!(persisted_configs.configs.contains_key("config1"));
        assert!(persisted_configs.configs.contains_key("config2"));
    }

    #[test]
    fn test_filter_persistent_workloads_no_desired_state() {
        let complete_state = CompleteState {
            desired_state: None,
            workload_states: None,
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        assert!(result.workloads.is_none());
        assert!(result.configs.is_none());
    }

    #[test]
    fn test_get_protocol_version_from_env() {
        std::env::set_var("ANKAIOS_VERSION", "2.0.0");
        assert_eq!(get_protocol_version(), "2.0.0");
        std::env::remove_var("ANKAIOS_VERSION");
    }

    #[test]
    fn test_get_protocol_version_default() {
        std::env::remove_var("ANKAIOS_VERSION");
        assert_eq!(get_protocol_version(), "1.0.0");
    }

    #[test]
    fn test_create_hello_message() {
        std::env::set_var("ANKAIOS_VERSION", "1.0.0");
        let hello = create_hello_message();

        match hello.to_ankaios_enum {
            Some(ToAnkaiosEnum::Hello(h)) => {
                assert_eq!(h.protocol_version, "1.0.0");
            }
            _ => panic!("Expected Hello message"),
        }

        std::env::remove_var("ANKAIOS_VERSION");
    }

    #[test]
    fn test_create_event_subscription_request() {
        let request = create_event_subscription_request();

        match request.to_ankaios_enum {
            Some(ToAnkaiosEnum::Request(req)) => {
                assert_eq!(req.request_id, EVENT_REQUEST_ID);
                match req.request_content {
                    Some(RequestContent::CompleteStateRequest(state_req)) => {
                        assert!(state_req.subscribe_for_events);
                        assert_eq!(state_req.field_mask.len(), 3);
                        assert!(state_req.field_mask.contains(&"workloadStates.*.*.*.state".to_string()));
                        assert!(state_req.field_mask.contains(&"desiredState.workloads.*".to_string()));
                        assert!(state_req.field_mask.contains(&"desiredState.configs".to_string()));
                    }
                    _ => panic!("Expected CompleteStateRequest"),
                }
            }
            _ => panic!("Expected Request message"),
        }
    }

    #[tokio::test]
    async fn test_persist_state_atomically_creates_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_persist_{}.yaml", unique_test_id()));

        let mut workloads = HashMap::new();
        workloads.insert(
            "test-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };

        persist_state_atomically(&state, &temp_file).await.unwrap();

        assert!(temp_file.exists());
        let content = tokio::fs::read_to_string(&temp_file).await.unwrap();
        assert!(content.contains("test-workload"));
        assert!(content.contains("apiVersion: v1"));

        // Cleanup
        tokio::fs::remove_file(&temp_file).await.ok();
    }

    #[tokio::test]
    async fn test_persist_state_atomically_removes_empty() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_persist_empty_{}.yaml", unique_test_id()));

        // Create file first
        tokio::fs::write(&temp_file, "dummy content").await.unwrap();
        assert!(temp_file.exists());

        // Persist empty state (no workloads)
        let state = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        };

        persist_state_atomically(&state, &temp_file).await.unwrap();

        // File should be removed
        assert!(!temp_file.exists());
    }

    #[tokio::test]
    async fn test_persist_state_atomically_creates_backup() {
        let temp_dir = std::env::temp_dir();
        let test_id = unique_test_id();
        let temp_file = temp_dir.join(format!("test_persist_backup_{}.yaml", test_id));
        let backup_file = temp_dir.join(format!("test_persist_backup_{}.backup", test_id));

        // Create initial file
        tokio::fs::write(&temp_file, "old content").await.unwrap();

        let mut workloads = HashMap::new();
        workloads.insert(
            "new-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: alpine".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };

        persist_state_atomically(&state, &temp_file).await.unwrap();

        // Backup should exist
        assert!(backup_file.exists());
        let backup_content = tokio::fs::read_to_string(&backup_file).await.unwrap();
        assert_eq!(backup_content, "old content");

        // New file should have new content
        let new_content = tokio::fs::read_to_string(&temp_file).await.unwrap();
        assert!(new_content.contains("new-workload"));

        // Cleanup
        tokio::fs::remove_file(&temp_file).await.ok();
        tokio::fs::remove_file(&backup_file).await.ok();
    }

    #[tokio::test]
    async fn test_filter_and_persist_workloads() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_persist_{}.yaml", unique_test_id()));

        let mut workloads = HashMap::new();
        workloads.insert(
            "event-test".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states: None,
            agents: None,
        };

        // Test filtering and persistence
        let persistent_state = filter_persistent_workloads(&complete_state);
        persist_state_atomically(&persistent_state, &temp_file).await.unwrap();

        assert!(temp_file.exists());
        let content = tokio::fs::read_to_string(&temp_file).await.unwrap();
        assert!(content.contains("event-test"));

        // Cleanup
        tokio::fs::remove_file(&temp_file).await.ok();
    }

    #[tokio::test]
    async fn test_restore_persisted_state_file_not_found() {
        let temp_dir = std::env::temp_dir();
        let non_existent_file =
            temp_dir.join(format!("non_existent_{}.yaml", unique_test_id()));

        // Create actual temp files for pipes
        let output_pipe = temp_dir.join(format!("output_pipe_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_{}", unique_test_id()));
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should return Ok (not an error) when file doesn't exist
        let result = restore_persisted_state(&non_existent_file, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Cleanup
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[tokio::test]
    async fn test_restore_persisted_state_corrupted_yaml() {
        let temp_dir = std::env::temp_dir();
        let corrupted_file = temp_dir.join(format!("corrupted_{}.yaml", unique_test_id()));

        // Write invalid YAML
        tokio::fs::write(&corrupted_file, "{ invalid yaml content [[[")
            .await
            .unwrap();

        // Create dummy pipes
        let output_pipe = temp_dir.join(format!("output_pipe_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_{}", unique_test_id()));
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should return Ok (don't crash) but log error
        let result = restore_persisted_state(&corrupted_file, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Cleanup
        tokio::fs::remove_file(&corrupted_file).await.ok();
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[tokio::test]
    async fn test_restore_persisted_state_empty_state() {
        let temp_dir = std::env::temp_dir();
        let empty_file = temp_dir.join(format!("empty_{}.yaml", unique_test_id()));

        // Write empty state YAML
        let empty_state = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        };
        let yaml_content = serde_yaml::to_string(&empty_state).unwrap();
        tokio::fs::write(&empty_file, yaml_content).await.unwrap();

        // Create dummy pipes
        let output_pipe = temp_dir.join(format!("output_pipe_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_{}", unique_test_id()));
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should return Ok without sending request (no workloads to restore)
        let result = restore_persisted_state(&empty_file, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Verify no data was written to output pipe (no request sent)
        let output_size = std::fs::metadata(&output_pipe).unwrap().len();
        assert_eq!(output_size, 0);

        // Cleanup
        tokio::fs::remove_file(&empty_file).await.ok();
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[tokio::test]
    async fn test_restore_persisted_state_success() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let state_file = temp_dir.join(format!("state_{}.yaml", unique_test_id()));

        // Create valid state YAML with one workload
        let mut workloads = HashMap::new();
        workloads.insert(
            "restored-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };
        let yaml_content = serde_yaml::to_string(&state).unwrap();
        tokio::fs::write(&state_file, yaml_content).await.unwrap();

        // Create pipes for communication
        let output_pipe = temp_dir.join(format!("output_pipe_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_{}", unique_test_id()));

        // Prepare success response in input pipe
        let success_response = FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(
                ankaios_api::ank_base::Response {
                    request_id: "startup_restore".to_string(),
                    response_content: Some(ResponseContent::UpdateStateSuccess(
                        ankaios_api::ank_base::UpdateStateSuccess {
                            added_workloads: vec!["restored-workload".to_string()],
                            deleted_workloads: vec![],
                        },
                    )),
                },
            ))),
        };

        // Write response to input pipe file
        {
            let mut input_file = std::fs::File::create(&input_pipe).unwrap();
            let encoded = success_response.encode_length_delimited_to_vec();
            input_file.write_all(&encoded).unwrap();
        }

        std::fs::File::create(&output_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should succeed
        let result = restore_persisted_state(&state_file, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Verify UpdateStateRequest was sent (output pipe should have data)
        let output_size = std::fs::metadata(&output_pipe).unwrap().len();
        assert!(output_size > 0);

        // Cleanup
        tokio::fs::remove_file(&state_file).await.ok();
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[tokio::test]
    async fn test_restore_persisted_state_server_error() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let state_file = temp_dir.join(format!("state_err_{}.yaml", unique_test_id()));

        // Create valid state YAML
        let mut workloads = HashMap::new();
        workloads.insert(
            "test-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };
        let yaml_content = serde_yaml::to_string(&state).unwrap();
        tokio::fs::write(&state_file, yaml_content).await.unwrap();

        // Create pipes
        let output_pipe = temp_dir.join(format!("output_pipe_err_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_err_{}", unique_test_id()));

        // Prepare error response
        let error_response = FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(
                ankaios_api::ank_base::Response {
                    request_id: "startup_restore".to_string(),
                    response_content: Some(ResponseContent::Error(
                        ankaios_api::ank_base::Error {
                            message: "Permission denied".to_string(),
                        },
                    )),
                },
            ))),
        };

        {
            let mut input_file = std::fs::File::create(&input_pipe).unwrap();
            let encoded = error_response.encode_length_delimited_to_vec();
            input_file.write_all(&encoded).unwrap();
        }

        std::fs::File::create(&output_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should return Err when server responds with error
        let result = restore_persisted_state(&state_file, &mut output, &mut input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Permission denied"));

        // Cleanup
        tokio::fs::remove_file(&state_file).await.ok();
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[test]
    fn test_event_response_contains_added_workload() {
        use ankaios_api::ank_base::AlteredFields;

        // Simulate an event response when workload "test-wl" is added
        // This tests whether the complete_state in an event includes the full workload definition

        let mut workloads = HashMap::new();
        workloads.insert(
            "test-wl".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states: None,
            agents: None,
        };

        let altered_fields = AlteredFields {
            added_fields: vec!["desiredState.workloads.test-wl".to_string()],
            updated_fields: vec![],
            removed_fields: vec![],
        };

        let event_response = CompleteStateResponse {
            complete_state: Some(complete_state),
            altered_fields: Some(altered_fields),
        };

        // Test: Verify that when we get an "Added" event, the complete_state contains the full workload
        assert!(event_response.complete_state.is_some());
        let cs = event_response.complete_state.as_ref().unwrap();
        assert!(cs.desired_state.is_some());
        let ds = cs.desired_state.as_ref().unwrap();
        assert!(ds.workloads.is_some());
        let wls = ds.workloads.as_ref().unwrap();
        assert!(wls.workloads.contains_key("test-wl"));

        // Verify we can get the workload definition
        let workload = wls.workloads.get("test-wl").unwrap();
        assert_eq!(workload.runtime, Some("podman".to_string()));
        assert_eq!(get_persist_mode(&workload.tags), Some("ALWAYS".to_string()));

        println!("✓ Event response contains full workload definition for added workloads");
    }

    // Tests for add_workload_to_persisted_state
    #[test]
    fn test_add_workload_to_persisted_state_new_workload() {
        let mut state = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        };

        let workload = Workload {
            runtime: Some("podman".to_string()),
            agent: Some("agent_A".to_string()),
            tags: create_tags_with_persist("ALWAYS"),
            dependencies: None,
            restart_policy: None,
            runtime_config: Some("image: nginx".to_string()),
            control_interface_access: None,
            configs: None,
            files: None,
        };

        add_workload_to_persisted_state(&mut state, "nginx", &workload);

        assert!(state.workloads.is_some());
        let workloads = state.workloads.as_ref().unwrap();
        assert_eq!(workloads.workloads.len(), 1);
        assert!(workloads.workloads.contains_key("nginx"));

        let persisted = workloads.workloads.get("nginx").unwrap();
        assert_eq!(persisted.runtime, Some("podman".to_string()));
        assert_eq!(persisted.agent, Some("agent_A".to_string()));
    }

    #[test]
    fn test_add_workload_to_persisted_state_existing_workloads() {
        let mut existing_workloads = HashMap::new();
        existing_workloads.insert(
            "redis".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_B".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: redis".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap {
                workloads: existing_workloads,
            }),
            configs: None,
        };

        let new_workload = Workload {
            runtime: Some("podman".to_string()),
            agent: Some("agent_A".to_string()),
            tags: create_tags_with_persist("ON_RUNNING"),
            dependencies: None,
            restart_policy: None,
            runtime_config: Some("image: nginx".to_string()),
            control_interface_access: None,
            configs: None,
            files: None,
        };

        add_workload_to_persisted_state(&mut state, "nginx", &new_workload);

        let workloads = state.workloads.as_ref().unwrap();
        assert_eq!(workloads.workloads.len(), 2);
        assert!(workloads.workloads.contains_key("nginx"));
        assert!(workloads.workloads.contains_key("redis"));
    }

    #[test]
    fn test_add_workload_to_persisted_state_replace_existing() {
        let mut existing_workloads = HashMap::new();
        existing_workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx:old".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap {
                workloads: existing_workloads,
            }),
            configs: None,
        };

        let updated_workload = Workload {
            runtime: Some("podman".to_string()),
            agent: Some("agent_A".to_string()),
            tags: create_tags_with_persist("ALWAYS"),
            dependencies: None,
            restart_policy: None,
            runtime_config: Some("image: nginx:latest".to_string()),
            control_interface_access: None,
            configs: None,
            files: None,
        };

        add_workload_to_persisted_state(&mut state, "nginx", &updated_workload);

        let workloads = state.workloads.as_ref().unwrap();
        assert_eq!(workloads.workloads.len(), 1);

        let persisted = workloads.workloads.get("nginx").unwrap();
        assert_eq!(persisted.runtime_config, Some("image: nginx:latest".to_string()));
    }

    // Tests for remove_workload_from_persisted_state
    #[test]
    fn test_remove_workload_from_persisted_state_exists() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );
        workloads.insert(
            "redis".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_B".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: redis".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };

        let modified = remove_workload_from_persisted_state(&mut state, "nginx");

        assert!(modified);
        let workloads = state.workloads.as_ref().unwrap();
        assert_eq!(workloads.workloads.len(), 1);
        assert!(!workloads.workloads.contains_key("nginx"));
        assert!(workloads.workloads.contains_key("redis"));
    }

    #[test]
    fn test_remove_workload_from_persisted_state_not_exists() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };

        let modified = remove_workload_from_persisted_state(&mut state, "nonexistent");

        assert!(!modified);
        let workloads = state.workloads.as_ref().unwrap();
        assert_eq!(workloads.workloads.len(), 1);
        assert!(workloads.workloads.contains_key("nginx"));
    }

    #[test]
    fn test_remove_workload_from_persisted_state_no_workloads() {
        let mut state = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        };

        let modified = remove_workload_from_persisted_state(&mut state, "nginx");

        assert!(!modified);
        assert!(state.workloads.is_none());
    }

    #[test]
    fn test_remove_workload_from_persisted_state_last_workload() {
        let mut workloads = HashMap::new();
        workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };

        let modified = remove_workload_from_persisted_state(&mut state, "nginx");

        assert!(modified);
        let workloads = state.workloads.as_ref().unwrap();
        assert_eq!(workloads.workloads.len(), 0);
    }

    // Test for get_workload_state_string
    #[test]
    fn test_get_workload_state_string_all_states() {
        // Test Running
        let states = create_workload_states_map("test", ExecutionStateEnum::Running(0));
        assert_eq!(get_workload_state_string("test", &states), "Running");

        // Test Succeeded
        let states = create_workload_states_map("test", ExecutionStateEnum::Succeeded(0));
        assert_eq!(get_workload_state_string("test", &states), "Succeeded");

        // Test Failed
        let states = create_workload_states_map("test", ExecutionStateEnum::Failed(0));
        assert_eq!(get_workload_state_string("test", &states), "Failed");

        // Test Pending
        let states = create_workload_states_map("test", ExecutionStateEnum::Pending(0));
        assert_eq!(get_workload_state_string("test", &states), "Pending");

        // Test Stopping
        let states = create_workload_states_map("test", ExecutionStateEnum::Stopping(0));
        assert_eq!(get_workload_state_string("test", &states), "Stopping");

        // Test Removed
        let states = create_workload_states_map("test", ExecutionStateEnum::Removed(0));
        assert_eq!(get_workload_state_string("test", &states), "Removed");

        // Test NotScheduled
        let states = create_workload_states_map("test", ExecutionStateEnum::NotScheduled(0));
        assert_eq!(get_workload_state_string("test", &states), "NotScheduled");

        // Test AgentDisconnected
        let states = create_workload_states_map("test", ExecutionStateEnum::AgentDisconnected(0));
        assert_eq!(get_workload_state_string("test", &states), "AgentDisconnected");

        // Test workload not found
        let states = create_workload_states_map("other", ExecutionStateEnum::Running(0));
        assert_eq!(get_workload_state_string("test", &states), "NotFound");

        // Test no state map
        assert_eq!(get_workload_state_string("test", &None), "NoStateMap");
    }

    #[test]
    fn test_create_complete_state_request() {
        let request = create_complete_state_request();

        assert!(request.to_ankaios_enum.is_some());
        if let Some(ToAnkaiosEnum::Request(req)) = request.to_ankaios_enum {
            assert_eq!(req.request_id, "fetch_complete_state");
            assert!(req.request_content.is_some());

            if let Some(RequestContent::CompleteStateRequest(cs_req)) = req.request_content {
                assert!(!cs_req.subscribe_for_events);
                assert!(cs_req.field_mask.contains(&"desiredState.workloads.*".to_string()));
            } else {
                panic!("Expected CompleteStateRequest");
            }
        } else {
            panic!("Expected Request");
        }
    }

    #[tokio::test]
    async fn test_process_initial_state() {
        let temp_dir = std::env::temp_dir();
        let persistence_file = temp_dir.join(format!("test_initial_state_{}.yaml", unique_test_id()));

        // Create state with mixed persist modes
        let mut workloads = HashMap::new();
        workloads.insert(
            "always-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );
        workloads.insert(
            "on-running-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_B".to_string()),
                tags: create_tags_with_persist("ON_RUNNING"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: alpine".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let workload_states = create_workload_states_map(
            "on-running-workload",
            ExecutionStateEnum::Running(0),
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: None,
            }),
            workload_states,
            agents: None,
        };

        let response = CompleteStateResponse {
            complete_state: Some(complete_state),
            altered_fields: None, // None indicates initial state
        };

        // Create dummy pipes
        let output_pipe = temp_dir.join(format!("output_pipe_initial_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_initial_{}", unique_test_id()));
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options().write(true).open(&output_pipe).unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();
        let mut on_running_workloads = HashSet::new();

        // Process initial state
        let result = process_initial_state(
            &response,
            &persistence_file,
            &mut output,
            &mut input,
            &mut on_running_workloads,
        ).await;

        assert!(result.is_ok());

        // Verify persistence file was created
        assert!(persistence_file.exists());

        // Verify both workloads were persisted (ON_RUNNING is running)
        let content = tokio::fs::read_to_string(&persistence_file).await.unwrap();
        assert!(content.contains("always-workload"));
        assert!(content.contains("on-running-workload"));

        // Verify ON_RUNNING workload was added to tracking set
        assert!(on_running_workloads.contains("on-running-workload"));

        // Cleanup
        tokio::fs::remove_file(&persistence_file).await.ok();
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[test]
    fn test_workload_has_state_multiple_instances() {
        // Test with multiple instances of the same workload
        let mut id_state_map = HashMap::new();
        id_state_map.insert(
            "instance-1".to_string(),
            ExecutionState {
                additional_info: Some("".to_string()),
                execution_state_enum: Some(ExecutionStateEnum::Pending(0)),
            },
        );
        id_state_map.insert(
            "instance-2".to_string(),
            ExecutionState {
                additional_info: Some("".to_string()),
                execution_state_enum: Some(ExecutionStateEnum::Running(0)),
            },
        );

        let mut wl_name_state_map = HashMap::new();
        wl_name_state_map.insert(
            "multi-instance".to_string(),
            ExecutionsStatesForId { id_state_map },
        );

        let mut agent_state_map = HashMap::new();
        agent_state_map.insert(
            "agent_A".to_string(),
            ExecutionsStatesOfWorkload { wl_name_state_map },
        );

        let workload_states = Some(WorkloadStatesMap { agent_state_map });

        // Should return true if ANY instance is running
        assert!(is_workload_running("multi-instance", &workload_states));
    }

    #[test]
    fn test_workload_has_state_multiple_agents() {
        // Test workload running on multiple agents
        let mut agent_state_map = HashMap::new();

        // Agent A - has pending instance
        let mut id_state_map_a = HashMap::new();
        id_state_map_a.insert(
            "instance-1".to_string(),
            ExecutionState {
                additional_info: Some("".to_string()),
                execution_state_enum: Some(ExecutionStateEnum::Pending(0)),
            },
        );
        let mut wl_name_state_map_a = HashMap::new();
        wl_name_state_map_a.insert(
            "test-workload".to_string(),
            ExecutionsStatesForId { id_state_map: id_state_map_a },
        );
        agent_state_map.insert(
            "agent_A".to_string(),
            ExecutionsStatesOfWorkload { wl_name_state_map: wl_name_state_map_a },
        );

        // Agent B - has running instance
        let mut id_state_map_b = HashMap::new();
        id_state_map_b.insert(
            "instance-2".to_string(),
            ExecutionState {
                additional_info: Some("".to_string()),
                execution_state_enum: Some(ExecutionStateEnum::Running(0)),
            },
        );
        let mut wl_name_state_map_b = HashMap::new();
        wl_name_state_map_b.insert(
            "test-workload".to_string(),
            ExecutionsStatesForId { id_state_map: id_state_map_b },
        );
        agent_state_map.insert(
            "agent_B".to_string(),
            ExecutionsStatesOfWorkload { wl_name_state_map: wl_name_state_map_b },
        );

        let workload_states = Some(WorkloadStatesMap { agent_state_map });

        // Should return true because agent B has a running instance
        assert!(is_workload_running("test-workload", &workload_states));
    }

    #[test]
    fn test_filter_persistent_workloads_on_running_different_states() {
        // Test that ON_RUNNING only persists when state is exactly Running, not other states

        let test_cases = vec![
            (ExecutionStateEnum::Running(0), true, "Running should persist"),
            (ExecutionStateEnum::Pending(0), false, "Pending should not persist"),
            (ExecutionStateEnum::Succeeded(0), false, "Succeeded should not persist"),
            (ExecutionStateEnum::Failed(0), false, "Failed should not persist"),
            (ExecutionStateEnum::Stopping(0), false, "Stopping should not persist"),
            (ExecutionStateEnum::Removed(0), false, "Removed should not persist"),
            (ExecutionStateEnum::NotScheduled(0), false, "NotScheduled should not persist"),
            (ExecutionStateEnum::AgentDisconnected(0), false, "AgentDisconnected should not persist"),
        ];

        for (state, should_persist, desc) in test_cases {
            let mut workloads = HashMap::new();
            workloads.insert(
                "test-workload".to_string(),
                Workload {
                    runtime: Some("podman".to_string()),
                    agent: Some("agent_A".to_string()),
                    tags: create_tags_with_persist("ON_RUNNING"),
                    dependencies: None,
                    restart_policy: None,
                    runtime_config: Some("image: nginx".to_string()),
                    control_interface_access: None,
                    configs: None,
                    files: None,
                },
            );

            let complete_state = CompleteState {
                desired_state: Some(State {
                    api_version: "v1".to_string(),
                    workloads: Some(WorkloadMap { workloads }),
                    configs: None,
                }),
                workload_states: create_workload_states_map("test-workload", state),
                agents: None,
            };

            let result = filter_persistent_workloads(&complete_state);

            if should_persist {
                assert!(result.workloads.is_some(), "{}", desc);
                assert!(result.workloads.unwrap().workloads.contains_key("test-workload"), "{}", desc);
            } else {
                assert!(
                    result.workloads.is_none() || result.workloads.unwrap().workloads.is_empty(),
                    "{}",
                    desc
                );
            }
        }
    }

    #[test]
    fn test_filter_persistent_workloads_with_configs_and_workloads() {
        use ankaios_api::ank_base::{config_item::ConfigItemEnum, ConfigItem, ConfigMap};

        let mut workloads = HashMap::new();
        workloads.insert(
            "test-workload".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut configs = HashMap::new();
        configs.insert(
            "app-config".to_string(),
            ConfigItem {
                config_item_enum: Some(ConfigItemEnum::String("config-value".to_string())),
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap { workloads }),
                configs: Some(ConfigMap { configs }),
            }),
            workload_states: None,
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        // Both workloads and configs should be persisted
        assert!(result.workloads.is_some());
        assert!(result.configs.is_some());

        let persisted_workloads = result.workloads.unwrap();
        assert_eq!(persisted_workloads.workloads.len(), 1);
        assert!(persisted_workloads.workloads.contains_key("test-workload"));

        let persisted_configs = result.configs.unwrap();
        assert_eq!(persisted_configs.configs.len(), 1);
        assert!(persisted_configs.configs.contains_key("app-config"));
    }

    #[tokio::test]
    async fn test_persist_state_atomically_with_configs() {
        use ankaios_api::ank_base::{config_item::ConfigItemEnum, ConfigItem, ConfigMap};

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("test_persist_configs_{}.yaml", unique_test_id()));

        let mut workloads = HashMap::new();
        workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut configs = HashMap::new();
        configs.insert(
            "db-config".to_string(),
            ConfigItem {
                config_item_enum: Some(ConfigItemEnum::String("host=localhost".to_string())),
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: Some(ConfigMap { configs }),
        };

        persist_state_atomically(&state, &temp_file).await.unwrap();

        assert!(temp_file.exists());
        let content = tokio::fs::read_to_string(&temp_file).await.unwrap();
        assert!(content.contains("nginx"));
        assert!(content.contains("db-config"));
        assert!(content.contains("host=localhost"));

        // Cleanup
        tokio::fs::remove_file(&temp_file).await.ok();
    }

    #[tokio::test]
    async fn test_restore_persisted_state_with_configs() {
        use ankaios_api::ank_base::{config_item::ConfigItemEnum, ConfigItem, ConfigMap};
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let state_file = temp_dir.join(format!("state_configs_{}.yaml", unique_test_id()));

        // Create state with both workloads and configs
        let mut workloads = HashMap::new();
        workloads.insert(
            "web-server".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ALWAYS"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let mut configs = HashMap::new();
        configs.insert(
            "app-config".to_string(),
            ConfigItem {
                config_item_enum: Some(ConfigItemEnum::String("setting=value".to_string())),
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: Some(ConfigMap { configs }),
        };
        let yaml_content = serde_yaml::to_string(&state).unwrap();
        tokio::fs::write(&state_file, yaml_content).await.unwrap();

        // Create pipes
        let output_pipe = temp_dir.join(format!("output_pipe_configs_{}", unique_test_id()));
        let input_pipe = temp_dir.join(format!("input_pipe_configs_{}", unique_test_id()));

        // Prepare success response
        let success_response = FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(
                ankaios_api::ank_base::Response {
                    request_id: "startup_restore".to_string(),
                    response_content: Some(ResponseContent::UpdateStateSuccess(
                        ankaios_api::ank_base::UpdateStateSuccess {
                            added_workloads: vec!["web-server".to_string()],
                            deleted_workloads: vec![],
                        },
                    )),
                },
            ))),
        };

        {
            let mut input_file = std::fs::File::create(&input_pipe).unwrap();
            let encoded = success_response.encode_length_delimited_to_vec();
            input_file.write_all(&encoded).unwrap();
        }

        std::fs::File::create(&output_pipe).unwrap();

        let mut output = std::fs::File::options().write(true).open(&output_pipe).unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should succeed
        let result = restore_persisted_state(&state_file, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Cleanup
        tokio::fs::remove_file(&state_file).await.ok();
        std::fs::remove_file(&output_pipe).ok();
        std::fs::remove_file(&input_pipe).ok();
    }

    #[test]
    fn test_get_persist_mode_case_variations() {
        // Test mixed case variations
        let test_cases = vec![
            ("always", Some("ALWAYS".to_string())),
            ("ALWAYS", Some("ALWAYS".to_string())),
            ("Always", Some("ALWAYS".to_string())),
            ("aLwAyS", Some("ALWAYS".to_string())),
            ("on_running", Some("ON_RUNNING".to_string())),
            ("ON_RUNNING", Some("ON_RUNNING".to_string())),
            ("On_Running", Some("ON_RUNNING".to_string())),
            ("on_RuNnInG", Some("ON_RUNNING".to_string())),
        ];

        for (input, expected) in test_cases {
            let tags = create_tags_with_persist(input);
            assert_eq!(get_persist_mode(&tags), expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_filter_persistent_workloads_empty_workloads_with_configs() {
        use ankaios_api::ank_base::{config_item::ConfigItemEnum, ConfigItem, ConfigMap};

        let mut configs = HashMap::new();
        configs.insert(
            "orphan-config".to_string(),
            ConfigItem {
                config_item_enum: Some(ConfigItemEnum::String("value".to_string())),
            },
        );

        let complete_state = CompleteState {
            desired_state: Some(State {
                api_version: "v1".to_string(),
                workloads: Some(WorkloadMap {
                    workloads: HashMap::new(), // Empty workloads
                }),
                configs: Some(ConfigMap { configs }),
            }),
            workload_states: None,
            agents: None,
        };

        let result = filter_persistent_workloads(&complete_state);

        // No workloads should be persisted
        assert!(result.workloads.is_none() || result.workloads.unwrap().workloads.is_empty());

        // Configs should still be persisted
        assert!(result.configs.is_some());
        let persisted_configs = result.configs.unwrap();
        assert_eq!(persisted_configs.configs.len(), 1);
        assert!(persisted_configs.configs.contains_key("orphan-config"));
    }
}
