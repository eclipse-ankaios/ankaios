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

#[path = "build/schema_annotations.rs"]
mod schema_annotations;
use schema_annotations::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = tonic_prost_build::configure()
        .build_server(true)
        .boxed("Request.RequestContent.updateStateRequest")
        .boxed("FromAnkaios.FromAnkaiosEnum.response")
        // Fix the enum serialization for fields/variants of the specified types as proto is generating them as integers
        .type_attribute(
            "Workload",
            "#[internal_derive_macros::fix_enum_serialization]",
        )
        .type_attribute(
            "StateRule",
            "#[internal_derive_macros::fix_enum_serialization]",
        )
        .type_attribute(
            "Dependencies",
            "#[internal_derive_macros::fix_enum_serialization]",
        )
        .type_attribute(
            "ExecutionStateEnum",
            "#[internal_derive_macros::fix_enum_serialization]",
        )
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        // TODO #313 Setup camelCase and SCREAMING_SNAKE_CASE for each object individually (if needed)
        .message_attribute(".", "#[serde(rename_all = \"camelCase\")]")
        .enum_attribute(
            "AddCondition",
            "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]",
        )
        .enum_attribute(
            "RestartPolicy",
            "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]",
        )
        .field_attribute("ReadWriteEnum.RW_NOTHING", "#[serde(rename = \"Nothing\")]")
        .field_attribute("ReadWriteEnum.RW_READ", "#[serde(rename = \"Read\")]")
        .field_attribute("ReadWriteEnum.RW_WRITE", "#[serde(rename = \"Write\")]")
        .field_attribute(
            "ReadWriteEnum.RW_READ_WRITE",
            "#[serde(rename = \"ReadWrite\")]",
        )
        .message_attribute("ank_base.ConfigItem", "#[serde(transparent)]")
        .message_attribute("ank_base.ConfigArray", "#[serde(transparent)]")
        .message_attribute("ank_base.ConfigObject", "#[serde(transparent)]")
        .enum_attribute("ConfigItemEnum", "#[serde(untagged)]")
        .enum_attribute(
            "ExecutionStateEnum",
            "#[serde(tag = \"state\", content = \"subState\")]",
        )
        .message_attribute("Tags", "#[derive(Eq)]")
        .message_attribute("AgentAttributes", "#[derive(Eq)]")
        .message_attribute("AgentMap", "#[derive(Eq)]")
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
        .field_attribute("ControlInterfaceAccess.denyRules", "#[serde(default)]")
        .field_attribute("Files.files", "#[serde(default)]")
        .field_attribute(
            "Files.files",
            "#[serde(with = \"serde_yaml::with::singleton_map_recursive\")]",
        )
        .field_attribute(
            "AgentStatus.cpu_usage",
            "#[serde(skip_serializing_if = \"::core::option::Option::is_none\")]",
        )
        .field_attribute("AgentStatus.cpu_usage", "#[serde(flatten)]")
        .field_attribute(
            "AgentStatus.free_memory",
            "#[serde(skip_serializing_if = \"::core::option::Option::is_none\")]",
        )
        .field_attribute("AgentStatus.free_memory", "#[serde(flatten)]")
        .field_attribute(
            "AgentAttributes.status",
            "#[serde(skip_serializing_if = \"::core::option::Option::is_none\")]",
        )
        .field_attribute(
            "AgentAttributes.tags",
            "#[serde(skip_serializing_if = \"::core::option::Option::is_none\")]",
        )
        .field_attribute(
            "AgentMap.agents",
            "#[serde(skip_serializing_if = \"::std::collections::HashMap::is_empty\")]",
        )
        .field_attribute(
            "AgentMap.agents",
            "#[serde(default, serialize_with = \"serialize_to_ordered_map\")]",
        );

    builder = setup_internal_files(builder);
    builder = setup_internal_control_interface_access(builder);
    builder = setup_internal_workload_instance_name(builder);
    builder = setup_internal_agent_map(builder);
    builder = setup_internal_configs(builder);
    builder = setup_internal_workload_states(builder);
    builder = setup_internal_workload(builder);
    builder = setup_internal_state(builder);

    builder = setup_schema_annotations(builder);

    builder
        .compile_protos(&["proto/control_api.proto"], &["proto"])
        .unwrap();
    Ok(())
}
