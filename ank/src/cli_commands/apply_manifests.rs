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

use super::{CliCommands, InputSourcePair};
use crate::cli_commands::State;
use crate::cli_error::CliError;
use crate::output;
use crate::{cli::ApplyArgs, output_debug};
use common::objects::{CompleteState, ALLOWED_SYMBOLS, CURRENT_API_VERSION};
use common::state_manipulation::{Object, Path};
use std::collections::HashSet;

#[cfg(test)]
use self::tests::get_input_sources_mock as get_input_sources;

#[cfg(not(test))]
use super::get_input_sources;

const WORKLOAD_LEVEL: usize = 1;

// [impl->swdd~cli-apply-supports-ankaios-manifest~1]
// [impl->swdd~cli-apply-manifest-check-for-api-version-compatibility~1]
pub fn parse_manifest(manifest: &mut InputSourcePair) -> Result<(Object, Vec<Path>), String> {
    let state_obj_parsing_check: serde_yaml::Value = serde_yaml::from_reader(&mut manifest.1)
        .map_err(|err| format!("Invalid manifest data provided: {}", err))?;
    let obj = state_obj_parsing_check.into();

    let mut workload_paths: HashSet<Path> = HashSet::new();
    let obj_paths = Vec::<Path>::from(&obj);
    for path in obj_paths {
        let parts = path.parts();
        if parts.len() > 1 {
            let _ = &mut workload_paths.insert(Path::from(format!("{}.{}", parts[0], parts[1])));
        } else if parts.contains(&"apiVersion".to_string()) {
            let manifest_api_version = obj
                .get(&path)
                .and_then(|value| value.as_str())
                .unwrap_or("Invalid manifest API version or format provided.");
            if manifest_api_version != CURRENT_API_VERSION {
                return Err(format!(
                    "Invalid manifest API version provided. Expected: '{}', got: '{}'.",
                    CURRENT_API_VERSION, manifest_api_version
                ));
            }
        }
    }

    Ok((obj, workload_paths.into_iter().collect()))
}

