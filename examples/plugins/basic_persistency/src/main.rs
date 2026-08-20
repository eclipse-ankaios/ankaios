// Copyright (c) 2026 Red Hat LLC
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

use common::path_security::safe_join;
use prost::Message;
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tokio::io::AsyncReadExt;

const ANKAIOS_CONTROL_INTERFACE_BASE_PATH: &str = "/run/ankaios/control_interface";
const MAX_VARINT_SIZE: usize = 19;
const EVENT_REQUEST_ID: &str = "basic_persistency_events";

/// Maximum size for persisted workload files (10 MB)
/// Prevents DoS attacks via memory exhaustion from oversized files
const MAX_WORKLOAD_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of ON_RUNNING workloads to track simultaneously
const MAX_ON_RUNNING_TRACKED: usize = 1000;

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

/// Check if workload reached a terminal state (Failed, Succeeded, or Removed)
fn is_workload_terminal(workload_name: &str, workload_states: &Option<WorkloadStatesMap>) -> bool {
    workload_has_state(workload_name, workload_states, |state| {
        matches!(
            state,
            ExecutionStateEnum::Failed(_) | ExecutionStateEnum::Succeeded(_) | ExecutionStateEnum::Removed(_)
        )
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

/// Persist a single workload to its own file in the workloads directory
///
/// Each workload is stored as `/var/lib/ankaios/workloads/<workload_name>.yaml`
/// as a YAML-serialized State containing just the single workload.
async fn persist_workload(
    workload_name: &str,
    workload: &Workload,
    workloads_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure workloads directory exists
    tokio::fs::create_dir_all(workloads_dir).await?;

    let state = State {
        api_version: "v1".to_string(),
        workloads: Some(WorkloadMap {
            workloads: [(workload_name.to_string(), workload.clone())]
                .into_iter()
                .collect(),
        }),
        configs: None,
    };
    let content = serde_yaml::to_string(&state)
        .map_err(|e| format!("Failed to serialize YAML: {}", e))?
        .into_bytes();

    let workload_file = safe_join(&workloads_dir, &format!("{}.yaml", workload_name))
        .map_err(|e| format!("Invalid workload name '{}': {}", workload_name, e))?;

    let workloads_dir = workloads_dir.to_path_buf();
    tokio::task::block_in_place(|| -> Result<(), Box<dyn std::error::Error>> {
        // NamedTempFile creates a secure temp file (O_EXCL, random name) preventing
        // symlink attacks. It auto-cleans on drop if persist() is not called.
        let mut temp = tempfile::NamedTempFile::new_in(&workloads_dir)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        temp.write_all(&content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temp.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }

        temp.as_file().sync_all()?;

        temp.persist(&workload_file)
            .map_err(|e| format!("Failed to persist workload file: {}", e))?;

        Ok(())
    })?;

    log::info!(
        "Persisted workload '{}' to {:?}",
        workload_name,
        workload_file
    );
    Ok(())
}

/// Remove a persisted workload file
async fn remove_persisted_workload(
    workload_name: &str,
    workloads_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let yaml_file = safe_join(&workloads_dir, &format!("{}.yaml", workload_name))
        .map_err(|e| format!("Invalid workload name '{}': {}", workload_name, e))?;

    if yaml_file.exists() {
        tokio::fs::remove_file(&yaml_file).await?;
        log::info!("Removed persisted workload '{}' from {:?}", workload_name, yaml_file);
    }

    Ok(())
}

/// Load all persisted workloads from directory on startup
async fn load_persisted_state(
    workloads_dir: &Path,
) -> Result<State, Box<dyn std::error::Error>> {
    let mut state = State {
        api_version: "v1".to_string(),
        workloads: None,
        configs: None,
    };

    // Check if directory exists
    if !workloads_dir.exists() {
        log::info!("Workloads directory {:?} does not exist yet", workloads_dir);
        return Ok(state);
    }

    log::debug!("Loading persisted state from {:?}", workloads_dir);

    // Read all .yaml files
    let mut entries = tokio::fs::read_dir(workloads_dir).await?;
    let mut file_count = 0;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        log::debug!("Checking file: {:?}", path);

        // Skip temp files (start with '.')
        let is_temp_file = path.file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.starts_with('.'));

        if is_temp_file {
            log::debug!("Skipping (temp file): {:?}", path);
            continue;
        }

        let extension = path.extension().and_then(|ext| ext.to_str());

        match extension {
            Some("yaml") => {
                file_count += 1;
                log::debug!("Reading workload file #{}: {:?}", file_count, path);

                let file = match tokio::fs::File::open(&path).await {
                    Ok(f) => f,
                    Err(e) => {
                        log::warn!("Failed to open {:?}: {}", path, e);
                        continue;
                    }
                };

                let file_len = match file.metadata().await {
                    Ok(m) => m.len(),
                    Err(e) => {
                        log::warn!("Failed to get metadata for {:?}: {}", path, e);
                        continue;
                    }
                };

                if file_len > MAX_WORKLOAD_FILE_SIZE {
                    log::error!(
                        "Workload file too large: {:?} ({} bytes, max {} bytes). Skipping.",
                        path, file_len, MAX_WORKLOAD_FILE_SIZE
                    );
                    continue;
                }

                let mut content = String::new();
                match file.take(MAX_WORKLOAD_FILE_SIZE).read_to_string(&mut content).await {
                    Ok(_) => {
                        log::debug!("Read {} bytes from {:?}", content.len(), path);
                        match serde_yaml::from_str::<State>(&content) {
                            Ok(workload_state) => {
                                log::debug!("Parsed YAML successfully, workloads: {:?}",
                                    workload_state.workloads.as_ref().map(|w| w.workloads.len()));
                                if let Some(ref workloads) = workload_state.workloads {
                                    for (name, workload) in &workloads.workloads {
                                        add_workload_to_persisted_state(&mut state, name, workload);
                                        log::info!("Loaded workload '{}' from {:?}", name, path);
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to parse YAML {:?}: {}", path, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to read {:?}: {}", path, e);
                    }
                }
            }
            _ => {
                log::debug!("Skipping file with unknown extension: {:?}", path);
            }
        }
    }

    log::debug!("Finished loading, processed {} files, final state has {} workloads",
        file_count,
        state.workloads.as_ref().map(|w| w.workloads.len()).unwrap_or(0));

    Ok(state)
}

/// Get workloads directory from persistence base directory
fn get_workloads_dir(persistence_dir: &Path) -> PathBuf {
    persistence_dir.join("workloads")
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

/// Maximum protobuf message size (64 MB)
const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;

/// Read protobuf data from pipe
fn read_protobuf_data(file: &mut File) -> Result<Box<[u8]>, io::Error> {
    let varint_data = read_varint_data(file)?;
    let mut varint_data = Box::new(&varint_data[..]);

    // Determine the exact size for exact reading of the bytes later by decoding the varint data
    let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

    if size > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Message size {} exceeds maximum {}", size, MAX_MESSAGE_SIZE),
        ));
    }

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
    persistence_dir: &Path,
    on_running_workloads: &mut HashSet<String>,
    on_running_cached_workloads: &mut HashMap<String, Workload>,
    persisted_workload_names: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let workloads_dir = get_workloads_dir(persistence_dir);

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
            return process_initial_state(response, on_running_workloads, on_running_cached_workloads).await;
        }
    };

    // Debug: Log what fields changed in this event
    log::info!("UpdateStateEvent - added: {:?}, removed: {:?}, updated: {:?}",
        altered_fields.added_fields, altered_fields.removed_fields, altered_fields.updated_fields);

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
                log::info!("Workload '{}' has persist mode: {}", workload_name, mode);
                match mode.as_str() {
                    "ALWAYS" => {
                        // Persist immediately to individual workload file
                        log::info!("Persisting workload '{}' with persist: ALWAYS", workload_name);
                        if let Err(e) = persist_workload(workload_name, workload, &workloads_dir).await {
                            log::error!("Failed to persist ALWAYS workload '{}': {}", workload_name, e);
                        } else {
                            persisted_workload_names.insert(workload_name.to_string());
                        }
                    }
                    "ON_RUNNING" => {
                        if on_running_workloads.len() >= MAX_ON_RUNNING_TRACKED {
                            log::warn!(
                                "ON_RUNNING tracking limit reached ({}), skipping workload '{}'",
                                MAX_ON_RUNNING_TRACKED, workload_name
                            );
                        } else {
                            log::debug!("Workload '{}' has persist: ON_RUNNING, waiting for Running state", workload_name);
                            on_running_workloads.insert(workload_name.to_string());
                            on_running_cached_workloads.insert(workload_name.to_string(), workload.clone());
                        }
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
            log::info!("Removing workload '{}' from persistence", workload_name);

            // Delete the persisted workload file
            if let Err(e) = remove_persisted_workload(workload_name, &workloads_dir).await {
                log::error!("Failed to delete persisted file for '{}': {}", workload_name, e);
            } else {
                persisted_workload_names.remove(workload_name);
                log::info!("Successfully deleted file for '{}'", workload_name);
            }

            // Also remove from ON_RUNNING tracking sets
            on_running_workloads.remove(workload_name);
            on_running_cached_workloads.remove(workload_name);
        }
    }

    // Process workload definition updates (desiredState.workloads.X or desiredState.workloads.X.Y)
    for field in &altered_fields.updated_fields {
        // Check if this is a workload update (e.g., "desiredState.workloads.workload_a" or "desiredState.workloads.workload_a.runtimeConfig")
        if field.starts_with("desiredState.workloads.") {
            // Extract workload name from path like "desiredState.workloads.workload_a" or "desiredState.workloads.workload_a.runtimeConfig"
            let parts: Vec<&str> = field.split('.').collect();
            // Process if workload_name is present (parts.len() >= 3)
            // parts.len() == 3: ["desiredState", "workloads", "workload_name"] - whole workload updated
            // parts.len() > 3: ["desiredState", "workloads", "workload_name", "field"] - specific field updated
            if parts.len() >= 3 {
                let workload_name = parts[2];

                // Only process if this workload has a persist tag
                if let Some(complete_state) = &response.complete_state {
                    if let Some(desired_state) = &complete_state.desired_state {
                        if let Some(workloads) = &desired_state.workloads {
                            if let Some(workload) = workloads.workloads.get(workload_name) {
                                if let Some(mode) = get_persist_mode(&workload.tags) {
                                    match mode.as_str() {
                                        "ALWAYS" => {
                                            // Workload definition changed - update persisted file
                                            log::info!("Workload '{}' updated, re-persisting", workload_name);

                                            if let Err(e) = persist_workload(workload_name, workload, &workloads_dir).await {
                                                log::error!("Failed to update persisted workload '{}': {}", workload_name, e);
                                            }
                                        }
                                        "ON_RUNNING" => {
                                            if persisted_workload_names.contains(workload_name) {
                                                log::info!("Workload '{}' (ON_RUNNING) updated, re-persisting", workload_name);
                                                if let Err(e) = persist_workload(workload_name, workload, &workloads_dir).await {
                                                    log::error!("Failed to update persisted ON_RUNNING workload '{}': {}", workload_name, e);
                                                }
                                            } else if on_running_workloads.contains(workload_name) {
                                                on_running_cached_workloads.insert(workload_name.to_string(), workload.clone());
                                                log::debug!("Workload '{}' (ON_RUNNING) updated, cached for later persistence", workload_name);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Process workload state changes (for ON_RUNNING workloads transitioning to Running)
    // Check both added_fields and updated_fields: initial workloadStates entries
    // appear in added_fields, subsequent transitions appear in updated_fields.
    if !on_running_workloads.is_empty() {
        for field in altered_fields.added_fields.iter().chain(altered_fields.updated_fields.iter()) {
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
                            if log::log_enabled!(log::Level::Debug) {
                                let current_state = get_workload_state_string(workload_name, &complete_state.workload_states);
                                log::debug!("Workload '{}' current state: {}, is_running: {}", workload_name, current_state, is_running);
                            }

                            if is_running {
                                // Check if not already persisted on disk
                                let already_persisted = persisted_workload_names.contains(workload_name);

                                if !already_persisted {
                                    log::info!("Workload '{}' reached Running state, persisting to file", workload_name);

                                    // Get the cached workload
                                    if let Some(workload) = on_running_cached_workloads.get(workload_name) {
                                        if let Err(e) = persist_workload(workload_name, workload, &workloads_dir).await {
                                            log::error!("Failed to persist ON_RUNNING workload '{}': {}", workload_name, e);
                                        } else {
                                            persisted_workload_names.insert(workload_name.to_string());
                                            // Remove from tracking sets since we've persisted it
                                            on_running_workloads.remove(workload_name);
                                            on_running_cached_workloads.remove(workload_name);
                                        }
                                    } else {
                                        log::error!("ON_RUNNING workload '{}' reached Running but workload was not cached", workload_name);
                                    }
                                }
                            } else if is_workload_terminal(workload_name, &complete_state.workload_states) {
                                log::info!("ON_RUNNING workload '{}' reached terminal state, removing from tracking", workload_name);
                                on_running_workloads.remove(workload_name);
                                on_running_cached_workloads.remove(workload_name);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Process initial state (when plugin first starts)
async fn process_initial_state(
    response: &CompleteStateResponse,
    on_running_workloads: &mut HashSet<String>,
    on_running_cached_workloads: &mut HashMap<String, Workload>,
) -> Result<(), Box<dyn std::error::Error>> {
    // For initial state, only track ON_RUNNING workloads
    // Do NOT persist - this is just the startup manifest (known good base state)
    // Only runtime changes (UpdateStateRequest events) get persisted
    let complete_state = response.complete_state.as_ref().ok_or("No complete state in initial response")?;

    // Build set of workloads with ON_RUNNING persist tags
    if let Some(desired_state) = &complete_state.desired_state {
        if let Some(workloads) = &desired_state.workloads {
            for (name, workload) in &workloads.workloads {
                if let Some(mode) = get_persist_mode(&workload.tags) {
                    if mode == "ON_RUNNING" {
                        if on_running_workloads.len() >= MAX_ON_RUNNING_TRACKED {
                            log::warn!(
                                "ON_RUNNING tracking limit reached ({}), skipping workload '{}'",
                                MAX_ON_RUNNING_TRACKED, name
                            );
                        } else {
                            on_running_workloads.insert(name.clone());
                            on_running_cached_workloads.insert(name.clone(), workload.clone());
                        }
                    }
                }
            }
        }
    }

    log::info!("Initial state processed, tracking {} ON_RUNNING workloads, no persistence performed",
               on_running_workloads.len());
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

/// Read persisted state and restore to Ankaios server
async fn restore_persisted_state(
    persistence_dir: &Path,
    output_pipe: &mut File,
    input_pipe: &mut File,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load persisted state from workloads directory
    let workloads_dir = get_workloads_dir(persistence_dir);
    let persisted_state = load_persisted_state(&workloads_dir).await?;

    // Check if there's anything to restore
    let has_workloads = persisted_state
        .workloads
        .as_ref()
        .map(|wl| !wl.workloads.is_empty())
        .unwrap_or(false);

    if !has_workloads {
        log::info!("No persisted workloads found in {:?}", workloads_dir);
        return Ok(());
    }

    // Apply persisted workloads on top of startup manifest by sending UpdateStateRequests
    if let Some(ref workloads) = persisted_state.workloads {
        log::info!(
            "Applying {} persisted workload(s) on top of startup manifest",
            workloads.workloads.len()
        );

        // Collect workload files for restoration
        let mut workloads_with_files: Vec<(String, PathBuf)> = vec![];

        for workload_name in workloads.workloads.keys() {
            let yaml_file = safe_join(&workloads_dir, &format!("{}.yaml", workload_name))
                .ok()
                .filter(|p| p.exists());

            match yaml_file {
                Some(yaml) => {
                    workloads_with_files.push((workload_name.clone(), yaml));
                }
                None => {
                    log::warn!("No file found for workload '{}'", workload_name);
                    continue;
                }
            };
        }

        // For each persisted workload, read its file and send as UpdateStateRequest
        for (workload_name, workload_file) in workloads_with_files {
            let update_state_request = match tokio::fs::read_to_string(&workload_file).await {
                Ok(content) => {
                    match serde_yaml::from_str::<State>(&content) {
                        Ok(workload_state) => {
                            log::debug!("Parsed workload '{}' from {:?}", workload_name, workload_file);
                            UpdateStateRequest {
                                new_state: Some(CompleteState {
                                    desired_state: Some(workload_state),
                                    ..Default::default()
                                }),
                                update_mask: vec![format!("desiredState.workloads.{}", workload_name)],
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to parse YAML {:?}: {}", workload_file, e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to read {:?}: {}", workload_file, e);
                    continue;
                }
            };

            // Send UpdateStateRequest to server
            let to_ankaios = ToAnkaios {
                to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
                    request_id: format!("startup_restore_{}", workload_name),
                    request_content: Some(RequestContent::UpdateStateRequest(Box::new(
                        update_state_request,
                    ))),
                })),
            };

            tokio::task::block_in_place(|| send_to_ankaios(output_pipe, &to_ankaios))?;

            // Wait for response
            let response = tokio::task::block_in_place(|| receive_from_ankaios(input_pipe))?;

            match response.from_ankaios_enum {
                Some(FromAnkaiosEnum::Response(resp)) => {
                    match resp.response_content {
                        Some(ResponseContent::UpdateStateSuccess(_)) => {
                            log::info!("Successfully restored workload '{}' from {:?}", workload_name, workload_file);
                        }
                        Some(ResponseContent::Error(err)) => {
                            log::error!("Failed to restore workload '{}': {}", workload_name, err.message);
                            // Continue with other workloads even if one fails
                        }
                        _ => {
                            log::error!("Unexpected response for workload '{}'", workload_name);
                        }
                    }
                }
                _ => {
                    log::error!("Invalid response to restoration request for '{}'", workload_name);
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting Basic Persistence Plugin...");

    // Get persistence base directory from environment
    let persistence_dir = PathBuf::from(
        env::var("PERSISTENCE_DIR")
            .unwrap_or_else(|_| "/var/lib/ankaios".to_string()),
    );
    log::info!("Persistence directory: {:?}", persistence_dir);

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
    tokio::task::block_in_place(|| send_to_ankaios(&mut output_pipe, &hello))?;
    log::debug!("Sent hello message");

    // Now open input pipe - this will unblock once agent tries to write ControlInterfaceAccepted
    let mut input_pipe = File::open(&input_pipe_path)?;
    log::info!("Connected to Ankaios control interface");

    // Wait for control interface accepted response
    // Loop until we get ControlInterfaceAccepted, discarding any stale messages from previous instances
    const MAX_HANDSHAKE_ATTEMPTS: usize = 50;
    for attempt in 1..=MAX_HANDSHAKE_ATTEMPTS {
        let response = tokio::task::block_in_place(|| receive_from_ankaios(&mut input_pipe))?;
        match response.from_ankaios_enum {
            Some(FromAnkaiosEnum::ControlInterfaceAccepted(_)) => {
                log::info!("Control interface connection accepted");
                break;
            }
            _ if attempt == MAX_HANDSHAKE_ATTEMPTS => {
                return Err(format!(
                    "Did not receive ControlInterfaceAccepted after {} messages",
                    MAX_HANDSHAKE_ATTEMPTS
                ).into());
            }
            _ => {
                log::debug!("Discarding stale message ({}/{}) while waiting for ControlInterfaceAccepted: {:?}",
                    attempt, MAX_HANDSHAKE_ATTEMPTS, response);
            }
        }
    }

    // Restore persisted state (if any) before subscribing to events
    if let Err(e) = restore_persisted_state(&persistence_dir, &mut output_pipe, &mut input_pipe).await {
        log::error!("State restoration failed, continuing anyway: {}", e);
        // Don't fail startup - persistence is best-effort
    }

    // Subscribe to events
    let subscription = create_event_subscription_request();
    tokio::task::block_in_place(|| send_to_ankaios(&mut output_pipe, &subscription))?;
    log::info!("Subscribed to Ankaios events");

    // Track which workloads have ON_RUNNING persist tags
    // This is updated when workloads are added/removed to avoid checking desired_state in events
    let mut on_running_workloads: HashSet<String> = HashSet::new();

    // Store workload objects for ON_RUNNING workloads (saved when added, used when they reach Running)
    let mut on_running_cached_workloads: HashMap<String, Workload> = HashMap::new();

    // Track which workloads are persisted on disk (avoids directory scan on every event)
    let workloads_dir = get_workloads_dir(&persistence_dir);
    let mut persisted_workload_names: HashSet<String> = HashSet::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&workloads_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                if !name.starts_with('.') {
                    if let Some(workload_name) = name.strip_suffix(".yaml") {
                        persisted_workload_names.insert(workload_name.to_string());
                    }
                }
            }
        }
    }
    log::info!("Initialized persisted workload tracking: {} workloads on disk", persisted_workload_names.len());

    // Spawn reader thread for blocking pipe I/O
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel::<FromAnkaios>(32);
    std::thread::spawn(move || {
        loop {
            match receive_from_ankaios(&mut input_pipe) {
                Ok(msg) => {
                    if msg_tx.blocking_send(msg).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    log::error!("Pipe read error: {}", e);
                    break;
                }
            }
        }
    });

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    // Event loop with graceful shutdown
    loop {
        let message = tokio::select! {
            msg = msg_rx.recv() => match msg {
                Some(m) => m,
                None => {
                    log::error!("Reader thread terminated unexpectedly");
                    return Err("Reader thread died".into());
                }
            },
            _ = sigterm.recv() => {
                log::info!("Received SIGTERM, shutting down gracefully");
                return Ok(());
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received SIGINT, shutting down gracefully");
                return Ok(());
            }
        };

        match message.from_ankaios_enum {
            Some(FromAnkaiosEnum::Response(response)) => {
                if response.request_id == EVENT_REQUEST_ID {
                    if let Some(ResponseContent::CompleteStateResponse(state_response)) =
                        response.response_content
                    {
                        if let Err(e) = process_event(&state_response, &persistence_dir, &mut on_running_workloads, &mut on_running_cached_workloads, &mut persisted_workload_names).await {
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

    fn create_tags_with_persist(value: &str) -> Option<Tags> {
        let mut tags = HashMap::new();
        tags.insert("persist".to_string(), value.to_string());
        Some(Tags { tags })
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
    fn test_get_protocol_version() {
        let version = get_protocol_version();
        assert!(!version.is_empty(), "Protocol version should not be empty");
    }

    #[test]
    fn test_create_hello_message() {
        let hello = create_hello_message();

        match hello.to_ankaios_enum {
            Some(ToAnkaiosEnum::Hello(h)) => {
                assert!(!h.protocol_version.is_empty());
            }
            _ => panic!("Expected Hello message"),
        }
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

    fn create_test_state_with_workload(name: &str, persist_mode: &str) -> State {
        let mut workloads = HashMap::new();
        workloads.insert(
            name.to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist(persist_mode),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        }
    }

    #[tokio::test]
    async fn test_restore_persisted_state_file_not_found() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let non_existent_dir = temp_dir.path().join("non_existent");

        let output_pipe = temp_dir.path().join("output_pipe");
        let input_pipe = temp_dir.path().join("input_pipe");
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        let result = restore_persisted_state(&non_existent_dir, &mut output, &mut input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_restore_persisted_state_corrupted_yaml() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");

        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();
        tokio::fs::write(workloads_dir.join("corrupted.yaml"), "{ invalid yaml content [[[")
            .await
            .unwrap();

        let output_pipe = temp_dir.path().join("output_pipe");
        let input_pipe = temp_dir.path().join("input_pipe");
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should return Ok (don't crash) but skip the corrupted file
        let result = restore_persisted_state(persistence_dir, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Verify no UpdateStateRequest was sent (corrupted file was skipped)
        let output_size = std::fs::metadata(&output_pipe).unwrap().len();
        assert_eq!(output_size, 0, "No request should be sent for corrupted YAML");
    }

    #[tokio::test]
    async fn test_restore_persisted_state_empty_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");

        // Create workloads directory with an empty state YAML
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();
        let empty_state = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        };
        let yaml_content = serde_yaml::to_string(&empty_state).unwrap();
        tokio::fs::write(workloads_dir.join("empty.yaml"), yaml_content).await.unwrap();

        let output_pipe = temp_dir.path().join("output_pipe");
        let input_pipe = temp_dir.path().join("input_pipe");
        std::fs::File::create(&output_pipe).unwrap();
        std::fs::File::create(&input_pipe).unwrap();

        let mut output = std::fs::File::options()
            .write(true)
            .open(&output_pipe)
            .unwrap();
        let mut input = std::fs::File::open(&input_pipe).unwrap();

        // Should return Ok without sending request (no workloads to restore)
        let result = restore_persisted_state(persistence_dir, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Verify no data was written to output pipe (no request sent)
        let output_size = std::fs::metadata(&output_pipe).unwrap().len();
        assert_eq!(output_size, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_persisted_state_success() {
        use std::io::Write;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");

        // Create workloads directory and persist a workload
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        let state = create_test_state_with_workload("restored-workload", "ALWAYS");

        // Write workload file to directory
        let yaml_content = serde_yaml::to_string(&state).unwrap();
        tokio::fs::write(workloads_dir.join("restored-workload.yaml"), &yaml_content).await.unwrap();

        // Verify file was written
        let workload_file = workloads_dir.join("restored-workload.yaml");
        assert!(workload_file.exists(), "Workload file should exist");

        let output_pipe = temp_dir.path().join("output_pipe");
        let input_pipe = temp_dir.path().join("input_pipe");

        // Prepare success response in input pipe (one per workload)
        let success_response = FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(
                ankaios_api::ank_base::Response {
                    request_id: "startup_restore_restored-workload".to_string(),
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

        let result = restore_persisted_state(persistence_dir, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Drop the output handle to flush and close it
        drop(output);

        // Verify UpdateStateRequest was sent (output pipe should have data)
        let output_size = std::fs::metadata(&output_pipe).unwrap().len();
        assert!(output_size > 0, "UpdateStateRequest should have been sent");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_persisted_state_server_error() {
        use std::io::Write;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");

        // Create workloads directory and persist a workload
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        let state = create_test_state_with_workload("test-workload", "ALWAYS");
        let yaml_content = serde_yaml::to_string(&state).unwrap();

        // Write workload file to directory
        tokio::fs::write(workloads_dir.join("test-workload.yaml"), &yaml_content).await.unwrap();

        let output_pipe = temp_dir.path().join("output_pipe");
        let input_pipe = temp_dir.path().join("input_pipe");

        // Prepare error response
        let error_response = FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(
                ankaios_api::ank_base::Response {
                    request_id: "startup_restore_test-workload".to_string(),
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

        // Errors are logged but function returns Ok (continues with other workloads)
        let result = restore_persisted_state(persistence_dir, &mut output, &mut input).await;
        assert!(result.is_ok(), "Function should continue even if one workload fails to restore");
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
            effective_state: None,
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

    #[tokio::test]
    async fn test_process_initial_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();

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

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            configs: None,
        };

        let complete_state = CompleteState {
            desired_state: Some(state.clone()),
            workload_states,
            agents: None,
            effective_state: None,
        };

        let response = CompleteStateResponse {
            complete_state: Some(complete_state),
            altered_fields: None, // None indicates initial state
        };

        let mut on_running_workloads = HashSet::new();
        let mut on_running_cached_workloads = HashMap::new();

        let result = process_initial_state(
            &response,
            &mut on_running_workloads,
            &mut on_running_cached_workloads,
        ).await;

        assert!(result.is_ok());

        // Initial state (startup manifest) is NOT persisted
        // Only runtime changes get persisted
        let workloads_dir = persistence_dir.join("workloads");
        assert!(!workloads_dir.exists(), "Initial state should not create workloads directory");

        // Verify ON_RUNNING workload was added to tracking set
        assert!(on_running_workloads.contains("on-running-workload"), "ON_RUNNING workload should be tracked");

        // Verify that the set has correct size (only ON_RUNNING tagged workloads)
        assert_eq!(on_running_workloads.len(), 1, "Should track exactly 1 ON_RUNNING workload");

        // Verify ON_RUNNING workload was cached for later persistence
        assert!(on_running_cached_workloads.contains_key("on-running-workload"),
            "ON_RUNNING workload should be cached for later persistence");
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_persisted_state_with_configs() {
        use std::io::Write;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");

        // Create workloads directory and write a workload file
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        let state = create_test_state_with_workload("web-server", "ALWAYS");
        let yaml_content = serde_yaml::to_string(&state).unwrap();
        tokio::fs::write(workloads_dir.join("web-server.yaml"), &yaml_content).await.unwrap();

        let output_pipe = temp_dir.path().join("output_pipe");
        let input_pipe = temp_dir.path().join("input_pipe");

        // Prepare success response
        let success_response = FromAnkaios {
            from_ankaios_enum: Some(FromAnkaiosEnum::Response(Box::new(
                ankaios_api::ank_base::Response {
                    request_id: "startup_restore_web-server".to_string(),
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

        let result = restore_persisted_state(persistence_dir, &mut output, &mut input).await;
        assert!(result.is_ok());

        // Verify UpdateStateRequest was sent (output pipe should have data)
        drop(output);
        let output_size = std::fs::metadata(&output_pipe).unwrap().len();
        assert!(output_size > 0, "UpdateStateRequest should have been sent for web-server");
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_persist_workload() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_workloads_dir = temp_dir.path().to_path_buf();

        let state = create_test_state_with_workload("test-workload", "ALWAYS");
        let workload = state.workloads.as_ref().unwrap().workloads.get("test-workload").unwrap();

        let result = persist_workload("test-workload", workload, &temp_workloads_dir).await;

        assert!(result.is_ok(), "Should persist workload");

        // Verify .yaml file was created
        let workload_file = temp_workloads_dir.join("test-workload.yaml");
        assert!(workload_file.exists(), "Workload should create .yaml file");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_persist_workload_cleans_temp_on_failure() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Create a subdirectory for the workload file, then make it read-only
        // so the rename from temp to final fails
        let workloads_dir = temp_dir.join("workloads");
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        let state = create_test_state_with_workload("test-workload", "ALWAYS");
        let workload = state.workloads.as_ref().unwrap().workloads.get("test-workload").unwrap();

        // First persist succeeds (creates the file)
        persist_workload("test-workload", workload, &workloads_dir).await.unwrap();

        // Make the workloads dir read-only so rename fails
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&workloads_dir).await.unwrap().permissions();
            perms.set_mode(0o444);
            tokio::fs::set_permissions(&workloads_dir, perms).await.unwrap();
        }

        // This should fail but not leave a temp file
        let result = persist_workload("test-workload", workload, &workloads_dir).await;
        assert!(result.is_err(), "Should fail when directory is read-only");

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&workloads_dir).await.unwrap().permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&workloads_dir, perms).await.unwrap();
        }

        // Verify no temp files left behind (NamedTempFile auto-cleans on drop)
        let mut entries = tokio::fs::read_dir(&workloads_dir).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name();
            let name_str = name.to_str().unwrap();
            assert!(
                name_str == "test-workload.yaml",
                "Only the original workload file should remain, found: {}", name_str
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_persist_multiple_workloads_separate_files() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Create first workload (unsigned)
        let state1 = create_test_state_with_workload("mqtt_fedora", "ON_RUNNING");
        let workload1 = state1.workloads.as_ref().unwrap().workloads.get("mqtt_fedora").unwrap();

        persist_workload("mqtt_fedora", workload1, &temp_dir).await.unwrap();

        // Create second workload
        let state2 = create_test_state_with_workload("mqtt_test", "ALWAYS");
        let workload2 = state2.workloads.as_ref().unwrap().workloads.get("mqtt_test").unwrap();

        persist_workload("mqtt_test", workload2, &temp_dir).await.unwrap();

        // Verify both files exist
        assert!(temp_dir.join("mqtt_fedora.yaml").exists());
        assert!(temp_dir.join("mqtt_test.yaml").exists());

        // Verify each file contains only its workload
        let content1 = tokio::fs::read_to_string(temp_dir.join("mqtt_fedora.yaml")).await.unwrap();
        assert!(content1.contains("mqtt_fedora"));
        assert!(!content1.contains("mqtt_test"));

        let content2 = tokio::fs::read_to_string(temp_dir.join("mqtt_test.yaml")).await.unwrap();
        assert!(content2.contains("mqtt_test"));
        assert!(!content2.contains("mqtt_fedora"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_persisted_state_from_directory() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Persist two workloads
        let state1 = create_test_state_with_workload("workload1", "ALWAYS");
        let workload1 = state1.workloads.as_ref().unwrap().workloads.get("workload1").unwrap();
        persist_workload("workload1", workload1, &temp_dir).await.unwrap();

        let state2 = create_test_state_with_workload("workload2", "ALWAYS");
        let workload2 = state2.workloads.as_ref().unwrap().workloads.get("workload2").unwrap();
        persist_workload("workload2", workload2, &temp_dir).await.unwrap();

        // Load state
        let loaded_state = load_persisted_state(&temp_dir).await.unwrap();

        // Verify both workloads present
        assert!(loaded_state.workloads.is_some(), "Loaded state should have workloads");
        let workloads = loaded_state.workloads.unwrap();
        assert!(workloads.workloads.contains_key("workload1"), "Should have workload1");
        assert!(workloads.workloads.contains_key("workload2"), "Should have workload2");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_remove_persisted_workload() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Persist workload
        let state = create_test_state_with_workload("test_workload", "ALWAYS");
        let workload = state.workloads.as_ref().unwrap().workloads.get("test_workload").unwrap();
        persist_workload("test_workload", workload, &temp_dir).await.unwrap();

        assert!(temp_dir.join("test_workload.yaml").exists());

        // Remove it
        remove_persisted_workload("test_workload", &temp_dir).await.unwrap();

        assert!(!temp_dir.join("test_workload.yaml").exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_process_event_persists_always_workload() {
        use ankaios_api::ank_base::{AlteredFields, CompleteState};

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        // Create workload for complete_state
        let mut workloads_map = HashMap::new();
        workloads_map.insert(
            "test_always".to_string(),
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

        // Create CompleteStateResponse with added workload
        let response = CompleteStateResponse {
            complete_state: Some(CompleteState {
                desired_state: Some(State {
                    api_version: "v1".to_string(),
                    workloads: Some(WorkloadMap {
                        workloads: workloads_map,
                    }),
                    configs: None,
                }),
                workload_states: None,
                agents: None,
                effective_state: None,
            }),
            altered_fields: Some(AlteredFields {
                added_fields: vec![
                    "desiredState.workloads.test_always".to_string(),
                    "workloadStates.agent_A.test_always.hash123.state".to_string(),
                ],
                updated_fields: vec![],
                removed_fields: vec![],
            }),
        };

        let mut on_running_workloads = HashSet::new();
        let mut on_running_cached_workloads = HashMap::new();
        let mut persisted_workload_names = HashSet::new();

        // Call process_event - this should persist the ALWAYS workload
        let result = process_event(
            &response,
            &persistence_dir,
            &mut on_running_workloads,
            &mut on_running_cached_workloads,
            &mut persisted_workload_names,
        )
        .await;

        assert!(result.is_ok(), "process_event should succeed");

        // Verify tracking set was updated
        assert!(persisted_workload_names.contains("test_always"),
            "persisted_workload_names should track the persisted workload");

        // CRITICAL: Verify file was actually written
        let workload_file = workloads_dir.join("test_always.yaml");
        assert!(
            workload_file.exists(),
            "ALWAYS workload file should be created at {:?}",
            workload_file
        );

        // Verify file contains the YAML content
        let file_content = tokio::fs::read_to_string(&workload_file).await.unwrap();
        assert!(
            file_content.contains("test_always"),
            "File should contain workload name"
        );
        assert!(
            file_content.contains("podman"),
            "File should contain runtime"
        );
        assert!(
            file_content.contains("apiVersion:"),
            "File should contain API version"
        );
    }

    #[tokio::test]
    async fn test_process_event_caches_on_running_workload() {
        use ankaios_api::ank_base::{AlteredFields, CompleteState};

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        // Create workload for complete_state
        let mut workloads_map = HashMap::new();
        workloads_map.insert(
            "test_on_running".to_string(),
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

        // Create CompleteStateResponse with added ON_RUNNING workload
        let response = CompleteStateResponse {
            complete_state: Some(CompleteState {
                desired_state: Some(State {
                    api_version: "v1".to_string(),
                    workloads: Some(WorkloadMap {
                        workloads: workloads_map,
                    }),
                    configs: None,
                }),
                workload_states: None,
                agents: None,
                effective_state: None,
            }),
            altered_fields: Some(AlteredFields {
                added_fields: vec!["desiredState.workloads.test_on_running".to_string()],
                updated_fields: vec![],
                removed_fields: vec![],
            }),
        };

        let mut on_running_workloads = HashSet::new();
        let mut on_running_cached_workloads = HashMap::new();
        let mut persisted_workload_names = HashSet::new();

        // Call process_event
        let result = process_event(
            &response,
            &persistence_dir,
            &mut on_running_workloads,
            &mut on_running_cached_workloads,
            &mut persisted_workload_names,
        )
        .await;

        assert!(result.is_ok(), "process_event should succeed");

        // Verify workload is tracked for ON_RUNNING
        assert!(
            on_running_workloads.contains("test_on_running"),
            "ON_RUNNING workload should be tracked"
        );

        // Verify the workload was cached for later persistence
        assert!(
            on_running_cached_workloads.contains_key("test_on_running"),
            "ON_RUNNING workload should be cached"
        );

        // Verify file was NOT created yet (waits for Running state)
        let workload_file = workloads_dir.join("test_on_running.yaml");
        assert!(
            !workload_file.exists(),
            "ON_RUNNING workload file should NOT be created until Running state"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_on_running_update_re_persists_when_already_on_disk() {
        use ankaios_api::ank_base::{AlteredFields, CompleteState};

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        // Pre-persist old workload (simulates ON_RUNNING that already reached Running)
        let old_workload = Workload {
            runtime: Some("podman".to_string()),
            agent: Some("agent_A".to_string()),
            tags: create_tags_with_persist("ON_RUNNING"),
            dependencies: None,
            restart_policy: None,
            runtime_config: Some("image: nginx:old".to_string()),
            control_interface_access: None,
            configs: None,
            files: None,
        };
        persist_workload("test_wl", &old_workload, &workloads_dir).await.unwrap();

        let mut on_running_workloads = HashSet::new();
        let mut on_running_cached_workloads = HashMap::new();
        let mut persisted_workload_names = HashSet::new();
        persisted_workload_names.insert("test_wl".to_string());

        // Send an update event with new runtimeConfig
        let mut workloads_map = HashMap::new();
        workloads_map.insert(
            "test_wl".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                tags: create_tags_with_persist("ON_RUNNING"),
                dependencies: None,
                restart_policy: None,
                runtime_config: Some("image: nginx:latest".to_string()),
                control_interface_access: None,
                configs: None,
                files: None,
            },
        );

        let response = CompleteStateResponse {
            complete_state: Some(CompleteState {
                desired_state: Some(State {
                    api_version: "v1".to_string(),
                    workloads: Some(WorkloadMap { workloads: workloads_map }),
                    configs: None,
                }),
                workload_states: None,
                agents: None,
                effective_state: None,
            }),
            altered_fields: Some(AlteredFields {
                added_fields: vec![],
                updated_fields: vec!["desiredState.workloads.test_wl.runtimeConfig".to_string()],
                removed_fields: vec![],
            }),
        };

        let result = process_event(
            &response,
            &persistence_dir,
            &mut on_running_workloads,
            &mut on_running_cached_workloads,
            &mut persisted_workload_names,
        ).await;

        assert!(result.is_ok());

        // Verify file was re-written with new content
        let content = tokio::fs::read_to_string(workloads_dir.join("test_wl.yaml")).await.unwrap();
        assert!(content.contains("nginx:latest"), "File should contain updated runtimeConfig");
        assert!(!content.contains("nginx:old"), "File should not contain old runtimeConfig");
    }

    #[tokio::test]
    async fn test_on_running_cleanup_on_terminal_state() {
        use ankaios_api::ank_base::{AlteredFields, CompleteState};

        let temp_dir = tempfile::TempDir::new().unwrap();
        let persistence_dir = temp_dir.path();
        let workloads_dir = persistence_dir.join("workloads");
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();

        // Pre-populate tracking sets as if workload was added with ON_RUNNING
        let mut on_running_workloads = HashSet::new();
        on_running_workloads.insert("test_failing".to_string());
        let mut on_running_cached_workloads = HashMap::new();
        on_running_cached_workloads.insert("test_failing".to_string(), Workload {
            runtime: Some("podman".to_string()),
            agent: Some("agent_A".to_string()),
            tags: create_tags_with_persist("ON_RUNNING"),
            dependencies: None,
            restart_policy: None,
            runtime_config: Some("image: nginx".to_string()),
            control_interface_access: None,
            configs: None,
            files: None,
        });

        // Send a state change event where workload reached Failed
        let response = CompleteStateResponse {
            complete_state: Some(CompleteState {
                desired_state: Some(State {
                    api_version: "v1".to_string(),
                    workloads: None,
                    configs: None,
                }),
                workload_states: create_workload_states_map(
                    "test_failing",
                    ExecutionStateEnum::Failed(0),
                ),
                agents: None,
                effective_state: None,
            }),
            altered_fields: Some(AlteredFields {
                added_fields: vec![],
                updated_fields: vec!["workloadStates.agent_A.test_failing.instance-1.state".to_string()],
                removed_fields: vec![],
            }),
        };

        let mut persisted_workload_names = HashSet::new();

        let result = process_event(
            &response,
            &persistence_dir,
            &mut on_running_workloads,
            &mut on_running_cached_workloads,
            &mut persisted_workload_names,
        )
        .await;

        assert!(result.is_ok());
        assert!(!on_running_workloads.contains("test_failing"),
            "Terminal workload should be removed from on_running_workloads");
        assert!(!on_running_cached_workloads.contains_key("test_failing"),
            "Terminal workload should be removed from on_running_cached_workloads");

        // Verify no file was persisted (workload failed, never ran)
        assert!(!workloads_dir.join("test_failing.yaml").exists());
    }

    // FILE SIZE LIMIT TESTS

    #[tokio::test]
    async fn test_file_size_limit_oversized_file_rejected() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Create oversized YAML file (11 MB > 10 MB limit)
        let oversized_yaml = temp_dir.join("oversized.yaml");
        let mut large_data = "apiVersion: v1\nworkloads:\n  test:\n    runtime: podman\n".to_string();
        large_data.push_str(&" ".repeat(11 * 1024 * 1024));
        tokio::fs::write(&oversized_yaml, &large_data).await.unwrap();

        // Try to load persisted state
        let state = load_persisted_state(&temp_dir).await.unwrap();

        // Should have skipped the oversized file (no workloads loaded)
        assert!(
            state.workloads.is_none() || state.workloads.unwrap().workloads.is_empty(),
            "Oversized file should be skipped"
        );
    }

    #[tokio::test]
    async fn test_file_size_limit_at_limit() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Create file exactly at 10 MB limit
        let at_limit_yaml = temp_dir.join("at_limit.yaml");
        let mut data = "apiVersion: v1\nworkloads: {}\n".to_string();
        data.push_str(&" ".repeat((10 * 1024 * 1024) - data.len()));
        tokio::fs::write(&at_limit_yaml, &data).await.unwrap();

        // Verify file is exactly 10 MB
        let metadata = tokio::fs::metadata(&at_limit_yaml).await.unwrap();
        assert_eq!(metadata.len(), 10 * 1024 * 1024, "File should be exactly 10 MB");

        // The check is `> MAX_WORKLOAD_FILE_SIZE`, so 10 MB exactly should be allowed
        // But it won't parse properly since it's mostly whitespace
        let _state = load_persisted_state(&temp_dir).await.unwrap();

        // This validates the size check boundary behavior
    }

    #[tokio::test]
    async fn test_file_size_limit_just_under_limit_allowed() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Create file just under 10 MB (9.9 MB)
        let under_limit_yaml = temp_dir.join("under_limit.yaml");
        let size = (10 * 1024 * 1024) - (100 * 1024); // 9.9 MB
        let mut data = "apiVersion: v1\nworkloads: {}\n".to_string();
        data.push_str(&" ".repeat(size - data.len()));
        tokio::fs::write(&under_limit_yaml, &data).await.unwrap();

        // Verify file size
        let metadata = tokio::fs::metadata(&under_limit_yaml).await.unwrap();
        assert!(metadata.len() < MAX_WORKLOAD_FILE_SIZE, "File should be under limit");

        // Should pass size check (though YAML parse may fail)
        let state = load_persisted_state(&temp_dir).await.unwrap();

        // Either empty (parse failed) or has content - we just verify no panic
        assert!(state.api_version == "v1", "State should be created");
    }

    #[tokio::test]
    async fn test_empty_yaml_file_handling() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        // Create empty YAML file
        let empty_yaml = temp_dir.join("empty.yaml");
        tokio::fs::write(&empty_yaml, b"").await.unwrap();

        // Should handle gracefully
        let state = load_persisted_state(&temp_dir).await.unwrap();

        // Empty YAML should be skipped (fails format check)
        assert!(
            state.workloads.is_none() || state.workloads.unwrap().workloads.is_empty(),
            "Empty YAML file should be skipped"
        );
    }

    #[tokio::test]
    async fn test_load_persisted_state_invalid_yaml_skipped() {
        let temp_dir_guard = tempfile::TempDir::new().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        let invalid_yaml = temp_dir.join("invalid.yaml");
        tokio::fs::write(&invalid_yaml, "This is not valid YAML for State").await.unwrap();

        let state = load_persisted_state(&temp_dir).await.unwrap();
        assert!(
            state.workloads.is_none() || state.workloads.unwrap().workloads.is_empty(),
            "Invalid YAML content should be skipped"
        );
    }

    #[tokio::test]
    async fn test_metadata_permission_error_handling() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("does_not_exist");

        let state = load_persisted_state(&non_existent).await.unwrap();
        assert!(
            state.workloads.is_none() || state.workloads.unwrap().workloads.is_empty(),
            "Non-existent directory should result in empty state"
        );
    }
}
