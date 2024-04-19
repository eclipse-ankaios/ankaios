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

use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
    time::Duration,
};
mod wait_list;
use tokio::time::interval;
use wait_list::WaitList;

#[cfg(not(test))]
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use common::{
    commands::{CompleteStateRequest, Response, ResponseContent},
    from_server_interface::{FromServer, FromServerReceiver},
    objects::{CompleteState, State, StoredWorkloadSpec, Tag, WorkloadInstanceName, WorkloadState},
    state_manipulation::{Object, Path},
    to_server_interface::{ToServer, ToServerInterface, ToServerSender},
};

#[cfg(not(test))]
use common::communications_client::CommunicationsClient;
#[cfg(not(test))]
use grpc::client::GRPCCommunicationsClient;

#[cfg(test)]
use tests::MockGRPCCommunicationsClient as GRPCCommunicationsClient;

use tabled::{settings::Style, Table, Tabled};
use url::Url;

use crate::{
    cli::{ApplyArgs, OutputFormat},
    cli_commands::wait_list::ParsedUpdateStateSuccess,
    output, output_and_error, output_debug,
};

use self::wait_list::WaitListDisplayTrait;

const BUFFER_SIZE: usize = 20;
const WAIT_TIME_MS: Duration = Duration::from_millis(3000);
const SPINNER_SYMBOLS: [&str; 4] = ["|", "/", "-", "\\"];
pub(crate) const COMPLETED_SYMBOL: &str = " ";

#[derive(Debug, Clone, PartialEq)]
pub enum CliError {
    YamlSerialization(String),
    JsonSerialization(String),
    ExecutionError(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CliError::YamlSerialization(message) => {
                write!(f, "Could not serialize YAML object: '{message}'")
            }
            CliError::JsonSerialization(message) => {
                write!(f, "Could not serialize JSON object: '{message}'")
            }
            CliError::ExecutionError(message) => {
                write!(f, "Command failed: '{}'", message)
            }
        }
    }
}

impl From<serde_yaml::Error> for CliError {
    fn from(value: serde_yaml::Error) -> Self {
        CliError::YamlSerialization(format!("{value}"))
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        CliError::JsonSerialization(format!("{value}"))
    }
}

fn generate_compact_state_output(
    state: &CompleteState,
    object_field_mask: Vec<String>,
    output_format: OutputFormat,
) -> Result<String, CliError> {
    let convert_to_output = |map: serde_yaml::Value| -> Result<String, CliError> {
        match output_format {
            // [impl -> swdd~cli-shall-support-desired-state-yaml~1]
            OutputFormat::Yaml => Ok(serde_yaml::to_string(&map)?),
            // [impl -> swdd~cli-shall-support-desired-state-json~1]
            OutputFormat::Json => Ok(serde_json::to_string_pretty(&map)?),
        }
    };

    let deserialized_state: serde_yaml::Value = serde_yaml::to_value(state)?;

    if object_field_mask.is_empty() {
        return convert_to_output(deserialized_state);
    }

    let mut compact_state = serde_yaml::Value::Mapping(Default::default());
    for mask in object_field_mask {
        let splitted_masks: Vec<&str> = mask.split('.').collect();
        if let Some(filtered_mapping) = get_filtered_value(&deserialized_state, &splitted_masks) {
            update_compact_state(
                &mut compact_state,
                &splitted_masks,
                filtered_mapping.to_owned(),
            );
        }
    }

    convert_to_output(compact_state)
}

fn get_filtered_value<'a>(
    map: &'a serde_yaml::Value,
    mask: &[&str],
) -> Option<&'a serde_yaml::Value> {
    mask.iter()
        .try_fold(map, |current_level, mask_part| current_level.get(mask_part))
}

fn update_compact_state(
    new_compact_state: &mut serde_yaml::Value,
    mask: &[&str],
    new_mapping: serde_yaml::Value,
) -> Option<()> {
    if mask.is_empty() {
        return Some(());
    }

    let mut current_level = new_compact_state;

    for mask_part in mask {
        if current_level.get(mask_part).is_some() {
            current_level = current_level.get_mut(mask_part)?;
            continue;
        }

        if let serde_yaml::Value::Mapping(current_mapping) = current_level {
            current_mapping.insert(
                (*mask_part).into(),
                serde_yaml::Value::Mapping(Default::default()),
            );

            current_level = current_mapping.get_mut(mask_part)?;
        } else {
            return None;
        }
    }

    *current_level = new_mapping;
    Some(())
}

// [impl->swdd~server-handle-cli-communication~1]
// [impl->swdd~cli-communication-over-middleware~1]
fn setup_cli_communication(
    cli_name: &str,
    server_url: Url,
) -> (
    tokio::task::JoinHandle<()>,
    ToServerSender,
    FromServerReceiver,
) // (task,sender,receiver)
{
    let mut grpc_communications_client =
        GRPCCommunicationsClient::new_cli_communication(cli_name.to_owned(), server_url);

    let (to_cli, cli_receiver) = tokio::sync::mpsc::channel::<FromServer>(BUFFER_SIZE);
    let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);

    let communications_task = tokio::spawn(async move {
        if let Err(err) = grpc_communications_client
            .run(server_receiver, to_cli.clone())
            .await
        {
            output_and_error!("{err}");
        }
    });
    (communications_task, to_server, cli_receiver)
}

mod apply_manifests {
    use crate::cli_commands::{ApplyManifestTableDisplay, State};
    use crate::{cli::ApplyArgs, output_debug};
    use common::objects::CompleteState;
    use common::state_manipulation::{Object, Path};
    use std::{collections::HashSet, io};

    pub type InputSourcePair = (String, Box<dyn io::Read + Send + Sync + 'static>);
    pub type InputSources = Result<Vec<InputSourcePair>, String>;

    #[cfg(not(test))]
    pub fn open_manifest(
        file_path: &str,
    ) -> io::Result<(String, Box<dyn io::Read + Send + Sync + 'static>)> {
        use std::fs::File;
        match File::open(file_path) {
            Ok(open_file) => Ok((file_path.to_owned(), Box::new(open_file))),
            Err(err) => Err(err),
        }
    }
    #[cfg(test)]
    use super::tests::open_manifest_mock as open_manifest;

    // [impl->swdd~cli-apply-supports-ankaios-manifest~1]
    pub fn parse_manifest(manifest: &mut InputSourcePair) -> Result<(Object, Vec<Path>), String> {
        let state_obj_parsing_check: serde_yaml::Value =
            serde_yaml::from_reader(&mut manifest.1)
                .map_err(|err| format!("Invalid manifest data provided: {}", err))?;
        match Object::try_from(&state_obj_parsing_check) {
            Err(err) => Err(format!(
                "Error while parsing the manifest data.\nError: {err}"
            )),
            Ok(obj) => {
                let mut workload_paths: HashSet<Path> = HashSet::new();
                for path in Vec::<Path>::from(&obj) {
                    let parts = path.parts();
                    if parts.len() > 1 {
                        let _ = &mut workload_paths
                            .insert(Path::from(format!("{}.{}", parts[0], parts[1])));
                    }
                }

                Ok((obj, workload_paths.into_iter().collect()))
            }
        }
    }

    // [impl->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    pub fn handle_agent_overwrite(
        filter_masks: &Vec<common::state_manipulation::Path>,
        desired_agent: &Option<String>,
        mut state_obj: Object,
        table_output: &mut [ApplyManifestTableDisplay],
    ) -> Result<State, String> {
        // No agent name specified through cli!
        if desired_agent.is_none() {
            // [impl->swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~1]
            for field in filter_masks {
                let path = &format!("{}.agent", String::from(field));
                if state_obj.get(&path.into()).is_none() {
                    return Err(
                        "No agent name specified -> use '--agent' option to specify!".to_owned(),
                    );
                }
            }
        }
        // An agent name specified through cli -> do an agent name overwrite!
        else {
            let desired_agent_name = desired_agent.as_ref().unwrap().to_string();
            for field in filter_masks {
                let path = &format!("{}.agent", String::from(field));
                if state_obj
                    .set(
                        &path.into(),
                        serde_yaml::Value::String(desired_agent_name.to_owned()),
                    )
                    .is_err()
                {
                    return Err("Could not find workload to update.".to_owned());
                }
            }
            table_output.iter_mut().for_each(|row| {
                row.base_info.agent = desired_agent_name.to_string();
            })
        }

        state_obj
            .try_into()
            .map_err(|err| format!("Invalid manifest data provided: {}", err))
    }

    pub fn update_request_obj(
        req_obj: &mut Object,
        cur_obj: &Object,
        paths: &[Path],
        manifest_file_name: &str,
        delete_mode: bool,
        table_output: &mut Vec<ApplyManifestTableDisplay>,
    ) -> Result<(), String> {
        for workload_path in paths.iter() {
            let workload_name = &workload_path.parts()[1];
            let cur_workload_spec = cur_obj.get(workload_path).unwrap().clone();
            if req_obj.get(workload_path).is_none() {
                let _ = req_obj.set(workload_path, cur_workload_spec.clone());
                let mapping_default = serde_yaml::Mapping::default();
                let value_default = serde_yaml::Value::default();
                let mut agent_name = "";

                if !delete_mode {
                    agent_name = cur_workload_spec
                        .as_mapping()
                        .unwrap_or(&mapping_default)
                        .get("agent")
                        .unwrap_or(&value_default)
                        .as_str()
                        .unwrap_or_default()
                };

                table_output.push(ApplyManifestTableDisplay::new(
                    workload_name,
                    agent_name,
                    if delete_mode {
                        super::ApplyManifestOperation::Remove
                    } else {
                        super::ApplyManifestOperation::AddOrUpdate
                    },
                    manifest_file_name,
                ));
            } else {
                return Err(format!(
                    "Multiple workloads with the same name '{}' found!",
                    workload_name
                ));
            }
        }

        Ok(())
    }