// [impl->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
pub fn handle_agent_overwrite(
    filter_masks: &Vec<common::state_manipulation::Path>,
    cli_specified_agent_name: &Option<String>,
    mut state_obj: Object,
) -> Result<State, String> {
    for mask_path in filter_masks {
        if mask_path.parts().starts_with(&["workloads".into()]) {
            let workload_agent_mask: Path = format!("{}.agent", String::from(mask_path)).into();
            if let Some(agent_name) = cli_specified_agent_name {
                // An agent name specified through cli -> do an agent name overwrite!
                state_obj
                    .set(
                        &workload_agent_mask,
                        serde_yaml::Value::String(agent_name.to_owned()),
                    )
                    .map_err(|_| "Could not find workload to update.".to_owned())?;
            } else if state_obj.get(&workload_agent_mask).is_none() {
                // No agent name specified through cli and inside workload configuration!
                // [impl->swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~2]
                return Err(
                    "No agent name specified -> use '--agent' option to specify!".to_owned(),
                );
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
        let workload_name = &workload_path.parts()[WORKLOAD_LEVEL];
        if !cur_obj.check_if_provided_path_exists(workload_path) {
            return Err(format!("The provided path does not exist! This may be caused by improper naming. Names expected to have characters in '{}'", ALLOWED_SYMBOLS));
        }
        let cur_workload_spec = cur_obj.get(workload_path).unwrap();
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
) -> Result<Option<(CompleteState, Vec<String>)>, String> {
    let mut req_obj: Object = State::default().try_into().unwrap();
    let mut req_paths: Vec<common::state_manipulation::Path> = Vec::new();
    for manifest in manifests.iter_mut() {
        let (cur_obj, mut cur_workload_paths) = parse_manifest(manifest)?;

        update_request_obj(&mut req_obj, &cur_obj, &cur_workload_paths)?;

        req_paths.append(&mut cur_workload_paths);
    }

    if req_paths.is_empty() {
        return Ok(None);
    }

    output_debug!("req_paths:\n{:?}\n", req_paths);
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

    Ok(Some((complete_state_req_obj, filter_masks)))
}

impl CliCommands {
    // [impl->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
    pub async fn apply_manifests(&mut self, apply_args: ApplyArgs) -> Result<(), CliError> {
        match get_input_sources(&apply_args.manifest_files) {
            Ok(mut manifests) => {
                if let Some((complete_state_req_obj, filter_masks)) =
                    generate_state_obj_and_filter_masks_from_manifests(&mut manifests, &apply_args)
                        .map_err(CliError::ExecutionError)?
                {
                    // [impl->swdd~cli-apply-send-update-state~1]
                    self.update_state_and_wait_for_complete(complete_state_req_obj, filter_masks)
                        .await
                } else {
                    output!("Nothing to update.");
                    Ok(())
                }
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
    use std::io;
    use std::io::Read;

    use api::ank_base::{self, UpdateStateSuccess};
    use mockall::predicate::eq;

    use common::{
        commands::UpdateWorkloadState,
        from_server_interface::FromServer,
        objects::{
            self, generate_test_workload_spec_with_param, CompleteState, ExecutionState,
            RunningSubstate, State, WorkloadState,
        },
        state_manipulation::{Object, Path},
        test_utils,
    };
    use serde_yaml::Value;

    use crate::{
        cli::ApplyArgs,
        cli_commands::{
            apply_manifests::{
                create_filter_masks_from_paths, generate_state_obj_and_filter_masks_from_manifests,
                handle_agent_overwrite, parse_manifest, update_request_obj,
            },
            server_connection::MockServerConnection,
            CliCommands, InputSourcePair,
        },
        filtered_complete_state::FilteredCompleteState,
    };

    mockall::lazy_static! {
        pub static ref FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST: std::sync::Mutex<std::collections::VecDeque<Result<Vec<InputSourcePair>, String>>>  =
        std::sync::Mutex::new(std::collections::VecDeque::new());
    }

    pub fn get_input_sources_mock(
        _manifest_files: &[String],
    ) -> Result<Vec<InputSourcePair>, String> {
        FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    const RESPONSE_TIMEOUT_MS: u64 = 3000;
    const OTHER_REQUEST_ID: &str = "other_request_id";

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

    // [utest->swdd~cli-apply-manifest-check-for-api-version-compatibility~1]
    #[test]
    fn utest_parse_manifest_invalid_api_version() {
        let manifest_content = io::Cursor::new(b"apiVersion: v3");

        assert!(parse_manifest(&mut (
            "invalid_api_version".to_string(),
            Box::new(manifest_content),
        ))
        .is_err());
    }

    #[test]
    fn utest_update_request_obj_ok() {
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
        let expected_obj: Object = content_value.into();
        let cur_obj = expected_obj.clone();
        let paths = vec![
            Path::from("workloads.simple"),
            Path::from("workloads.complex"),
        ];

        assert!(update_request_obj(&mut req_obj, &cur_obj, &paths).is_ok());
        assert_eq!(expected_obj, req_obj);
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
        let cur_obj: Object = content_value.into();

        // simulates the workload 'same_workload_name' is already there
        let mut req_obj = cur_obj.clone();

        let paths = vec![Path::from("workloads.same_workload_name")];

        assert!(update_request_obj(&mut req_obj, &cur_obj, &paths,).is_err());
    }

    #[test]
    fn utest_update_request_obj_delete_mode_on_ok() {
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
        let cur_obj = content_value.into();
        let paths = vec![
            Path::from("workloads.simple"),
            Path::from("workloads.complex"),
        ];

        assert!(update_request_obj(&mut req_obj, &cur_obj, &paths).is_ok());
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

    // [utest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    #[test]
    fn utest_handle_agent_overwrite_agent_name_provided_through_agent_flag() {
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
            )
            .unwrap(),
            expected_state
        );
    }

    // [utest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    #[test]
    fn utest_handle_agent_overwrite_one_agent_name_provided_in_workload_specs() {
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
            )
            .unwrap(),
            state
        );
    }

    // [utest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    #[test]
    fn utest_handle_agent_overwrite_multiple_agent_names_provided_in_workload_specs() {
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
            )
            .unwrap(),
            state
        );
    }

    // [utest->swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~2]
    // [utest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    #[test]
    fn utest_handle_agent_overwrite_no_agent_name_provided_at_all() {
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
            handle_agent_overwrite(&vec!["workloads.wl1".into()], &None, obj)
        );
    }

    // [utest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    #[test]
    fn utest_handle_agent_overwrite_missing_agent_name() {
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
            )
            .unwrap(),
            expected_state
        );
    }

    // [utest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
    #[test]
    fn utest_handle_agent_overwrite_considers_only_workloads() {
        let state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        let expected_state = test_utils::generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "wl1".to_string(),
                "runtime_X".to_string(),
            ),
        ]);

        let cli_specified_agent_name = None;

        assert_eq!(
            handle_agent_overwrite(
                &vec!["workloads.wl1".into(), "configs.config_key".into()],
                &cli_specified_agent_name,
                state.try_into().unwrap(),
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
            Ok(Some((expected_complete_state_obj, expected_filter_masks))),
            generate_state_obj_and_filter_masks_from_manifests(
                &mut manifests[..],
                &ApplyArgs {
                    agent_name: None,
                    manifest_files: vec![manifest_file_name.to_string()],
                    delete_mode: false,
                },
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
            Ok(Some((expected_complete_state_obj, expected_filter_masks))),
            generate_state_obj_and_filter_masks_from_manifests(
                &mut manifests[..],
                &ApplyArgs {
                    agent_name: None,
                    manifest_files: vec![manifest_file_name.to_string()],
                    delete_mode: true,
                },
            )
        );
    }

    //[utest->swdd~cli-apply-send-update-state~1]
    // [utest->swdd~cli-watches-workloads-on-updates~1]
    #[tokio::test]
    async fn utest_apply_manifests_delete_mode_ok() {
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

        let updated_state = CompleteState {
            ..Default::default()
        };

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(updated_state.clone()),
                eq(vec!["desiredState.workloads.simple_manifest1".to_string()]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec!["name4.abc.agent_B".to_string()],
                })
            });
        let updated_state_clone = updated_state.clone();
        mock_server_connection
            .expect_get_complete_state()
            .times(2)
            .with(eq(vec![]))
            .returning(move |_| {
                Ok((ank_base::CompleteState::from(updated_state_clone.clone())).into())
            });
        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(std::vec::Vec::new);
        mock_server_connection
            .expect_read_next_update_workload_state()
            .return_once(|| {
                Ok(UpdateWorkloadState {
                    workload_states: vec![WorkloadState {
                        instance_name: "name4.abc.agent_B".try_into().unwrap(),
                        execution_state: ExecutionState {
                            state: objects::ExecutionStateEnum::Removed,
                            ..Default::default()
                        },
                    }],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(vec![(
                "manifest.yml".to_string(),
                Box::new(manifest_content),
            )]));

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
    // [utest->swdd~cli-watches-workloads-on-updates~1]
    #[tokio::test]
    async fn utest_apply_manifests_workloads_updated_ok() {
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

        let updated_state = CompleteState {
            desired_state: serde_yaml::from_str(&manifest_data).unwrap(),
            ..Default::default()
        };

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(updated_state.clone()),
                eq(vec!["desiredState.workloads.simple_manifest1".to_string()]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec!["simple_manifest1.abc.agent_B".to_string()],
                    deleted_workloads: vec![],
                })
            });
        mock_server_connection
            .expect_get_complete_state()
            .once()
            .returning(|_| Ok(FilteredCompleteState::default()));

        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                Ok((ank_base::CompleteState::from(CompleteState {
                    desired_state: updated_state.desired_state,
                    ..Default::default()
                }))
                .into())
            });
        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(|| {
                vec![
                    FromServer::Response(ank_base::Response {
                        request_id: OTHER_REQUEST_ID.into(),
                        response_content: Some(ank_base::response::ResponseContent::Error(
                            Default::default(),
                        )),
                    }),
                    FromServer::UpdateWorkloadState(UpdateWorkloadState {
                        workload_states: vec![WorkloadState {
                            instance_name: "simple_manifest1.abc.agent_B".try_into().unwrap(),
                            execution_state: ExecutionState {
                                state: objects::ExecutionStateEnum::Running(RunningSubstate::Ok),
                                ..Default::default()
                            },
                        }],
                    }),
                ]
            });
        mock_server_connection
            .expect_read_next_update_workload_state()
            .return_once(|| {
                Ok(UpdateWorkloadState {
                    workload_states: vec![WorkloadState {
                        instance_name: "simple_manifest1.abc.agent_B".try_into().unwrap(),
                        execution_state: ExecutionState {
                            state: objects::ExecutionStateEnum::Running(RunningSubstate::Ok),
                            ..Default::default()
                        },
                    }],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(vec![(
                "manifest.yml".to_string(),
                Box::new(manifest_content),
            )]));

        let apply_result = cmd
            .apply_manifests(ApplyArgs {
                agent_name: None,
                delete_mode: false,
                manifest_files: vec!["manifest_yaml".to_string()],
            })
            .await;
        assert!(apply_result.is_ok());
    }

    // [utest->swdd~cli-apply-generates-state-object-from-ankaios-manifests~1]
    // [utest->swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1]
    // [utest->swdd~cli-apply-send-update-state~1]
    #[tokio::test]
    async fn utest_apply_manifests_only_configs_to_update_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads: {}\nconfigs:\n  config_1: config_value_1",
        );

        let mut manifest_data = String::new();
        let _ = manifest_content.clone().read_to_string(&mut manifest_data);

        let updated_state = CompleteState {
            desired_state: serde_yaml::from_str(&manifest_data).unwrap(),
            ..Default::default()
        };

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(updated_state.clone()),
                eq(vec!["desiredState.configs.config_1".to_string()]),
            )
            .return_once(|_, _| Ok(UpdateStateSuccess::default()));

        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| Ok(FilteredCompleteState::default()));

        mock_server_connection
            .expect_take_missed_from_server_messages()
            .never();

        mock_server_connection
            .expect_read_next_update_workload_state()
            .never();

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: true,
            server_connection: mock_server_connection,
        };

        FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(vec![(
                "manifest.yml".to_string(),
                Box::new(manifest_content),
            )]));

        let apply_result = cmd
            .apply_manifests(ApplyArgs {
                agent_name: None,
                delete_mode: false,
                manifest_files: vec!["manifest_yaml".to_string()],
            })
            .await;
        assert!(apply_result.is_ok());
    }

    // [utest->swdd~cli-apply-generates-state-object-from-ankaios-manifests~1]
    // [utest->swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1]
    #[tokio::test]
    async fn utest_apply_manifests_nothing_to_update_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let manifest_content = io::Cursor::new(b"apiVersion: \"v0.1\"");

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection.expect_update_state().never();

        mock_server_connection.expect_get_complete_state().never();

        mock_server_connection
            .expect_take_missed_from_server_messages()
            .never();

        mock_server_connection
            .expect_read_next_update_workload_state()
            .never();

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(vec![(
                "manifest.yml".to_string(),
                Box::new(manifest_content),
            )]));

        let apply_result = cmd
            .apply_manifests(ApplyArgs {
                agent_name: None,
                delete_mode: false,
                manifest_files: vec!["manifest_yaml".to_string()],
            })
            .await;
        assert!(apply_result.is_ok());
    }

    #[tokio::test]
    async fn utest_apply_manifest_invalid_names() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let manifest_content = io::Cursor::new(
            b"apiVersion: \"v0.1\"\nworkloads:
            simple.manifest1:
              runtime: podman
              agent: agent_A
              runtimeConfig: \"\"
                ",
        );

        let mut manifest_data = String::new();
        let _ = manifest_content.clone().read_to_string(&mut manifest_data);

        let updated_state = CompleteState {
            desired_state: serde_yaml::from_str(&manifest_data).unwrap(),
            ..Default::default()
        };

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(updated_state.clone()),
                eq(vec!["desiredState.workloads.simple.manifest1".to_string()]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec!["simple_manifest1.abc.agent_B".to_string()],
                    deleted_workloads: vec![],
                })
            });
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                Ok((ank_base::CompleteState::from(CompleteState {
                    desired_state: updated_state.desired_state,
                    ..Default::default()
                }))
                .into())
            });
        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(|| {
                vec![
                    FromServer::Response(ank_base::Response {
                        request_id: OTHER_REQUEST_ID.into(),
                        response_content: Some(ank_base::response::ResponseContent::Error(
                            Default::default(),
                        )),
                    }),
                    FromServer::UpdateWorkloadState(UpdateWorkloadState {
                        workload_states: vec![WorkloadState {
                            instance_name: "simple_manifest1.abc.agent_B".try_into().unwrap(),
                            execution_state: ExecutionState {
                                state: objects::ExecutionStateEnum::Running(RunningSubstate::Ok),
                                ..Default::default()
                            },
                        }],
                    }),
                ]
            });
        mock_server_connection
            .expect_read_next_update_workload_state()
            .return_once(|| {
                Ok(UpdateWorkloadState {
                    workload_states: vec![WorkloadState {
                        instance_name: "simple_manifest1.abc.agent_B".try_into().unwrap(),
                        execution_state: ExecutionState {
                            state: objects::ExecutionStateEnum::Running(RunningSubstate::Ok),
                            ..Default::default()
                        },
                    }],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        FAKE_GET_INPUT_SOURCE_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Ok(vec![(
                "manifest.yml".to_string(),
                Box::new(manifest_content),
            )]));

        let apply_result = cmd
            .apply_manifests(ApplyArgs {
                agent_name: None,
                delete_mode: false,
                manifest_files: vec!["manifest_yaml".to_string()],
            })
            .await;
        assert!(apply_result.is_err());
    }
}
