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

use crate::cli_commands::State;
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
    let state_obj_parsing_check: serde_yaml::Value = serde_yaml::from_reader(&mut manifest.1)
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
    }

    state_obj
        .try_into()
        .map_err(|err| format!("Invalid manifest data provided: {}", err))
}

pub fn update_request_obj(
    req_obj: &mut Object,
    cur_obj: &Object,
    paths: &[Path],
) -> Result<(), String> {
    for workload_path in paths.iter() {
        let workload_name = &workload_path.parts()[1];
        let cur_workload_spec = cur_obj.get(workload_path).unwrap().clone();
        if req_obj.get(workload_path).is_none() {
            let _ = req_obj.set(workload_path, cur_workload_spec.clone());
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
) -> Result<(CompleteState, Vec<String>), String> {
    let mut req_obj: Object = State::default().try_into().unwrap();
    let mut req_paths: Vec<common::state_manipulation::Path> = Vec::new();
    for manifest in manifests.iter_mut() {
        let (cur_obj, mut cur_workload_paths) = parse_manifest(manifest)?;

        update_request_obj(&mut req_obj, &cur_obj, &cur_workload_paths)?;

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
            handle_agent_overwrite(&req_paths, &apply_args.agent_name, req_obj)?;
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
