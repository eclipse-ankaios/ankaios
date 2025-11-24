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

/// This function is used to create and configure the following structs:
/// - CompleteStateSpec
/// - StateSpec
/// - WorkloadMapSpec
pub fn setup_spec_state(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "CompleteState",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "CompleteState",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute(
            "CompleteState",
            "#[spec_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .field_attribute("CompleteState.desiredState", "#[spec_mandatory]")
        .field_attribute("CompleteState.desiredState", "#[spec_field_attr(#[serde(default)])]")
        .field_attribute("CompleteState.workloadStates", "#[spec_default]")
        .field_attribute("CompleteState.workloadStates", "#[spec_field_attr(#[serde(default)])]")
        .field_attribute("CompleteState.agents", "#[spec_default]")
        .field_attribute("CompleteState.agents", "#[spec_field_attr(#[serde(default)])]")

        .message_attribute(
            "State",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "State",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        ).field_attribute(
            "State.apiVersion",
            "#[spec_field_attr(#[serde(rename = \"apiVersion\")])]",
        )
        .field_attribute("State.workloads","#[spec_mandatory]")
        .field_attribute("State.workloads", "#[spec_field_attr(#[serde(default)])]")
        .field_attribute("State.configs","#[spec_default]")
        .field_attribute("State.configs", "#[spec_field_attr(#[serde(default)])]")

        .message_attribute("WorkloadMap", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "WorkloadMap",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute("WorkloadMap", "#[spec_type_attr(#[serde(transparent)])]")
        .field_attribute("WorkloadMap.workloads", "#[spec_field_attr(#[serde(serialize_with = \"serialize_to_ordered_map\")])]")
}

/// This function is used to create and configure the following structs:
/// - FilesSpec
/// - FileSpec
/// - FileContentSpec
pub fn setup_spec_files(builder: Builder) -> Builder {
    builder
        .message_attribute("Files", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "Files",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .field_attribute("Files.files", "#[spec_field_attr(#[serde(default)])]")
        .message_attribute("File", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "File",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .message_attribute(
            "File",
            "#[spec_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .enum_attribute("FileContent", "#[derive(spec_macros::Spec)]")
        .enum_attribute(
            "FileContent",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .enum_attribute("FileContent", "#[spec_type_attr(#[serde(untagged)])]")
        .field_attribute("File.FileContent", "#[spec_mandatory]")
        .field_attribute(
            "File.FileContent",
            "#[spec_field_attr(#[serde(flatten)])]",
        )
        .field_attribute("File.FileContent.data", "#[spec_enum_named]")
        .field_attribute(
            "File.FileContent.data",
            "#[spec_field_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .field_attribute("File.FileContent.binaryData", "#[spec_enum_named]")
        .field_attribute(
            "File.FileContent.binaryData",
            "#[spec_field_attr(#[serde(rename_all = \"camelCase\")])]",
        )
}

/// This function is used to create and configure the following structs:
/// - WorkloadSpec
/// - DependenciesSpec
/// - TagsSpec
pub fn setup_spec_workload(builder: Builder) -> Builder {
    builder
        .message_attribute("Workload", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "Workload",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute(
            "Workload",
            "#[spec_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )

        .field_attribute("Workload.agent", "#[spec_mandatory]")
        .field_attribute("Workload.restartPolicy", "#[spec_default]")
        .field_attribute("Workload.dependencies", "#[spec_default]")
        .field_attribute("Workload.tags", "#[spec_default]")
        .field_attribute("Workload.runtime", "#[spec_mandatory]")
        .field_attribute("Workload.runtimeConfig", "#[spec_mandatory]")
        .field_attribute("Workload.controlInterfaceAccess", "#[spec_default]")
        .field_attribute("Workload.configs", "#[spec_default]")
        .field_attribute("Workload.files", "#[spec_default]")

        .field_attribute("Workload.dependencies", "#[spec_field_attr(#[serde(flatten)])]")
        .field_attribute("Workload.tags", "#[spec_field_attr(#[serde(flatten)])]")
        .field_attribute("Workload.configs", "#[spec_field_attr(#[serde(flatten)])]")
        .field_attribute("Workload.files", "#[spec_field_attr(#[serde(flatten)])]")

        .field_attribute("Workload.restartPolicy", "#[spec_field_attr(#[serde(default)])]")
        .field_attribute("Workload.controlInterfaceAccess", "#[spec_field_attr(#[serde(default)])]")

        .message_attribute(
            "Dependencies",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "Dependencies",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .field_attribute("Dependencies.dependencies", "#[spec_field_attr(#[serde(default, serialize_with = \"serialize_to_ordered_map\")])]")

        .message_attribute("Tags", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "Tags",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]",
        )
        // The custom deserializer is added to support the deprecated apiVersions: v0.1
        .field_attribute("Tags.tags", "#[spec_field_attr(#[serde(default, serialize_with = \"serialize_to_ordered_map\", deserialize_with = \"tag_adapter_deserializer\")])]")
}

/// This function is used to create and configure the following structs:
/// - WorkloadInstanceNameSpec
pub fn setup_spec_workload_instance_name(builder: Builder) -> Builder {
    builder.message_attribute(
        "WorkloadInstanceName",
        "#[derive(spec_macros::Spec)]",
    )
    .message_attribute(
        "WorkloadInstanceName",
        "#[spec_derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]",
    )
    .message_attribute(
        "WorkloadInstanceName",
        "#[spec_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    )
}

/// This function is used to create and configure the following structs:
/// - ControlInterfaceAccessSpec
/// - AccessRightsRuleSpec
/// - AccessRightsRuleEnumSpec
/// - StateRuleSpec
/// - LogRuleSpec
pub fn setup_spec_control_interface_access(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "ControlInterfaceAccess",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[spec_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .field_attribute(
            "ControlInterfaceAccess.allowRules",
            "#[spec_field_attr(#[serde(default)])]",
        )
        .field_attribute(
            "ControlInterfaceAccess.denyRules",
            "#[spec_field_attr(#[serde(default)])]",
        )

        .message_attribute(
            "AccessRightsRule",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "AccessRightsRule",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .field_attribute("AccessRightsRule.AccessRightsRuleEnum", "#[serde(flatten)]")
        .field_attribute("AccessRightsRule.AccessRightsRuleEnum", "#[spec_field_attr(#[serde(flatten)])]")
        .field_attribute(
            "AccessRightsRule.AccessRightsRuleEnum",
            "#[spec_mandatory]",
        )
        .enum_attribute(
            "AccessRightsRuleEnum",
            "#[derive(spec_macros::Spec)]",
        )
        .enum_attribute(
            "AccessRightsRule.AccessRightsRuleEnum",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]")
        .enum_attribute("AccessRightsRule.AccessRightsRuleEnum", "#[spec_type_attr(#[serde(tag = \"type\")])]")
        .enum_attribute(
            "AccessRightsRule.AccessRightsRuleEnum",
            "#[serde(tag = \"type\")]",
        )

        .message_attribute("StateRule", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "StateRule",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .message_attribute(
            "StateRule",
            "#[spec_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        // The alias to filterMask is added to support the deprecated apiVersions: v0.1
        .field_attribute("StateRule.filterMasks", "#[spec_field_attr(#[serde(alias = \"filterMask\")])]")

        .message_attribute("LogRule", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "LogRule",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .message_attribute(
            "LogRule",
            "#[spec_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
}

/// This function is used to create and configure the following structs:
/// - ConfigMappingsSpec
/// - ConfigMapSpec
/// - ConfigItemSpec
/// - ConfigItemEnumSpec
/// - ConfigObjectSpec
/// - ConfigArraySpec
pub fn setup_spec_configs(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "ConfigMappings",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "ConfigMappings",
            "#[spec_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]",
        )
        .field_attribute("ConfigMappings.configs", "#[spec_field_attr(#[serde(default, serialize_with = \"serialize_to_ordered_map\")])]")

        .message_attribute("ConfigMap", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "ConfigMap",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute("ConfigMap", "#[spec_type_attr(#[serde(transparent)])]")
        // .field_attribute("ConfigMap.configs", "#[spec_field_attr(#[serde(flatten)])]")
        .message_attribute("ConfigItem", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "ConfigItem",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )

        .message_attribute("ConfigItem", "#[spec_type_attr(#[serde(transparent)])]")

        .field_attribute("ConfigItem.ConfigItemEnum", "#[spec_mandatory]")
        .field_attribute("ConfigItem.ConfigItemEnum", "#[spec_field_attr(#[serde(flatten)])]")
        .enum_attribute("ConfigItemEnum", "#[derive(spec_macros::Spec)]")
        .enum_attribute(
            "ConfigItemEnum",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[spec_type_attr(#[serde(untagged)])]",
        )
        .message_attribute("ConfigArray", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "ConfigArray",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .message_attribute(
            "ConfigArray",
            "#[spec_type_attr(#[serde(transparent)])]",
        )
        .message_attribute("ConfigObject", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "ConfigObject",
            "#[spec_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .field_attribute("ConfigObject.fields", "#[spec_field_attr(#[serde(flatten)])]")
}

/// This function is used to create and configure the following structs:
/// - AgentMapSpec
/// - AgentAttributesSpec
/// - AgentStatusSpec
/// - CpuUsageSpec
/// - FreeMemorySpec
pub fn setup_spec_agent_map(builder: Builder) -> Builder {
    builder
        .message_attribute("AgentMap", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "AgentMap",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .message_attribute(
            "AgentAttributes",
            "#[derive(spec_macros::Spec)]",
        )
        .message_attribute(
            "AgentAttributes",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .message_attribute("AgentStatus", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "AgentStatus",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .field_attribute("AgentStatus.cpu_usage", "#[spec_field_attr(#[serde(flatten)])]")
        .field_attribute("AgentStatus.free_memory", "#[spec_field_attr(#[serde(flatten)])]")

        .message_attribute("CpuUsage", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "CpuUsage",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .message_attribute("FreeMemory", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "FreeMemory",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
}

/// This function is used to create and configure the following structs:
/// - WorkloadStateSpec
/// - WorkloadStatesMapSpec
/// - ExecutionsStatesOfWorkloadSpec
/// - ExecutionsStatesForIdSpec
/// - ExecutionStateSpec
/// - ExecutionStateEnumSpec
pub fn setup_spec_workload_states(builder: Builder) -> Builder {
    builder
    .message_attribute(
        "WorkloadState",
        "#[derive(spec_macros::Spec)]",
    )
    .message_attribute(
        "WorkloadState",
        "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .message_attribute(
        "WorkloadState",
        "#[spec_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    ).message_attribute(
        "WorkloadState",
        "#[spec_type_attr(#[serde(rename = \"workloadState\")])]",
    )
    .field_attribute("WorkloadState.instanceName", "#[spec_mandatory]")
    .field_attribute("WorkloadState.executionState", "#[spec_mandatory]")

    .message_attribute(
        "WorkloadStatesMap",
        "#[derive(spec_macros::Spec)]",
    )
    .message_attribute(
        "WorkloadStatesMap",
        "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .field_attribute(
        "WorkloadStatesMap.agentStateMap",
        "#[spec_field_attr(#[serde(flatten)])]",
    )
    .message_attribute(
        "ExecutionsStatesOfWorkload",
        "#[derive(spec_macros::Spec)]",
    )
    .message_attribute(
        "ExecutionsStatesOfWorkload",
        "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .field_attribute(
        "ExecutionsStatesOfWorkload.wlNameStateMap",
        "#[spec_field_attr(#[serde(flatten)])]",
    )
    .message_attribute(
        "ExecutionsStatesForId",
        "#[derive(spec_macros::Spec)]",
    )
    .message_attribute(
        "ExecutionsStatesForId",
        "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .field_attribute(
        "ExecutionsStatesForId.idStateMap",
        "#[spec_field_attr(#[serde(flatten)])]",
    )

    .message_attribute(
        "ExecutionState",
        "#[derive(spec_macros::Spec)]",
    )
    .message_attribute(
        "ExecutionState",
        "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .message_attribute(
        "ExecutionState",
        "#[spec_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    )
    .field_attribute("ExecutionState.additionalInfo", "#[spec_mandatory]")
    .field_attribute("ExecutionState.ExecutionStateEnum", "#[spec_mandatory]")
    .field_attribute(
        "ExecutionState.ExecutionStateEnum",
        "#[spec_field_attr(#[serde(flatten)])]",
    )
    .enum_attribute(
        "ExecutionStateEnum",
        "#[derive(spec_macros::Spec)]",
    )
    .enum_attribute(
        "ExecutionStateEnum",
        "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
    )
    .enum_attribute(
        "ExecutionStateEnum",
        "#[spec_type_attr(#[serde(tag = \"state\", content = \"subState\")])]",
    )
}

/// This function is used to create and configure the following structs:
/// - LogsRequestSpec
/// - LogsCancelRequestSpec
/// - EventsCancelRequestSpec
/// - UpdateStateRequestSpec
/// - CompleteStateRequestSpec
/// - RequestContentSpec
/// - RequestSpec
pub fn setup_spec_requests(builder: Builder) -> Builder {
    builder
        .message_attribute("LogsRequest", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "LogsRequest",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .field_attribute("LogsRequest.follow", "#[spec_default(false)]")
        .field_attribute("LogsRequest.tail", "#[spec_default(-1)]")
        .message_attribute("LogsCancelRequest", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "LogsCancelRequest",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .message_attribute("EventsCancelRequest", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "EventsCancelRequest",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .message_attribute("UpdateStateRequest", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "UpdateStateRequest",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .field_attribute("UpdateStateRequest.newState", "#[spec_mandatory]")
        .message_attribute("CompleteStateRequest", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "CompleteStateRequest",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .enum_attribute("RequestContent", "#[derive(spec_macros::Spec)]")
        .enum_attribute(
            "RequestContent",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .message_attribute("Request", "#[derive(spec_macros::Spec)]")
        .message_attribute(
            "Request",
            "#[spec_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .field_attribute("Request.RequestContent", "#[spec_mandatory]")
}
