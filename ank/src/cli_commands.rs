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

use std::{collections::HashSet, time::Duration};
pub mod server_connection;
mod wait_list;
use tokio::time::interval;
use wait_list::WaitList;
mod get_workload_table_display;
use get_workload_table_display::GetWorkloadTableDisplay;
mod wait_list_display;

// CLI commands implemented in another files
mod apply_manifests;
mod delete_workloads;
mod get_state;
mod get_workloads;
mod set_state;

use common::{
    communications_error::CommunicationMiddlewareError,
    from_server_interface::FromServer,
    objects::{CompleteState, State, StoredWorkloadSpec, Tag, WorkloadInstanceName},
};

use wait_list_display::WaitListDisplay;

#[cfg_attr(test, mockall_double::double)]
use self::server_connection::ServerConnection;
use crate::{
    cli_commands::wait_list::ParsedUpdateStateSuccess, cli_error::CliError, output, output_debug,
};

pub struct CliCommands {
    // Left here for the future use.
    _response_timeout_ms: u64,
    no_wait: bool,
    server_connection: ServerConnection,
}

impl CliCommands {
    pub fn init(
        response_timeout_ms: u64,
        cli_name: String,
        server_url: String,
        no_wait: bool,
    ) -> Result<Self, CommunicationMiddlewareError> {
        Ok(Self {
            _response_timeout_ms: response_timeout_ms,
            no_wait,
            server_connection: ServerConnection::new(cli_name.as_str(), server_url.clone())?,
        })
    }

    pub async fn shut_down(self) {
        self.server_connection.shut_down().await
    }

    async fn get_workloads(
        &mut self,
    ) -> Result<Vec<(WorkloadInstanceName, GetWorkloadTableDisplay)>, CliError> {
        let res_complete_state = self
            .server_connection
            .get_complete_state(&Vec::new())
            .await?;

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
                .find(|&(wl_name, wl_spec)| *wl_name == wi.1.name && wl_spec.agent == wi.1.agent)
            {
                wi.1.runtime = found_wl_spec.runtime.clone();
            }
        }

        Ok(workload_infos)
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

        let update_mask = vec![format!("desiredState.workloads.{}", workload_name)];

        let mut complete_state_update = CompleteState::default();
        complete_state_update
            .desired_state
            .workloads
            .insert(workload_name, new_workload);

        output_debug!(
            "The complete state update: {:?}, update mask {:?}",
            complete_state_update,
            update_mask
        );
        self.update_state_and_wait_for_complete(complete_state_update, update_mask)
            .await
    }

    // [impl->swdd~cli-requests-update-state-with-watch~1]
    async fn update_state_and_wait_for_complete(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), CliError> {
        let update_state_success = self
            .server_connection
            .update_state(new_state, update_mask)
            .await?;

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
            self.wait_for_complete(update_state_success).await
        }
    }

    // [impl->swdd~cli-watches-workloads~1]
    async fn wait_for_complete(
        &mut self,
        update_state_success: ParsedUpdateStateSuccess,
    ) -> Result<(), CliError> {
        let mut changed_workloads =
            HashSet::from_iter(update_state_success.added_workloads.iter().cloned());
        changed_workloads.extend(update_state_success.deleted_workloads.iter().cloned());

        if changed_workloads.is_empty() {
            output!("No workloads to update");
            return Ok(());
        } else {
            output!("Successfully applied the manifest(s).\nWaiting for workload(s) to reach desired states (press Ctrl+C to interrupt).\n");
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

        let missed_workload_states = self
            .server_connection
            .take_missed_from_server_messages()
            .into_iter()
            .filter_map(|m| {
                if let FromServer::UpdateWorkloadState(u) = m {
                    Some(u)
                } else {
                    None
                }
            })
            .flat_map(|u| u.workload_states);

        wait_list.update(missed_workload_states);
        let mut spinner_interval = interval(Duration::from_millis(100));

        while !wait_list.is_empty() {
            tokio::select! {
                update_workload_state = self.server_connection.read_next_update_workload_state() => {
                    let update_workload_state = update_workload_state?;
                    output_debug!("Got update workload state: {:?}", update_workload_state);
                    wait_list.update(update_workload_state.workload_states);
                }
                _ = spinner_interval.tick() => {
                    wait_list.step_spinner();
                }
            }
        }
        Ok(())
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
        commands::{UpdateStateSuccess, UpdateWorkloadState},
        from_server_interface::{FromServer, FromServerSender},
        objects::{
            self, generate_test_workload_spec_with_param, CompleteState, ExecutionState, State,
            StoredWorkloadSpec, Tag, WorkloadState,
        },
        state_manipulation::{Object, Path},
        test_utils::{self},
        to_server_interface::ToServerReceiver,
    };
    use mockall::predicate::eq;
    use std::io;

    use super::apply_manifests::{
        create_filter_masks_from_paths, generate_state_obj_and_filter_masks_from_manifests,
        handle_agent_overwrite, parse_manifest, update_request_obj, InputSourcePair,
    };
    use crate::{cli::ApplyArgs, cli_commands::server_connection::MockServerConnection};
    use serde_yaml::Value;
    use std::io::Read;

    use super::CliCommands;

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    mockall::mock! {
        pub GRPCCommunicationsClient {
            pub fn new_cli_communication(name: String, server_address: String) -> Self;
            pub async fn run(
                &mut self,
                mut server_rx: ToServerReceiver,
                agent_tx: FromServerSender,
            ) -> Result<(), String>;
        }
    }

    // [utest->swdd~cli-provides-run-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    #[tokio::test]
    async fn utest_run_workload_one_new_workload() {
        const TEST_WORKLOAD_NAME: &str = "name4";
        let test_workload_agent = "agent_B".to_string();
        let test_workload_runtime_name = "runtime2".to_string();
        let test_workload_runtime_cfg = "some config".to_string();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

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
        let mut complete_state_update = CompleteState::default();
        complete_state_update
            .desired_state
            .workloads
            .insert(TEST_WORKLOAD_NAME.into(), new_workload);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(complete_state_update.clone()),
                eq(vec![format!(
                    "desiredState.workloads.{}",
                    TEST_WORKLOAD_NAME
                )]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![format!(
                        "{}.abc.agent_B",
                        TEST_WORKLOAD_NAME.to_string()
                    )],
                    deleted_workloads: vec![],
                })
            });
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok(Box::new(complete_state_update)));
        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(|| {
                vec![FromServer::UpdateWorkloadState(UpdateWorkloadState {
                    workload_states: vec![WorkloadState {
                        instance_name: "name4.abc.agent_B".try_into().unwrap(),
                        execution_state: ExecutionState {
                            state: objects::ExecutionStateEnum::Running(
                                objects::RunningSubstate::Ok,
                            ),
                            additional_info: "".to_string(),
                        },
                    }],
                })]
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let run_workload_result = cmd
            .run_workload(
                TEST_WORKLOAD_NAME.into(),
                test_workload_runtime_name,
                test_workload_runtime_cfg,
                test_workload_agent,
                vec![("key".to_string(), "value".to_string())],
            )
            .await;
        assert!(run_workload_result.is_ok());
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

        assert!(update_request_obj(&mut req_obj, &cur_obj, &paths,).is_ok());
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
        let cur_obj = Object::try_from(&content_value).unwrap();

        // simulates the workload 'same_workload_name' is already there
        let mut req_obj = Object::try_from(&content_value).unwrap();

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
        let cur_obj = Object::try_from(&content_value).unwrap();
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

    // [utest->swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~1]
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
            )
        );
    }
}
