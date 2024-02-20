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

use common::objects::{self as ankaios, WorkloadInstanceName};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredState {
    pub workloads: HashMap<String, StoredWorkloadSpec>,
    #[serde(default)]
    pub configs: HashMap<String, String>,
    #[serde(default)]
    pub cron_jobs: HashMap<String, ankaios::Cronjob>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredWorkloadSpec {
    pub runtime: String,
    pub agent: String,
    pub restart: bool,
    #[serde(default)]
    pub dependencies: HashMap<String, ankaios::AddCondition>,
    pub update_strategy: ankaios::UpdateStrategy,
    pub access_rights: ankaios::AccessRights,
    #[serde(default)]
    pub tags: Vec<ankaios::Tag>,
    pub runtime_config: String,
}

// [impl->swdd~stored-workload-spec-parses-yaml~1]
pub fn parse(state_yaml: String) -> Result<ankaios::State, Box<dyn std::error::Error>> {
    let stored_state = serde_yaml::from_str(state_yaml.as_str())?;
    Ok(from_stored_state(stored_state))
}

fn from_stored_state(stored_state: StoredState) -> ankaios::State {
    ankaios::State {
        workloads: from_stored_workloads(stored_state.workloads),
        configs: stored_state.configs,
        cron_jobs: stored_state.cron_jobs,
    }
}

fn from_stored_workloads(
    stored_workloads: HashMap<String, StoredWorkloadSpec>,
) -> HashMap<String, ankaios::WorkloadSpec> {
    let mut workload_specs: HashMap<String, ankaios::WorkloadSpec> = HashMap::new();
    for (name, stored_workload) in stored_workloads {
        let workload = ankaios::WorkloadSpec {
            instance_name: WorkloadInstanceName::builder()
                .workload_name(name)
                .agent_name(stored_workload.agent)
                .config(&stored_workload.runtime_config)
                .build(),
            tags: stored_workload.tags,
            runtime_config: stored_workload.runtime_config,
            runtime: stored_workload.runtime,
            dependencies: stored_workload.dependencies,
            update_strategy: stored_workload.update_strategy,
            restart: stored_workload.restart,
            access_rights: stored_workload.access_rights,
        };
        workload_specs.insert(name, workload);
    }
    workload_specs
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
    use super::*;
    use common::objects::{Tag, UpdateStrategy};
    // [utest->swdd~stored-workload-spec-parses-yaml~1]
    #[test]
    fn utest_reads_start_config() {
        let data = "workloads:
          nginx:
            runtime: podman
            agent: agent_A
            restart: true
            updateStrategy: AT_MOST_ONCE
            accessRights:
              allow: []
              deny: []
            tags:
            - key: owner
              value: Ankaios team 1
            runtimeConfig: |
              image: docker.io/nginx:latest
              commandOptions: [\"-p\", \"8081:80\"]
          hello:
            runtime: podman
            agent: agent_B
            restart: false
            updateStrategy: AT_LEAST_ONCE
            accessRights:
              allow: []
              deny: []
            runtimeConfig: |
              image: alpine:latest
              commandArgs: [ \"echo\", \"Hello Ankaios\"]
        "
        .to_string();

        let state =
            parse(data).unwrap_or_else(|error| panic!("Parsing failed with error {}", error));

        assert_eq!(state.workloads.len(), 2);
        assert_eq!(state.cron_jobs.len(), 0);
        assert_eq!(state.configs.len(), 0);

        // asserts workload nginx
        assert!(state.workloads.contains_key("nginx"));
        let workload_spec_nginx = state.workloads.get("nginx").unwrap();
        assert_eq!(workload_spec_nginx.runtime, "podman");
        assert_eq!(workload_spec_nginx.agent, "agent_A");
        assert_eq!(workload_spec_nginx.name, "nginx");
        assert!(workload_spec_nginx.restart);
        assert_eq!(
            workload_spec_nginx.update_strategy,
            UpdateStrategy::AtMostOnce
        );
        assert_eq!(workload_spec_nginx.access_rights.allow.len(), 0);
        assert_eq!(workload_spec_nginx.access_rights.deny.len(), 0);
        assert_eq!(workload_spec_nginx.tags.len(), 1);
        assert_eq!(
            workload_spec_nginx.tags[0],
            Tag {
                key: "owner".to_owned(),
                value: "Ankaios team 1".to_owned(),
            }
        );
        assert_eq!(
            workload_spec_nginx.runtime_config,
            "image: docker.io/nginx:latest\ncommandOptions: [\"-p\", \"8081:80\"]\n"
        );

        // asserts workload hello
        assert!(state.workloads.contains_key("hello"));
        let workload_spec_hello = state.workloads.get("hello").unwrap();
        assert_eq!(workload_spec_hello.runtime, "podman");
        assert_eq!(workload_spec_hello.agent, "agent_B");
        assert_eq!(workload_spec_hello.name, "hello");
        assert!(!workload_spec_hello.restart);
        assert_eq!(
            workload_spec_hello.update_strategy,
            UpdateStrategy::AtLeastOnce
        );
        assert_eq!(workload_spec_hello.access_rights.allow.len(), 0);
        assert_eq!(workload_spec_hello.access_rights.deny.len(), 0);
        assert_eq!(workload_spec_hello.tags.len(), 0);

        assert_eq!(
            workload_spec_hello.runtime_config,
            "image: alpine:latest\ncommandArgs: [ \"echo\", \"Hello Ankaios\"]\n"
        );
    }

    #[test]
    fn utest_reports_error_on_missing_workloads() {
        use std::str::FromStr;
        let input = String::from_str("cronJob:\n  someJob: xxx").unwrap();
        let result = parse(input);

        result.expect_err("Missing workloads must result in error.");
    }
}