    pub fn create_filter_masks_from_paths(
        paths: &[common::state_manipulation::Path],
        prefix: &str,
    ) -> Vec<String> {
        let mut filter_masks = paths
            .iter()
            .map(|path| format!("{}.{}", prefix, String::from(path)))
            .collect::<Vec<String>>();
        filter_masks.sort();
        filter_masks.dedup();
        filter_masks
    }
    // [impl->swdd~cli-apply-generates-state-object-from-ankaios-manifests~1]
    // [impl->swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1]
    pub fn generate_state_obj_and_filter_masks_from_manifests(
        manifests: &mut [InputSourcePair],
        apply_args: &ApplyArgs,
        table_output: &mut Vec<ApplyManifestTableDisplay>,
    ) -> Result<(CompleteState, Vec<String>), String> {
        let mut req_obj: Object = State::default().try_into().unwrap();
        let mut req_paths: Vec<common::state_manipulation::Path> = Vec::new();
        for manifest in manifests.iter_mut() {
            let (cur_obj, mut cur_workload_paths) = parse_manifest(manifest)?;

            update_request_obj(
                &mut req_obj,
                &cur_obj,
                &cur_workload_paths,
                &manifest.0,
                apply_args.delete_mode,
                table_output,
            )?;

            req_paths.append(&mut cur_workload_paths);
        }

        if req_paths.is_empty() {
            return Err("No workload provided in manifests!".to_owned());
        }

        let filter_masks = create_filter_masks_from_paths(&req_paths, "desiredState");
        output_debug!("\nfilter_masks:\n{:?}\n", filter_masks);

        let complete_state_req_obj = if apply_args.delete_mode {
            CompleteState {
                ..Default::default()
            }
        } else {
            let state_from_req_obj =
                handle_agent_overwrite(&req_paths, &apply_args.agent_name, req_obj, table_output)?;
            CompleteState {
                desired_state: state_from_req_obj,
                ..Default::default()
            }
        };
        output_debug!("\nstate_obj:\n{:?}\n", complete_state_req_obj);

        Ok((complete_state_req_obj, filter_masks))
    }

    impl ApplyArgs {
        pub fn get_input_sources(&self) -> InputSources {
            if let Some(first_arg) = self.manifest_files.first() {
                match first_arg.as_str() {
                    // [impl->swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1]
                    "-" => Ok(vec![("stdin".to_owned(), Box::new(io::stdin()))]),
                    // [impl->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
                    _ => {
                        let mut res: InputSources = Ok(vec![]);
                        for file_path in self.manifest_files.iter() {
                            match open_manifest(file_path) {
                                Ok(open_file) => res.as_mut().unwrap().push(open_file),
                                Err(err) => {
                                    res = Err(match err.kind() {
                                        io::ErrorKind::NotFound => {
                                            format!("File '{}' not found!", file_path)
                                        }
                                        _ => err.to_string(),
                                    });
                                    break;
                                }
                            }
                        }
                        res
                    }
                }
            } else {
                Ok(vec![])
            }
        }
    }
}
#[derive(Debug, Tabled, Clone)]
#[tabled(rename_all = "UPPERCASE")]
struct WorkloadBaseTableDisplay {
    #[tabled(rename = "WORKLOAD NAME")]
    name: String,
    agent: String,
}

impl WorkloadBaseTableDisplay {
    fn new(name: &str, agent: &str) -> Self {
        WorkloadBaseTableDisplay {
            name: name.to_string(),
            agent: agent.to_string(),
        }
    }
}
#[derive(Debug, Tabled, Clone)]
#[tabled(rename_all = "UPPERCASE")]
struct GetWorkloadTableDisplay {
    #[tabled(inline)]
    base_info: WorkloadBaseTableDisplay,
    runtime: String,
    #[tabled(rename = "EXECUTION STATE")]
    execution_state: String,
    #[tabled(rename = "ADDITIONAL INFO")]
    additional_info: String,
}

struct GetWorkloadTableDisplayWithSpinner<'a> {
    data: &'a GetWorkloadTableDisplay,
    spinner: &'a str,
}

impl GetWorkloadTableDisplay {
    const EXECUTION_STATE_POS: usize = 3;
}

impl<'a> Tabled for GetWorkloadTableDisplayWithSpinner<'a> {
    const LENGTH: usize = GetWorkloadTableDisplay::LENGTH;

    fn fields(&self) -> Vec<std::borrow::Cow<'_, str>> {
        let mut fields = self.data.fields();
        *(fields[GetWorkloadTableDisplay::EXECUTION_STATE_POS].to_mut()) = format!(
            "{} {}",
            self.spinner,
            fields[GetWorkloadTableDisplay::EXECUTION_STATE_POS]
        );
        fields
    }

    fn headers() -> Vec<std::borrow::Cow<'static, str>> {
        let mut headers = GetWorkloadTableDisplay::headers();
        *(headers[GetWorkloadTableDisplay::EXECUTION_STATE_POS].to_mut()) = format!(
            "  {}",
            headers[GetWorkloadTableDisplay::EXECUTION_STATE_POS]
        );
        headers
    }
}

struct WaitListDisplay {
    data: HashMap<WorkloadInstanceName, GetWorkloadTableDisplay>,
    not_completed: HashSet<WorkloadInstanceName>,
    spinner: Spinner,
}

impl Display for WaitListDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let current_spinner = self.spinner.to_string();
        let mut data: Vec<_> = self
            .data
            .iter()
            .map(|(workload_name, table_entry)| {
                let update_state_symbol = if self.not_completed.contains(workload_name) {
                    &current_spinner
                } else {
                    COMPLETED_SYMBOL
                };
                GetWorkloadTableDisplayWithSpinner {
                    data: table_entry,
                    spinner: update_state_symbol,
                }
            })
            .collect();
        data.sort_by_key(|x| &x.data.base_info.name);

        // [impl->swdd~cli-shall-present-workloads-as-table~1]
        write!(
            f,
            "{}",
            Table::new(data).with(tabled::settings::Style::blank())
        )
    }
}

impl WaitListDisplayTrait for WaitListDisplay {
    fn update(&mut self, workload_state: &common::objects::WorkloadState) {
        if let Some(entry) = self.data.get_mut(&workload_state.instance_name) {
            entry.execution_state = workload_state.execution_state.state.to_string();
            entry.additional_info = workload_state.execution_state.additional_info.clone();
        }
    }

    fn set_complete(&mut self, workload: &WorkloadInstanceName) {
        self.not_completed.remove(workload);
    }

    fn step_spinner(&mut self) {
        self.spinner.step();
    }
}

#[derive(Default)]
struct Spinner {
    pos: usize,
}

impl Spinner {
    pub fn step(&mut self) {
        self.pos = (self.pos + 1) % SPINNER_SYMBOLS.len();
    }
}

impl Display for Spinner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", SPINNER_SYMBOLS[self.pos])
    }
}

impl GetWorkloadTableDisplay {
    fn new(
        name: &str,
        agent: &str,
        runtime: &str,
        execution_state: &str,
        additional_info: &str,
    ) -> Self {
        GetWorkloadTableDisplay {
            base_info: WorkloadBaseTableDisplay::new(name, agent),
            runtime: runtime.to_string(),
            execution_state: execution_state.to_string(),
            additional_info: additional_info.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
enum ApplyManifestOperation {
    AddOrUpdate,
    Remove,
}

impl fmt::Display for ApplyManifestOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyManifestOperation::AddOrUpdate => write!(f, "Add/Update"),
            ApplyManifestOperation::Remove => write!(f, "Remove"),
        }
    }
}

#[derive(Debug, Tabled, Clone)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ApplyManifestTableDisplay {
    #[tabled(inline)]
    base_info: WorkloadBaseTableDisplay,
    operation: ApplyManifestOperation,
    #[tabled(rename = "FILE")]
    manifest_file: String,
}

impl ApplyManifestTableDisplay {
    fn new(
        workload_name: &str,
        agent_name: &str,
        operation: ApplyManifestOperation,
        manifest_file: &str,
    ) -> Self {
        ApplyManifestTableDisplay {
            base_info: WorkloadBaseTableDisplay::new(workload_name, agent_name),
            operation,
            manifest_file: manifest_file.to_string(),
        }
    }
}

pub struct CliCommands {
    // Left here for the future use.
    _response_timeout_ms: u64,
    cli_name: String,
    task: tokio::task::JoinHandle<()>,
    to_server: ToServerSender,
    from_server: FromServerReceiver,
    no_wait: bool,
}

