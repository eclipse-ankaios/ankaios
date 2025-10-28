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

#[path = "build/internal_structs.rs"]
mod internal_structs;
use internal_structs::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = tonic_prost_build::configure()
        .build_server(true)
        .boxed("Request.RequestContent.updateStateRequest")
        .boxed("FromAnkaios.FromAnkaiosEnum.response")
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .type_attribute(".", "#[serde(rename_all = \"camelCase\")]")
        // .enum_attribute(".", "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]")
        .type_attribute(
            "ank_base.ConfigItem",
            "#[serde(into = \"serde_yaml::Value\")]",
        )
        .type_attribute(
            "ank_base.ConfigItem",
            "#[serde(try_from = \"serde_yaml::Value\")]",
        )
        .field_attribute("Workload.tags", "#[serde(flatten)]")
        .field_attribute("Workload.configs", "#[serde(flatten)]")
        .field_attribute("Workload.dependencies", "#[serde(flatten)]")
        .field_attribute("Workload.files", "#[serde(flatten)]")
        .field_attribute("WorkloadStatesMap.agentStateMap", "#[serde(flatten)]")
        .field_attribute(
            "ExecutionsStatesOfWorkload.wlNameStateMap",
            "#[serde(flatten)]",
        )
        .field_attribute("ExecutionsStatesForId.idStateMap", "#[serde(flatten)]")
        .field_attribute("ExecutionState.ExecutionStateEnum", "#[serde(flatten)]")
        .type_attribute(
            "ExecutionStateEnum",
            "#[serde(tag = \"state\", content = \"subState\")]",
        )
        .field_attribute("WorkloadMap.workloads", "#[serde(flatten)]")
        .field_attribute("AgentMap.agents", "#[serde(flatten)]")
        .field_attribute("ConfigMap.configs", "#[serde(flatten)]")
        .field_attribute(
            "ControlInterfaceAccess.allowRules",
            "#[serde(with = \"serde_yaml::with::singleton_map_recursive\")]",
        )
        .field_attribute("ControlInterfaceAccess.allowRules", "#[serde(default)]")
        .field_attribute(
            "ControlInterfaceAccess.denyRules",
            "#[serde(with = \"serde_yaml::with::singleton_map_recursive\")]",
        )
        .field_attribute("Files.files", "#[serde(default)]")
        .field_attribute(
            "Files.files",
            "#[serde(with = \"serde_yaml::with::singleton_map_recursive\")]",
        )
        .field_attribute("ControlInterfaceAccess.denyRules", "#[serde(default)]");

    builder = setup_internal_files(builder);
    builder = setup_internal_control_interface_access(builder);
    builder = setup_internal_workload(builder);
    builder = setup_internal_workload_instance_name(builder);
    builder = setup_internal_agent_map(builder);
    builder = setup_internal_configs(builder);
    builder = setup_internal_workload_state(builder);

    builder
        .compile_protos(&["proto/control_api.proto"], &["proto"])
        .unwrap();
    Ok(())
}
