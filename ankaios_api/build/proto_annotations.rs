// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use tonic_prost_build::Builder;

/// This function provides the proto objects with annotations required for:
/// - serde serialization/deserialization
/// - changes to the object structure for better usability
pub fn setup_proto_annotations(mut builder: Builder) -> Builder {
    builder = annotate_general(builder);
    builder = annotate_complete_state(builder);
    builder = annotate_agent(builder);
    builder = annotate_configs(builder);
    builder = annotate_workload(builder);
    builder
}

fn annotate_general(builder: Builder) -> Builder {
    builder
        .boxed("Request.RequestContent.updateStateRequest")
        .boxed("FromAnkaios.FromAnkaiosEnum.response")
        .boxed("Response.ResponseContent.completeStateResponse")
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .message_attribute(".", "#[serde(rename_all = \"camelCase\")]")
}

fn annotate_complete_state(builder: Builder) -> Builder {
    builder
        // Complete State
        .field_attribute(
            "CompleteState.desiredState",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "CompleteState.workloadStates",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "CompleteState.agents",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        // State
        .field_attribute(
            "State.workloads",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "State.configs",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        // Execution states
        .enum_attribute(
            "ExecutionStateEnum",
            "#[serde(tag = \"state\", content = \"subState\")]",
        )
        .field_attribute(
            "ExecutionsStatesOfWorkload.wlNameStateMap",
            "#[serde(flatten)]",
        )
        .field_attribute("ExecutionsStatesForId.idStateMap", "#[serde(flatten)]")
        .field_attribute(
            "ExecutionState.additionalInfo",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute("ExecutionState.ExecutionStateEnum", "#[serde(flatten)]")
        .field_attribute("WorkloadStatesMap.agentStateMap", "#[serde(flatten)]")
        // AlteredFields
        .field_attribute(
            "AlteredFields.addedFields",
            "#[serde(skip_serializing_if = \"Vec::is_empty\")]",
        )
        .field_attribute(
            "AlteredFields.updatedFields",
            "#[serde(skip_serializing_if = \"Vec::is_empty\")]",
        )
        .field_attribute(
            "AlteredFields.removedFields",
            "#[serde(skip_serializing_if = \"Vec::is_empty\")]",
        )
}

fn annotate_workload(builder: Builder) -> Builder {
    builder
        // Skip serializing fields
        .field_attribute(
            "Workload.agent",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "Workload.restartPolicy",
            "#[serde(default, skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "Workload.runtime",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "Workload.runtimeConfig",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "Workload.files",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        .field_attribute(
            "Workload.controlInterfaceAccess",
            "#[serde(skip_serializing_if = \"Option::is_none\")]",
        )
        // Flatten fields
        .field_attribute("Workload.tags", "#[serde(flatten)]")
        .field_attribute("Workload.configs", "#[serde(flatten)]")
        .field_attribute("Workload.dependencies", "#[serde(flatten)]")
        .field_attribute("Workload.files", "#[serde(flatten)]")
        // Workload objects
        .enum_attribute(
            "RestartPolicy",
            "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]",
        )
        .message_attribute("Tags", "#[derive(Eq)]")
        .field_attribute(
            "Files.files",
            "#[serde(default, skip_serializing_if = \"Vec::is_empty\")]",
        )
        .field_attribute(
            "Files.files",
            // Yes, this is not a map, but this is the only way to get the desired serialization behavior without ! in the YAML and a custom serializer
            "#[serde(with = \"serde_yaml::with::singleton_map_recursive\")]",
        )
        // Control Interface Access
        .field_attribute(
            "ControlInterfaceAccess.allowRules",
            "#[serde(default, with = \"serde_yaml::with::singleton_map_recursive\", skip_serializing_if = \"Vec::is_empty\")]",
        )
        .field_attribute(
            "ControlInterfaceAccess.denyRules",
            "#[serde(default, with = \"serde_yaml::with::singleton_map_recursive\", skip_serializing_if = \"Vec::is_empty\")]",
        )
        .enum_attribute(
            "AddCondition",
            "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]",
        )
        .field_attribute("ReadWriteEnum.RW_NOTHING", "#[serde(rename = \"Nothing\")]")
        .field_attribute("ReadWriteEnum.RW_READ", "#[serde(rename = \"Read\")]")
        .field_attribute("ReadWriteEnum.RW_WRITE", "#[serde(rename = \"Write\")]")
        .field_attribute(
            "ReadWriteEnum.RW_READ_WRITE",
            "#[serde(rename = \"ReadWrite\")]",
        )
        // Workload Map
        .field_attribute("WorkloadMap.workloads", "#[serde(flatten)]")
}

fn annotate_agent(builder: Builder) -> Builder {
    builder
        .message_attribute("AgentAttributes", "#[derive(Eq)]")
        .message_attribute("AgentMap", "#[derive(Eq)]")
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
        .field_attribute("AgentMap.agents", "#[serde(flatten)]")
        .field_attribute(
            "AgentMap.agents",
            "#[serde(skip_serializing_if = \"::std::collections::HashMap::is_empty\")]",
        )
        .field_attribute(
            "AgentMap.agents",
            "#[serde(default, serialize_with = \"serialize_to_ordered_map\")]",
        )
}

fn annotate_configs(builder: Builder) -> Builder {
    builder
        .field_attribute("ConfigMap.configs", "#[serde(flatten)]")
        .message_attribute("ank_base.ConfigItem", "#[serde(transparent)]")
        .message_attribute("ank_base.ConfigArray", "#[serde(transparent)]")
        .message_attribute("ank_base.ConfigObject", "#[serde(transparent)]")
        .enum_attribute("ConfigItemEnum", "#[serde(untagged)]")
}