impl CliCommands {
    pub fn init(
        response_timeout_ms: u64,
        cli_name: String,
        server_url: Url,
        no_wait: bool,
    ) -> Self {
        let (task, to_server, from_server) =
            setup_cli_communication(cli_name.as_str(), server_url.clone());
        Self {
            _response_timeout_ms: response_timeout_ms,
            cli_name,
            task,
            to_server,
            from_server,
            no_wait,
        }
    }

    pub async fn shut_down(self) {
        drop(self.to_server);

        let _ = self.task.await;
    }

    async fn get_complete_state(
        &mut self,
        object_field_mask: &Vec<String>,
    ) -> Result<Box<CompleteState>, CliError> {
        output_debug!(
            "get_complete_state: object_field_mask={:?} ",
            object_field_mask
        );

        // send complete state request to server
        self.to_server
            .request_complete_state(
                self.cli_name.to_owned(),
                CompleteStateRequest {
                    field_mask: object_field_mask.clone(),
                },
            )
            .await
            .map_err(|err| CliError::ExecutionError(err.to_string()))?;

        let poll_complete_state_response = async {
            loop {
                match self.from_server.recv().await {
                    Some(FromServer::Response(Response {
                        request_id: _,
                        response_content: ResponseContent::CompleteState(res),
                    })) => return Ok(res),
                    None => return Err("Channel preliminary closed."),
                    Some(message) => {
                        output_debug!("Got unexpected message: {:?}", message)
                    }
                }
            }
        };
        match tokio::time::timeout(WAIT_TIME_MS, poll_complete_state_response).await {
            Ok(Ok(res)) => Ok(res),
            Ok(Err(err)) => Err(CliError::ExecutionError(format!(
                "Failed to get complete state.\nError: {err}"
            ))),
            Err(_) => Err(CliError::ExecutionError(format!(
                "Failed to get complete state in time (timeout={WAIT_TIME_MS:?})."
            ))),
        }
    }

    pub async fn get_state(
        &mut self,
        object_field_mask: Vec<String>,
        output_format: OutputFormat,
    ) -> Result<String, CliError> {
        output_debug!(
            "Got: object_field_mask={:?} output_format={:?}",
            object_field_mask,
            output_format
        );

        let res_complete_state = self.get_complete_state(&object_field_mask).await?;
        // [impl->swdd~cli-returns-api-version-with-desired-state~1]
        // [impl->swdd~cli-returns-api-version-with-startup-state~1]
        // [impl->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
        match generate_compact_state_output(&res_complete_state, object_field_mask, output_format) {
            Ok(res) => Ok(res),
            Err(err) => {
                output_and_error!(
                    "Error occurred during processing response from server.\nError: {err}"
                );
                Err(err)
            }
        }
    }

    fn add_default_workload_spec_per_update_mask(
        update_mask: &Vec<String>,
        complete_state: &mut CompleteState,
    ) {
        for field_mask in update_mask {
            let path: Path = field_mask.into();

            // if we want to set an attribute of a workload create a default object for the workload
            if path.parts().len() >= 4
                && path.parts()[0] == "desiredState"
                && path.parts()[1] == "workloads"
            {
                let stored_workload = StoredWorkloadSpec {
                    agent: "".to_string(),
                    runtime: "".to_string(),
                    runtime_config: "".to_string(),
                    ..Default::default()
                };

                complete_state
                    .desired_state
                    .workloads
                    .insert(path.parts()[2].to_string(), stored_workload);
            }
        }
    }

    pub async fn set_state(
        &mut self,
        object_field_mask: Vec<String>,
        state_object_file: Option<String>,
    ) -> Result<(), CliError> {
        output_debug!(
            "Got: object_field_mask={:?} state_object_file={:?}",
            object_field_mask,
            state_object_file
        );

        let mut complete_state = CompleteState::default();
        if let Some(state_object_file) = state_object_file {
            let state_object_data =
                read_file_to_string(state_object_file)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("Could not read the state object file.\nError: {}", error)
                    });
            let value: serde_yaml::Value = serde_yaml::from_str(&state_object_data)?;
            let x = Object::try_from(&value)?;

            // This here is a workaround for the default workload specs
            Self::add_default_workload_spec_per_update_mask(
                &object_field_mask,
                &mut complete_state,
            );

            // now overwrite with the values from the field mask
            let mut complete_state_object: Object = complete_state.try_into()?;
            for field_mask in &object_field_mask {
                let path: Path = field_mask.into();

                complete_state_object
                    .set(
                        &path,
                        x.get(&path)
                            .ok_or(CliError::ExecutionError(format!(
                                "Specified update mask '{field_mask}' not found in the input config.",
                            )))?
                            .clone(),
                    )
                    .map_err(|err| CliError::ExecutionError(err.to_string()))?;
            }
            complete_state = complete_state_object.try_into()?;
        }

        output_debug!(
            "Send UpdateState request with the CompleteState {:?}",
            complete_state
        );

        // [impl->swdd~cli-blocks-until-ankaios-server-responds-set-desired-state~2]
        self.update_state_and_wait_for_complete(complete_state, object_field_mask)
            .await
    }

    // [impl->swdd~cli-provides-list-of-workloads~1]
    pub async fn get_workloads_table(
        &mut self,
        agent_name: Option<String>,
        state: Option<String>,
        workload_name: Vec<String>,
    ) -> Result<String, CliError> {
        // [impl->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
        let mut workload_infos = self.get_workloads().await?;
        output_debug!("The table before filtering:\n{:?}", workload_infos);

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        if let Some(agent_name) = agent_name {
            workload_infos.retain(|wi| wi.1.base_info.agent == agent_name);
        }

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        if let Some(state) = state {
            workload_infos.retain(|wi| wi.1.execution_state.to_lowercase() == state.to_lowercase());
        }

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        if !workload_name.is_empty() {
            workload_infos.retain(|wi| workload_name.iter().any(|wn| wn == &wi.1.base_info.name));
        }

        // The order of workloads in RequestCompleteState is not sable -> make sure that the user sees always the same order.
        // [impl->swdd~cli-shall-sort-list-of-workloads~1]
        workload_infos.sort_by_key(|wi| wi.1.base_info.name.clone());

        output_debug!("The table after filtering:\n{:?}", workload_infos);

        // [impl->swdd~cli-shall-present-list-of-workloads~1]
        // [impl->swdd~cli-shall-present-workloads-as-table~1]
        Ok(Table::new(workload_infos.iter().map(|x| &x.1))
            .with(Style::blank())
            .to_string())
    }

    async fn get_workloads(
        &mut self,
    ) -> Result<Vec<(WorkloadInstanceName, GetWorkloadTableDisplay)>, CliError> {
        let res_complete_state = self.get_complete_state(&Vec::new()).await?;

        let mut workload_infos: Vec<(WorkloadInstanceName, GetWorkloadTableDisplay)> =
            res_complete_state
                .workload_states
                .into_iter()
                .map(|wl_state| {
                    (
                        wl_state.instance_name.clone(),
                        GetWorkloadTableDisplay::new(
                            wl_state.instance_name.workload_name(),
                            wl_state.instance_name.agent_name(),
                            Default::default(),
                            &wl_state.execution_state.state.to_string(),
                            &wl_state.execution_state.additional_info.to_string(),
                        ),
                    )
                })
                .collect();

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        for wi in &mut workload_infos {
            if let Some((_found_wl_name, found_wl_spec)) = res_complete_state
                .desired_state
                .workloads
                .iter()
                .find(|&(wl_name, wl_spec)| {
                    *wl_name == wi.1.base_info.name && wl_spec.agent == wi.1.base_info.agent
                })
            {
                wi.1.runtime = found_wl_spec.runtime.clone();
            }
        }

        Ok(workload_infos)
    }

    // [impl->swdd~cli-provides-delete-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    pub async fn delete_workloads(&mut self, workload_names: Vec<String>) -> Result<(), CliError> {
        let complete_state = self.get_complete_state(&Vec::new()).await?;

        output_debug!("Got current state: {:?}", complete_state);
        let mut new_state = complete_state.clone();
        // Filter out workloads to be deleted.
        new_state
            .desired_state
            .workloads
            .retain(|k, _v| !workload_names.clone().into_iter().any(|wn| &wn == k));

        // Filter out workload statuses of the workloads to be deleted.
        // Only a nice-to-have, but it could be better to avoid sending misleading information
        new_state.workload_states.retain(|ws| {
            !workload_names
                .clone()
                .into_iter()
                .any(|wn| wn == ws.instance_name.workload_name())
        });

        let update_mask = vec!["desiredState".to_string()];
        self.update_state_and_wait_for_complete(*new_state, update_mask)
            .await
    }

    // [impl->swdd~cli-provides-run-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    pub async fn run_workload(
        &mut self,
        workload_name: String,
        runtime_name: String,
        runtime_config: String,
        agent_name: String,
        tags_strings: Vec<(String, String)>,
    ) -> Result<(), CliError> {
        let tags: Vec<Tag> = tags_strings
            .into_iter()
            .map(|(k, v)| Tag { key: k, value: v })
            .collect();

        let new_workload = StoredWorkloadSpec {
            agent: agent_name,
            runtime: runtime_name,
            tags,
            runtime_config,
            ..Default::default()
        };
        output_debug!("Request to run new workload: {:?}", new_workload);

        let res_complete_state = self.get_complete_state(&Vec::new()).await?;
        output_debug!("Got current state: {:?}", res_complete_state);
        let mut new_state = *res_complete_state.clone();
        new_state
            .desired_state
            .workloads
            .insert(workload_name, new_workload);

        let update_mask = vec!["desiredState".to_string()];

        self.update_state_and_wait_for_complete(new_state, update_mask)
            .await
    }

    // [impl->swdd~cli-requests-update-state-with-watch~1]
    async fn update_state_and_wait_for_complete(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), CliError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        output_debug!("Sending the new state {:?}", new_state);
        self.to_server
            .update_state(request_id.clone(), new_state, update_mask)
            .await
            .map_err(|err| CliError::ExecutionError(err.to_string()))?;

        let mut missed_messages = Vec::new();

        let update_state_success = loop {
            let Some(server_message) = self.from_server.recv().await else {
                return Err(CliError::ExecutionError(
                    "Connection to server interrupted".into(),
                ));
            };
            match server_message {
                FromServer::Response(response) => {
                    if response.request_id != request_id {
                        output_debug!(
                            "Received unexpected response for request ID: '{}'",
                            response.request_id
                        );
                    } else {
                        match response.response_content {
                            ResponseContent::UpdateStateSuccess(update_state_success) => {
                                break update_state_success
                            }
                            // [impl->swdd~cli-requests-update-state-with-watch-error~1]
                            ResponseContent::Error(error) => {
                                return Err(CliError::ExecutionError(format!(
                                    "SetState failed with: '{}'",
                                    error.message
                                )));
                            }
                            // [impl->swdd~cli-requests-update-state-with-watch-error~1]
                            response_content => {
                                return Err(CliError::ExecutionError(format!(
                                    "Received unexpected response: {:?}",
                                    response_content
                                )));
                            }
                        }
                    }
                }
                FromServer::UpdateWorkloadState(mut update_workload_state) => {
                    missed_messages.append(&mut update_workload_state.workload_states);
                }
                other_message => {
                    output_debug!("Received unexpected message: {:?}", other_message)
                }
            }
        };

        output_debug!("Got update success: {:?}", update_state_success);

        // [impl->swdd~cli-requests-update-state-with-watch-error~1]
        let update_state_success = ParsedUpdateStateSuccess::try_from(update_state_success)
            .map_err(|error| {
                CliError::ExecutionError(format!(
                    "Could not parse UpdateStateSuccess message: {error}"
                ))
            })?;

        if self.no_wait {
            Ok(())
        } else {
            // [impl->swdd~cli-requests-update-state-with-watch-success~1]
            self.wait_for_complete(update_state_success, missed_messages)
                .await
        }
    }

    // [impl->swdd~cli-watches-workloads~1]
    async fn wait_for_complete(
        &mut self,
        update_state_success: ParsedUpdateStateSuccess,
        missed_messages: Vec<WorkloadState>,
    ) -> Result<(), CliError> {
        let mut changed_workloads =
            HashSet::from_iter(update_state_success.added_workloads.iter().cloned());
        changed_workloads.extend(update_state_success.deleted_workloads.iter().cloned());

        if changed_workloads.is_empty() {
            output!("No workloads to update");
            return Ok(());
        }

        let states_of_all_workloads = self.get_workloads().await.unwrap();
        let states_of_changed_workloads = states_of_all_workloads
            .into_iter()
            .filter(|x| changed_workloads.contains(&x.0))
            .collect::<Vec<_>>();

        let mut wait_list = WaitList::new(
            update_state_success,
            WaitListDisplay {
                data: states_of_changed_workloads.into_iter().collect(),
                spinner: Default::default(),
                not_completed: changed_workloads,
            },
        );

        wait_list.update(missed_messages);
        let mut spinner_interval = interval(Duration::from_millis(100));

        while !wait_list.is_empty() {
            tokio::select! {
                server_message = self.from_server.recv() => {
                    output_debug!("Got server message: {:?}", server_message);
                    let Some(server_message) = server_message else {
                        return Err(CliError::ExecutionError(
                            "Connection to server interrupted".into(),
                        ));
                    };
                    if let FromServer::UpdateWorkloadState(update_workload_state) = server_message {
                        wait_list.update(update_workload_state.workload_states);
                    } else {
                        output_debug!("Received unexpected message: {:?}", server_message);
                    }
                }
                _ = spinner_interval.tick() => {
                    wait_list.step_spinner();
                }
            }
        }
        Ok(())
    }

    // [impl->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
    pub async fn apply_manifests(&mut self, apply_args: ApplyArgs) -> Result<(), CliError> {
        use apply_manifests::*;
        match apply_args.get_input_sources() {
            Ok(mut manifests) => {
                let mut table_output = Vec::<ApplyManifestTableDisplay>::default();
                let (complete_state_req_obj, filter_masks) =
                    generate_state_obj_and_filter_masks_from_manifests(
                        &mut manifests,
                        &apply_args,
                        &mut table_output,
                    )
                    .map_err(CliError::ExecutionError)?;

                // [impl->swdd~cli-apply-send-update-state~1]
                self.update_state_and_wait_for_complete(complete_state_req_obj, filter_masks)
                    .await
            }
            Err(err) => Err(CliError::ExecutionError(err.to_string())),
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
    use common::{
        commands::{
            self, CompleteStateRequest, RequestContent, Response, ResponseContent,
            UpdateStateRequest, UpdateStateSuccess, UpdateWorkloadState,
        },
        from_server_interface::{FromServer, FromServerSender},
        objects::{
            self, generate_test_workload_spec_with_param, CompleteState, ExecutionState,
            RunningSubstate, State, StoredWorkloadSpec, Tag, WorkloadState,
        },
        state_manipulation::{Object, Path},
        test_utils::{self, generate_test_complete_state},
        to_server_interface::{ToServer, ToServerReceiver},
    };
    use std::{collections::HashMap, io, thread};
    use tabled::{settings::Style, Table};
    use tokio::sync::mpsc::{Receiver, Sender};

    use super::apply_manifests::{
        create_filter_masks_from_paths, generate_state_obj_and_filter_masks_from_manifests,
        handle_agent_overwrite, parse_manifest, update_request_obj, InputSourcePair,
    };
    use super::{ApplyManifestOperation, ApplyManifestTableDisplay};
    use crate::{
        cli::OutputFormat,
        cli_commands::{
            generate_compact_state_output, get_filtered_value, update_compact_state, ApplyArgs,
            GetWorkloadTableDisplay,
        },
    };
    use serde_yaml::Value;
    use std::io::Read;

    use super::CliCommands;

    use url::Url;

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    const EXAMPLE_STATE_INPUT: &str = r#"{
        "desiredState": {
            "workloads": {
                "nginx": {
                    "restartPolicy": ALWAYS,
                    "agent": "agent_A"
                },
                "hello1": {
                    "agent": "agent_B"
                }
            }
        }
    }"#;

    mockall::lazy_static! {
        pub static ref FAKE_READ_TO_STRING_MOCK_RESULT_LIST: tokio::sync::Mutex<std::collections::VecDeque<io::Result<String>>>  =
        tokio::sync::Mutex::new(std::collections::VecDeque::new());

        pub static ref FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST: std::sync::Mutex<std::collections::VecDeque<io::Result<InputSourcePair>>>  =
        std::sync::Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn read_to_string_mock(_file: String) -> io::Result<String> {
        FAKE_READ_TO_STRING_MOCK_RESULT_LIST
            .lock()
            .await
            .pop_front()
            .unwrap()
    }

    mockall::mock! {
        pub GRPCCommunicationsClient {
            pub fn new_cli_communication(name: String, server_address: Url) -> Self;
            pub async fn run(
                &mut self,
                mut server_rx: ToServerReceiver,
                agent_tx: FromServerSender,
            ) -> Result<(), String>;
        }
    }

    fn generate_multiple_test_apply_manifest_table_display() -> String {
        let operation = ApplyManifestOperation::AddOrUpdate;
        return tabled::Table::new(vec![
            ApplyManifestTableDisplay::new(
                "simple",
                "agent1",
                operation.clone(),
                "manifest_file_name",
            ),
            ApplyManifestTableDisplay::new("complex", "agent1", operation, "manifest_file_name"),
        ])
        .with(tabled::settings::Style::blank())
        .to_string();
    }

    fn generate_multiple_test_apply_manifest_table_display_operation_remove() -> String {
        let operation = ApplyManifestOperation::Remove;
        return tabled::Table::new(vec![
            ApplyManifestTableDisplay::new("simple", "", operation.clone(), "manifest_file_name"),
            ApplyManifestTableDisplay::new("complex", "", operation, "manifest_file_name"),
        ])
        .with(tabled::settings::Style::blank())
        .to_string();
    }

    fn generate_apply_manifest_table_output(
        table_output: &Vec<super::ApplyManifestTableDisplay>,
    ) -> String {
        tabled::Table::new(table_output)
            .with(tabled::settings::Style::blank())
            .to_string()
    }

    pub fn open_manifest_mock(
        _file_path: &str,
    ) -> io::Result<(String, Box<dyn io::Read + Send + Sync + 'static>)> {
        FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    fn prepare_server_response(
        complete_states: Vec<FromServer>,
        to_cli: FromServerSender,
    ) -> Result<(), String> {
        let sync_code = thread::spawn(move || {
            complete_states.into_iter().for_each(|cs| {
                to_cli.blocking_send(cs).unwrap();
            });
        });
        sync_code.join().unwrap();
        Ok(())
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[tokio::test]
    async fn get_workloads_empty_table() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let empty_complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(
                test_utils::generate_test_complete_state(Vec::new()),
            )),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(empty_complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd.get_workloads_table(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_empty_table: Vec<GetWorkloadTableDisplay> = Vec::new();
        let expected_table_text = Table::new(expected_empty_table)
            .with(Style::blank())
            .to_string();

        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-provides-list-of-workloads~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
    // [utest->swdd~cli-shall-present-list-of-workloads~1]
    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    // [utest->swdd~cli-shall-sort-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_no_filtering() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(
                test_utils::generate_test_complete_state(vec![
                    generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ]),
            )),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd.get_workloads_table(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<GetWorkloadTableDisplay> = vec![
            GetWorkloadTableDisplay::new(
                "name1",
                "agent_A",
                "runtime",
                &ExecutionState::running().state.to_string(),
                Default::default(),
            ),
            GetWorkloadTableDisplay::new(
                "name2",
                "agent_B",
                "runtime",
                &ExecutionState::running().state.to_string(),
                Default::default(),
            ),
            GetWorkloadTableDisplay::new(
                "name3",
                "agent_B",
                "runtime",
                &ExecutionState::running().state.to_string(),
                Default::default(),
            ),
        ];
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_workload_name() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(
                test_utils::generate_test_complete_state(vec![
                    generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ]),
            )),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_workloads_table(None, None, vec!["name1".to_string()])
            .await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<GetWorkloadTableDisplay> = vec![GetWorkloadTableDisplay::new(
            "name1",
            "agent_A",
            "runtime",
            &ExecutionState::running().state.to_string(),
            Default::default(),
        )];
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_agent() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(
                test_utils::generate_test_complete_state(vec![
                    generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ]),
            )),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_workloads_table(Some("agent_B".to_string()), None, Vec::new())
            .await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<GetWorkloadTableDisplay> = vec![
            GetWorkloadTableDisplay::new(
                "name2",
                "agent_B",
                "runtime",
                &ExecutionState::running().state.to_string(),
                Default::default(),
            ),
            GetWorkloadTableDisplay::new(
                "name3",
                "agent_B",
                "runtime",
                &ExecutionState::running().state.to_string(),
                Default::default(),
            ),
        ];
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(
                test_utils::generate_test_complete_state(vec![
                    generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ]),
            )),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_workloads_table(None, Some("Failed".to_string()), Vec::new())
            .await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<GetWorkloadTableDisplay> = Vec::new();
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[tokio::test]
    async fn get_workloads_deleted_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = objects::CompleteState {
            workload_states: vec![common::objects::generate_test_workload_state_with_agent(
                "Workload_1",
                "agent_A",
                ExecutionState::removed(),
            )],
            ..Default::default()
        };

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let cmd_text = cmd.get_workloads_table(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_empty_table: Vec<GetWorkloadTableDisplay> =
            vec![GetWorkloadTableDisplay::new(
                "Workload_1",
                "agent_A",
                Default::default(),
                "Removed",
                Default::default(),
            )];
        let expected_table_text = Table::new(expected_empty_table)
            .with(Style::blank())
            .to_string();

        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-provides-delete-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    #[tokio::test]
    async fn delete_workloads_two_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let startup_state = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);
        let updated_state =
            test_utils::generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            )]);

        let mut mock_client_builder = MockGRPCCommunicationClientBuilder::default();
        mock_client_builder.expect_receive_request(
            "complete_state_request",
            RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask: vec![] }),
        );
        mock_client_builder.will_send_response(
            "complete_state_request",
            ResponseContent::CompleteState(Box::new(startup_state)),
        );
        mock_client_builder.expect_receive_request(
            "update_state_request",
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: updated_state,
                update_mask: vec!["desiredState".to_string()],
            })),
        );
        mock_client_builder.will_send_response(
            "update_state_request",
            ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                added_workloads: vec![],
                deleted_workloads: vec![],
            }),
        );

        let mock_client = mock_client_builder.build();

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let delete_result = cmd
            .delete_workloads(vec!["name1".to_string(), "name2".to_string()])
            .await;
        assert!(delete_result.is_ok());
    }

    // [utest->swdd~no-delete-workloads-when-not-found~1]
    #[tokio::test]
    async fn delete_workloads_unknown_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let startup_state = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);
        let updated_state = startup_state.clone();

        let mut mock_client_builder = MockGRPCCommunicationClientBuilder::default();
        mock_client_builder.expect_receive_request(
            "complete_state_request",
            RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask: vec![] }),
        );
        mock_client_builder.will_send_response(
            "complete_state_request",
            ResponseContent::CompleteState(Box::new(startup_state)),
        );
        mock_client_builder.expect_receive_request(
            "update_state_request",
            RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                state: updated_state,
                update_mask: vec!["desiredState".to_string()],
            })),
        );
        mock_client_builder.will_send_response(
            "update_state_request",
            ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                added_workloads: vec![],
                deleted_workloads: vec![],
            }),
        );

        let mock_client = mock_client_builder.build();

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let delete_result = cmd
            .delete_workloads(vec!["unknown_workload".to_string()])
            .await;
        assert!(delete_result.is_ok());
    }

    // [utest -> swdd~cli-returns-desired-state-from-server~1]
    // [utest -> swdd~cli-shall-support-desired-state-yaml~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-get-desired-state~1]
    // [utest->swdd~cli-provides-get-desired-state~1]
    #[tokio::test]
    async fn get_state_complete_desired_state_yaml() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_state(vec![], crate::cli::OutputFormat::Yaml)
            .await
            .unwrap();
        let expected_text = serde_yaml::to_string(&test_data).unwrap();
        assert_eq!(cmd_text, expected_text);
    }

    // [utest -> swdd~cli-shall-support-desired-state-json~1]
    #[tokio::test]
    async fn get_state_complete_desired_state_json() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_state(vec![], crate::cli::OutputFormat::Json)
            .await
            .unwrap();

        let expected_text = serde_json::to_string_pretty(&test_data).unwrap();
        assert_eq!(cmd_text, expected_text);
    }

    // [utest->swdd~cli-returns-api-version-with-startup-state~1]
    #[tokio::test]
    async fn get_state_startup_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = CompleteState::default();

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_state(
                vec!["startupState".to_owned()],
                crate::cli::OutputFormat::Yaml,
            )
            .await
            .unwrap();

        let expected_single_field_result_text =
            "startupState:\n  apiVersion: v0.1\n  workloads: {}\n";

        assert_eq!(cmd_text, expected_single_field_result_text);
    }

    // [utest -> swdd~cli-returns-desired-state-from-server~1]
    // [utest->swdd~cli-returns-api-version-with-desired-state~1]
    #[tokio::test]
    async fn get_state_single_field_of_desired_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_state(
                vec!["desiredState.workloads.name3.runtime".to_owned()],
                crate::cli::OutputFormat::Yaml,
            )
            .await
            .unwrap();

        let expected_single_field_result_text =
            "desiredState:\n  workloads:\n    name3:\n      runtime: runtime\n";

        assert_eq!(cmd_text, expected_single_field_result_text);
    }

    // [utest->swdd~cli-provides-object-field-mask-arg-to-get-partial-desired-state~1]
    // [utest->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
    // [utest->swdd~cli-returns-api-version-with-desired-state~1]
    #[tokio::test]
    async fn get_state_multiple_fields_of_desired_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let cmd_text = cmd
            .get_state(
                vec![
                    "desiredState.workloads.name1.runtime".to_owned(),
                    "desiredState.workloads.name2.runtime".to_owned(),
                ],
                crate::cli::OutputFormat::Yaml,
            )
            .await
            .unwrap();
        assert!(matches!(cmd_text,
            txt if txt == *"desiredState:\n  workloads:\n    name1:\n      runtime: runtime\n    name2:\n      runtime: runtime\n" ||
            txt == *"desiredState:\n  workloads:\n    name2:\n      runtime: runtime\n    name1:\n      runtime: runtime\n"));
    }

    #[tokio::test]
    async fn get_state_single_field_without_api_version() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(Vec::new());

        let complete_state = vec![FromServer::Response(Response {
            request_id: "TestCli".to_owned(),
            response_content: ResponseContent::CompleteState(Box::new(test_data.clone())),
        })];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );
        let cmd_text = cmd
            .get_state(
                vec!["workloadStates".to_owned()],
                crate::cli::OutputFormat::Yaml,
            )
            .await
            .unwrap();

        assert_eq!(cmd_text, "workloadStates: []\n");
    }

    // [utest->swdd~cli-provides-run-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    #[tokio::test]
    async fn run_workload_one_new_workload() {
        let test_workload_name = "name4".to_string();
        let test_workload_agent = "agent_B".to_string();
        let test_workload_runtime_name = "runtime2".to_string();
        let test_workload_runtime_cfg = "some config".to_string();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let startup_state = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        // The "run workload" command shall add one new workload to the startup state.
        let new_workload = StoredWorkloadSpec {
            agent: test_workload_agent.to_owned(),
            runtime: test_workload_runtime_name.clone(),
            tags: vec![Tag {
                key: "key".to_string(),
                value: "value".to_string(),
            }],
            runtime_config: test_workload_runtime_cfg.clone(),
            ..Default::default()
        };
        let mut updated_state = startup_state.clone();
        updated_state
            .desired_state
            .workloads
            .insert(test_workload_name.clone(), new_workload);

        let mut mock_client_builder = MockGRPCCommunicationClientBuilder::default();
        mock_client_builder.expect_receive_request(
            "first_complete_state_request",
            RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask: vec![] }),
        );
        mock_client_builder.will_send_response(
            "first_complete_state_request",
            ResponseContent::CompleteState(Box::new(startup_state.clone())),
        );
        mock_client_builder.expect_receive_request(
            "update_state_request",
            RequestContent::UpdateStateRequest(Box::new(commands::UpdateStateRequest {
                state: updated_state.clone(),
                update_mask: vec!["desiredState".to_string()],
            })),
        );
        mock_client_builder.will_send_response(
            "update_state_request",
            ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                added_workloads: vec![format!("name4.abc.agent_B")],
                deleted_workloads: vec![],
            }),
        );
        mock_client_builder.expect_receive_request(
            "second_complete_state_request",
            RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask: vec![] }),
        );
        mock_client_builder.will_send_response(
            "second_complete_state_request",
            ResponseContent::CompleteState(Box::new(updated_state)),
        );
        mock_client_builder.will_send_message(FromServer::UpdateWorkloadState(
            UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name: "name4.abc.agent_B".try_into().unwrap(),
                    execution_state: ExecutionState {
                        state: objects::ExecutionStateEnum::Running(objects::RunningSubstate::Ok),
                        additional_info: "".to_string(),
                    },
                }],
            },
        ));
        let mock_client = mock_client_builder.build();

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let run_workload_result = cmd
            .run_workload(
                test_workload_name,
                test_workload_runtime_name,
                test_workload_runtime_cfg,
                test_workload_agent,
                vec![("key".to_string(), "value".to_string())],
            )
            .await;
        assert!(run_workload_result.is_ok());
    }

    #[test]
    fn utest_generate_compact_state_output_empty_filter_masks() {
        let input_state = generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "podman".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "podman".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "podman".to_string(),
            ),
        ]);

        let cli_output =
            generate_compact_state_output(&input_state, vec![], OutputFormat::Yaml).unwrap();

        // state shall remain unchanged
        assert_eq!(cli_output, serde_yaml::to_string(&input_state).unwrap());
    }

    #[test]
    fn utest_generate_compact_state_output_single_filter_mask() {
        let input_state = generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "podman".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "podman".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "podman".to_string(),
            ),
        ]);

        let expected_state = r#"{
            "desiredState": {
                "workloads": {
                  "name1": {
                    "agent": "agent_A",
                    "tags": [
                        {
                        "key": "key",
                        "value": "value"
                        }
                    ],
                    "dependencies": {
                        "workload A": "ADD_COND_RUNNING",
                        "workload C": "ADD_COND_SUCCEEDED"
                    },
                    "restartPolicy": "NEVER",
                    "runtime": "podman",
                    "runtimeConfig": "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
                    }
                }
            }
        }"#;

        let cli_output = generate_compact_state_output(
            &input_state,
            vec!["desiredState.workloads.name1".to_string()],
            OutputFormat::Yaml,
        )
        .unwrap();

        let expected_value: serde_yaml::Value = serde_yaml::from_str(expected_state).unwrap();

        assert_eq!(cli_output, serde_yaml::to_string(&expected_value).unwrap());
    }

    #[test]
    fn utest_generate_compact_state_output_multiple_filter_masks() {
        let input_state = generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "podman".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "podman".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "podman".to_string(),
            ),
        ]);

        let expected_state = r#"{
            "desiredState": {
                "workloads": {
                    "name1": {
                        "agent": "agent_A",
                        "tags": [
                            {
                            "key": "key",
                            "value": "value"
                            }
                        ],
                        "dependencies": {
                            "workload A": "ADD_COND_RUNNING",
                            "workload C": "ADD_COND_SUCCEEDED"
                        },
                        "restartPolicy": "NEVER",
                        "runtime": "podman",
                        "runtimeConfig": "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
                    },
                    "name2": {
                        "agent": "agent_B"
                    }
                }
            }
        }"#;

        let cli_output = generate_compact_state_output(
            &input_state,
            vec![
                "desiredState.workloads.name1".to_string(),
                "desiredState.workloads.name2.agent".to_string(),
            ],
            OutputFormat::Yaml,
        )
        .unwrap();

        let expected_value: serde_yaml::Value = serde_yaml::from_str(expected_state).unwrap();

        assert_eq!(cli_output, serde_yaml::to_string(&expected_value).unwrap());
    }

    #[test]
    fn utest_get_filtered_value_filter_key_with_mapping() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        let result =
            get_filtered_value(&deserialized_map, &["desiredState", "workloads", "nginx"]).unwrap();
        assert_eq!(
            result.get("restartPolicy").unwrap(),
            &serde_yaml::Value::String("ALWAYS".into())
        );
    }

    #[test]
    fn utest_get_filtered_value_filter_key_without_mapping() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        let result = get_filtered_value(
            &deserialized_map,
            &["desiredState", "workloads", "nginx", "agent"],
        )
        .unwrap();
        let expected = serde_yaml::Value::String("agent_A".to_string());
        assert_eq!(result, &expected);
    }

    #[test]
    fn utest_get_filtered_value_empty_mask() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        let result = get_filtered_value(&deserialized_map, &[]).unwrap();
        assert!(result.get("desiredState").is_some());
    }

    #[test]
    fn utest_get_filtered_value_not_existing_keys() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();

        let result = get_filtered_value(
            &deserialized_map,
            &["desiredState", "workloads", "notExistingWorkload", "nginx"],
        );
        assert!(result.is_none());

        let result = get_filtered_value(
            &deserialized_map,
            &[
                "desiredState",
                "workloads",
                "notExistingWorkload",
                "notExistingField",
            ],
        );
        assert!(result.is_none());

        let result = get_filtered_value(
            &deserialized_map,
            &[
                "desiredState",
                "workloads",
                "nginx",
                "agent",
                "notExistingField",
            ],
        );
        assert!(result.is_none());
    }

    #[test]
    fn utest_update_compact_state_create_two_keys() {
        let mut deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();

        // update by inserting two new nested keys and a new empty mapping as value
        update_compact_state(
            &mut deserialized_map,
            &[
                "desiredState",
                "workloads",
                "createThisKey",
                "createThisKey",
            ],
            serde_yaml::Value::Mapping(Default::default()),
        );

        assert!(deserialized_map
            .get("desiredState")
            .and_then(|next| next.get("workloads").and_then(|next| next
                .get("createThisKey")
                .and_then(|next| next.get("createThisKey"))))
            .is_some());
    }

    #[test]
    fn utest_update_compact_state_keep_value_of_existing_key() {
        let mut deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        // do not update value of existing key
        update_compact_state(
            &mut deserialized_map,
            &[
                "desiredState",
                "workloads",
                "nginx",
                "restartPolicy",
                "createThisKey",
            ],
            serde_yaml::Value::Mapping(Default::default()),
        );

        assert_eq!(
            deserialized_map
                .get("desiredState")
                .and_then(|next| next
                    .get("workloads")
                    .and_then(|next| next.get("nginx").and_then(|next| next.get("restartPolicy"))))
                .unwrap(),
            &serde_yaml::Value::String("ALWAYS".into())
        );
    }

    #[test]
    fn utest_update_compact_state_insert_into_empty_map() {
        // insert keys nested into empty map and add empty mapping as value
        let mut empty_map = serde_yaml::Value::Mapping(Default::default());
        update_compact_state(
            &mut empty_map,
            &["desiredState", "workloads", "nginx"],
            serde_yaml::Value::Mapping(Default::default()),
        );

        assert!(empty_map
            .get("desiredState")
            .and_then(|next| next.get("workloads").and_then(|next| next.get("nginx")))
            .is_some());
    }

    #[test]
    fn utest_update_compact_state_do_not_update_on_empty_mask() {
        let mut empty_map = serde_yaml::Value::Mapping(Default::default());
        empty_map.as_mapping_mut().unwrap().insert(
            "desiredState".into(),
            serde_yaml::Value::Mapping(Default::default()),
        );
        let expected_map = empty_map.clone();

        // do not update map if no masks are provided
        update_compact_state(
            &mut empty_map,
            &[],
            serde_yaml::Value::Mapping(Default::default()),
        );
        assert_eq!(empty_map, expected_map);
    }

    // [utest->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
    #[tokio::test]
    async fn utest_apply_args_get_input_sources_manifest_files_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let _dummy_content = io::Cursor::new(b"manifest content");
        for i in 1..3 {
            FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
                .lock()
                .unwrap()
                .push_back(Ok((
                    format!("manifest{i}.yml"),
                    Box::new(_dummy_content.clone()),
                )));
        }

        let args = ApplyArgs {
            manifest_files: vec!["manifest1.yml".to_owned(), "manifest2.yml".to_owned()],
            agent_name: None,
            delete_mode: false,
        };
        let expected = vec!["manifest1.yml".to_owned(), "manifest2.yml".to_owned()];
        let actual = args.get_input_sources().unwrap();

        let get_file_name = |item: &InputSourcePair| -> String { item.0.to_owned() };
        assert_eq!(
            expected,
            actual.iter().map(get_file_name).collect::<Vec<String>>()
        )
    }

    #[tokio::test]
    async fn utest_apply_args_get_input_sources_manifest_files_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let _dummy_content = io::Cursor::new(b"manifest content");
        FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Err(io::Error::other(
                "Some error occurred during open the manifest file!",
            )));

        let args = ApplyArgs {
            manifest_files: vec!["manifest1.yml".to_owned()],
            agent_name: None,
            delete_mode: false,
        };

        assert!(args.get_input_sources().is_err(), "Expected an error");
    }

    // [utest->swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1]
    #[test]
    fn utest_apply_args_get_input_sources_valid_manifest_stdin() {
        let args = ApplyArgs {
            manifest_files: vec!["-".to_owned()],
            agent_name: None,
            delete_mode: false,
        };
        let expected = vec!["stdin".to_owned()];
        let actual = args.get_input_sources().unwrap();

        let get_file_name = |item: &InputSourcePair| -> String { item.0.to_owned() };
        assert_eq!(
            expected,
            actual.iter().map(get_file_name).collect::<Vec<String>>()
        )
    }

    // [utest->swdd~cli-apply-supports-ankaios-manifest~1]
    #[test]
    fn utest_parse_manifest_ok() {
        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads:
        simple:
          runtime: podman
          agent: agent_A
          runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: [\"-p\", \"8081:80\"]",
        );

        assert!(parse_manifest(&mut (
            "valid_manifest_content".to_string(),
            Box::new(manifest_content)
        ))
        .is_ok());
    }

    #[test]
    fn utest_parse_manifest_invalid_manifest_content() {
        let manifest_content = io::Cursor::new(b"invalid manifest content");

        let (obj, paths) = parse_manifest(&mut (
            "invalid_manifest_content".to_string(),
            Box::new(manifest_content),
        ))
        .unwrap();

        assert!(TryInto::<State>::try_into(obj).is_err());
        assert!(paths.is_empty());
    }

    #[test]
    fn utest_update_request_obj_ok() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();
        let mut req_obj = Object::default();
        let content_value: Value = serde_yaml::from_str(
            r#"
        workloads:
         simple:
            agent: agent1
         complex:
            agent: agent1
        "#,
        )
        .unwrap();
        let cur_obj = Object::try_from(&content_value).unwrap();
        let paths = vec![
            Path::from("workloads.simple"),
            Path::from("workloads.complex"),
        ];
        let expected_obj = Object::try_from(&content_value).unwrap();
        let expected_output = generate_multiple_test_apply_manifest_table_display();

        assert!(update_request_obj(
            &mut req_obj,
            &cur_obj,
            &paths,
            "manifest_file_name",
            false,
            &mut table_output,
        )
        .is_ok());
        assert_eq!(expected_obj, req_obj);
        assert_eq!(
            expected_output,
            generate_apply_manifest_table_output(&table_output)
        );
    }

    #[test]
    fn utest_update_request_obj_failed_same_workload_names() {
        let content_value: Value = serde_yaml::from_str(
            r#"
        workloads:
         same_workload_name: {}
        "#,
        )
        .unwrap();
        let cur_obj = Object::try_from(&content_value).unwrap();

        // simulates the workload 'same_workload_name' is already there
        let mut req_obj = Object::try_from(&content_value).unwrap();

        let paths = vec![Path::from("workloads.same_workload_name")];

        assert!(update_request_obj(
            &mut req_obj,
            &cur_obj,
            &paths,
            "manifest_file_name",
            false,
            &mut Vec::<super::ApplyManifestTableDisplay>::default(),
        )
        .is_err());
    }

    #[test]
    fn utest_update_request_obj_delete_mode_on_ok() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();
        let mut req_obj = Object::default();
        let content_value: Value = serde_yaml::from_str(
            r#"
        workloads:
         simple:
            agent: agent1
         complex:
            agent: agent1
        "#,
        )
        .unwrap();
        let cur_obj = Object::try_from(&content_value).unwrap();
        let paths = vec![
            Path::from("workloads.simple"),
            Path::from("workloads.complex"),
        ];
        let expected_output =
            generate_multiple_test_apply_manifest_table_display_operation_remove();

        assert!(update_request_obj(
            &mut req_obj,
            &cur_obj,
            &paths,
            "manifest_file_name",
            true,
            &mut table_output,
        )
        .is_ok());
        assert_eq!(
            expected_output,
            generate_apply_manifest_table_output(&table_output)
        );
    }

    #[test]
    fn utest_create_filter_masks_from_paths_unique_ok() {
        let paths = vec![
            Path::from("workloads.simple"),
            Path::from("workloads.simple"),
        ];
        assert_eq!(
            vec!["currentState.workloads.simple"],
            create_filter_masks_from_paths(&paths, "currentState")
        );
    }

    #[test]
    fn utest_handle_agent_overwrite_agent_name_provided_through_agent_flag() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();

        let state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        let expected_state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "overwritten_agent_name".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        assert_eq!(
            handle_agent_overwrite(
                &vec!["workloads.wl1".into()],
                &Some("overwritten_agent_name".to_string()),
                state.try_into().unwrap(),
                &mut table_output
            )
            .unwrap(),
            expected_state
        );
    }

    #[test]
    fn utest_handle_agent_overwrite_one_agent_name_provided_in_workload_specs() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();

        let state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        assert_eq!(
            handle_agent_overwrite(
                &vec!["workloads.wl1".into()],
                &None,
                state.clone().try_into().unwrap(),
                &mut table_output
            )
            .unwrap(),
            state
        );
    }

    #[test]
    fn utest_handle_agent_overwrite_multiple_agent_names_provided_in_workload_specs() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();
        let state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "wl2".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        assert_eq!(
            handle_agent_overwrite(
                &vec!["workloads.wl1".into(), "workloads.wl2".into()],
                &None,
                state.clone().try_into().unwrap(),
                &mut table_output
            )
            .unwrap(),
            state
        );
    }

    // [utest->swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~1]
    #[test]
    fn utest_handle_agent_overwrite_no_agent_name_provided_at_all() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();
        let state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        let mut obj: Object = state.try_into().unwrap();

        obj.remove(&"workloads.wl1.agent".into()).unwrap();

        assert_eq!(
            Err("No agent name specified -> use '--agent' option to specify!".to_string()),
            handle_agent_overwrite(&vec!["workloads.wl1".into()], &None, obj, &mut table_output)
        );
        assert!(table_output.is_empty())
    }

    #[test]
    fn utest_handle_agent_overwrite_missing_agent_name() {
        let mut table_output = Vec::<super::ApplyManifestTableDisplay>::default();

        let state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        let expected_state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "overwritten_agent_name".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        let mut obj: Object = state.try_into().unwrap();

        obj.remove(&"workloads.wl1.agent".into()).unwrap();

        assert_eq!(
            handle_agent_overwrite(
                &vec!["workloads.wl1".into()],
                &Some("overwritten_agent_name".to_string()),
                obj,
                &mut table_output
            )
            .unwrap(),
            expected_state
        );
    }

    // [utest->swdd~cli-apply-generates-state-object-from-ankaios-manifests~1]
    // [utest->swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1]
    #[test]
    fn utest_generate_state_obj_and_filter_masks_from_manifests_ok() {
        let manifest_file_name = "manifest.yaml";
        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads:
        simple:
          runtime: podman
          agent: agent_A
          restartPolicy: ALWAYS
          updateStrategy: AT_MOST_ONCE
          accessRights:
            allow: []
            deny: []
          tags:
            - key: owner
              value: Ankaios team
          runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: [\"-p\", \"8081:80\"]",
        );

        let mut data = String::new();
        let _ = manifest_content.clone().read_to_string(&mut data);
        let expected_complete_state_obj = CompleteState {
            desired_state: serde_yaml::from_str(&data).unwrap(),
            ..Default::default()
        };

        let expected_filter_masks = vec!["desiredState.workloads.simple".to_string()];

        let mut manifests: Vec<InputSourcePair> =
            vec![(manifest_file_name.to_string(), Box::new(manifest_content))];

        assert_eq!(
            Ok((expected_complete_state_obj, expected_filter_masks)),
            generate_state_obj_and_filter_masks_from_manifests(
                &mut manifests[..],
                &ApplyArgs {
                    agent_name: None,
                    manifest_files: vec![manifest_file_name.to_string()],
                    delete_mode: false,
                },
                &mut Vec::<super::ApplyManifestTableDisplay>::default(),
            )
        );
    }

    // [utest->swdd~cli-apply-generates-state-object-from-ankaios-manifests~1]
    // [utest->swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1]
    #[test]
    fn utest_generate_state_obj_and_filter_masks_from_manifests_delete_mode_ok() {
        let manifest_file_name = "manifest.yaml";
        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads:
        simple:
          runtime: podman
          agent: agent_A
          runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: [\"-p\", \"8081:80\"]",
        );

        let expected_complete_state_obj = CompleteState {
            ..Default::default()
        };

        let expected_filter_masks = vec!["desiredState.workloads.simple".to_string()];

        let mut manifests: Vec<InputSourcePair> =
            vec![(manifest_file_name.to_string(), Box::new(manifest_content))];

        assert_eq!(
            Ok((expected_complete_state_obj, expected_filter_masks)),
            generate_state_obj_and_filter_masks_from_manifests(
                &mut manifests[..],
                &ApplyArgs {
                    agent_name: None,
                    manifest_files: vec![manifest_file_name.to_string()],
                    delete_mode: true,
                },
                &mut Vec::<super::ApplyManifestTableDisplay>::default(),
            )
        );
    }

    #[test]
    fn utest_generate_state_obj_and_filter_masks_from_manifests_no_workload_provided() {
        let manifest_file_name = "manifest.yaml";
        let manifest_content = io::Cursor::new(b"apiVersion: \"v0.1\"");
        let mut manifests: Vec<InputSourcePair> =
            vec![(manifest_file_name.to_string(), Box::new(manifest_content))];

        assert_eq!(
            Err("No workload provided in manifests!".to_string()),
            generate_state_obj_and_filter_masks_from_manifests(
                &mut manifests[..],
                &ApplyArgs {
                    agent_name: None,
                    manifest_files: vec![manifest_file_name.to_string()],
                    delete_mode: true,
                },
                &mut Vec::<super::ApplyManifestTableDisplay>::default(),
            )
        );
    }

    //[utest->swdd~cli-apply-send-update-state~1]
    #[tokio::test]
    async fn apply_manifests_delete_mode_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads:
    simple_manifest1:
      runtime: podman
      agent: agent_A
      runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: [\"-p\", \"8081:80\"]",
        );

        let mut manifest_data = String::new();
        let _ = manifest_content.clone().read_to_string(&mut manifest_data);

        FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(("manifest.yml".to_string(), Box::new(manifest_content))));

        let updated_state = CompleteState {
            ..Default::default()
        };

        let mut mock_client_builder = MockGRPCCommunicationClientBuilder::default();
        mock_client_builder.expect_receive_request(
            "update_state_request",
            RequestContent::UpdateStateRequest(Box::new(commands::UpdateStateRequest {
                state: updated_state.clone(),
                update_mask: vec!["desiredState.workloads.simple_manifest1".to_string()],
            })),
        );
        mock_client_builder.will_send_response(
            "update_state_request",
            ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                added_workloads: vec![],
                deleted_workloads: vec![format!("name4.abc.agent_B")],
            }),
        );

        mock_client_builder.expect_receive_request(
            "complete_state_request",
            RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask: vec![] }),
        );
        mock_client_builder.will_send_response(
            "complete_state_request",
            ResponseContent::CompleteState(Box::new(updated_state)),
        );
        mock_client_builder.will_send_message(FromServer::UpdateWorkloadState(
            UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name: "name4.abc.agent_B".try_into().unwrap(),
                    execution_state: ExecutionState {
                        state: objects::ExecutionStateEnum::Removed,
                        ..Default::default()
                    },
                }],
            },
        ));

        let mock_client = mock_client_builder.build();

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let apply_result = cmd
            .apply_manifests(ApplyArgs {
                agent_name: None,
                delete_mode: true,
                manifest_files: vec!["manifest_yaml".to_string()],
            })
            .await;
        assert!(apply_result.is_ok());
    }

    //[utest->swdd~cli-apply-send-update-state~1]
    #[tokio::test]
    async fn apply_manifests_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads:
        simple_manifest1:
          runtime: podman
          agent: agent_A
          runtimeConfig: \"\"
            ",
        );

        let mut manifest_data = String::new();
        let _ = manifest_content.clone().read_to_string(&mut manifest_data);

        FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(("manifest.yml".to_string(), Box::new(manifest_content))));

        let updated_state = CompleteState {
            desired_state: serde_yaml::from_str(&manifest_data).unwrap(),
            ..Default::default()
        };

        let mut mock_client_builder = MockGRPCCommunicationClientBuilder::default();
        mock_client_builder.expect_receive_request(
            "update_state_request",
            RequestContent::UpdateStateRequest(Box::new(commands::UpdateStateRequest {
                state: updated_state.clone(),
                update_mask: vec!["desiredState.workloads.simple_manifest1".to_string()],
            })),
        );
        mock_client_builder.will_send_response(
            "update_state_request",
            ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                added_workloads: vec!["simple_manifest1.abc.agent_B".to_string()],
                deleted_workloads: vec![],
            }),
        );
        mock_client_builder.expect_receive_request(
            "complete_state_request",
            RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask: vec![] }),
        );
        mock_client_builder.will_send_response(
            "complete_state_request",
            ResponseContent::CompleteState(Box::new(CompleteState {
                desired_state: updated_state.desired_state,
                ..Default::default()
            })),
        );
        mock_client_builder.will_send_message(FromServer::UpdateWorkloadState(
            UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name: "simple_manifest1.abc.agent_B".try_into().unwrap(),
                    execution_state: ExecutionState {
                        state: objects::ExecutionStateEnum::Running(RunningSubstate::Ok),
                        ..Default::default()
                    },
                }],
            },
        ));

        let mock_client = mock_client_builder.build();

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
            false,
        );

        let apply_result = cmd
            .apply_manifests(ApplyArgs {
                agent_name: None,
                delete_mode: false,
                manifest_files: vec!["manifest_yaml".to_string()],
            })
            .await;
        assert!(apply_result.is_ok());
    }

    #[derive(Default)]
    struct MockGRPCCommunicationClientBuilder {
        join_handle: Option<tokio::sync::oneshot::Receiver<tokio::task::JoinHandle<()>>>,
        is_ready: Option<tokio::sync::oneshot::Receiver<Receiver<ToServer>>>,
        actions: Vec<MockGRPCCommunicationClientAction>,
    }

    #[derive(Clone)]
    enum MockGRPCCommunicationClientAction {
        WillSendMessage(FromServer),
        WillSendResponse(String, ResponseContent),
        ExpectReceiveRequest(String, RequestContent),
    }

    impl MockGRPCCommunicationClientBuilder {
        pub fn build(&mut self) -> MockGRPCCommunicationsClient {
            let mut mock_client = MockGRPCCommunicationsClient::default();
            mock_client.expect_run().return_once(self.create());
            mock_client
        }

        fn create(
            &mut self,
        ) -> impl FnOnce(Receiver<ToServer>, Sender<FromServer>) -> Result<(), String> {
            let (join_handler_sender, join_handler) = tokio::sync::oneshot::channel();
            let (is_ready_sender, is_ready) = tokio::sync::oneshot::channel();
            let actions = self.actions.clone();
            self.join_handle = Some(join_handler);
            self.is_ready = Some(is_ready);
            |mut to_server: Receiver<ToServer>, from_server: Sender<FromServer>| {
                let _ = join_handler_sender.send(tokio::spawn(async move {
                    let mut request_ids = HashMap::<String, String>::new();
                    for a in actions {
                        match a {
                            MockGRPCCommunicationClientAction::WillSendMessage(message) => {
                                from_server.send(message).await.unwrap()
                            }
                            MockGRPCCommunicationClientAction::WillSendResponse(
                                request_name,
                                response,
                            ) => {
                                let request_id = request_ids.get(&request_name).unwrap();
                                from_server
                                    .send(FromServer::Response(Response {
                                        request_id: request_id.to_owned(),
                                        response_content: response,
                                    }))
                                    .await
                                    .unwrap();
                            }
                            MockGRPCCommunicationClientAction::ExpectReceiveRequest(
                                request_name,
                                expected_request,
                            ) => {
                                let actual_message = to_server.recv().await.unwrap();
                                let common::to_server_interface::ToServer::Request(actual_request) =
                                    actual_message
                                else {
                                    panic!("Expected a request")
                                };
                                request_ids.insert(request_name, actual_request.request_id);
                                assert_eq!(actual_request.request_content, expected_request);
                            }
                        }
                    }
                    is_ready_sender.send(to_server).unwrap();
                }));
                Ok(())
            }
        }

        pub fn will_send_message(&mut self, message: FromServer) {
            self.actions
                .push(MockGRPCCommunicationClientAction::WillSendMessage(message));
        }

        pub fn will_send_response(&mut self, request_name: &str, response: ResponseContent) {
            self.actions
                .push(MockGRPCCommunicationClientAction::WillSendResponse(
                    request_name.to_string(),
                    response,
                ));
        }

        pub fn expect_receive_request(&mut self, request_name: &str, request: RequestContent) {
            self.actions
                .push(MockGRPCCommunicationClientAction::ExpectReceiveRequest(
                    request_name.to_string(),
                    request,
                ));
        }
    }

    impl Drop for MockGRPCCommunicationClientBuilder {
        fn drop(&mut self) {
            let Some(join_handle) = &mut self.join_handle else {
                return;
            };

            let Ok(join_handle) = join_handle.try_recv() else {
                return;
            };

            let Some(is_ready) = &mut self.is_ready else {
                return;
            };

            let Ok(mut to_server) = is_ready.try_recv() else {
                panic!("Not all messages have been sent or received");
            };
            join_handle.abort();
            if let Ok(message) = to_server.try_recv() {
                panic!("Received unexpected message: {:#?}", message);
            }
        }
    }
}
