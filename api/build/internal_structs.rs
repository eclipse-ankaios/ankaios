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
/// - CompleteStateInternal
/// - StateInternal
/// - WorkloadMapInternal
pub fn setup_internal_state(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "CompleteState",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "CompleteState",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute(
            "CompleteState",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .field_attribute("CompleteState.desiredState", "#[internal_mandatory]")
        .field_attribute("CompleteState.desiredState", "#[internal_field_attr(#[serde(default)])]")
        .field_attribute("CompleteState.workloadStates", "#[internal_mandatory]")
        .field_attribute("CompleteState.workloadStates", "#[internal_field_attr(#[serde(default)])]")
        .field_attribute("CompleteState.agents", "#[internal_mandatory]")
        .field_attribute("CompleteState.agents", "#[internal_field_attr(#[serde(default)])]")

        .message_attribute(
            "State",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "State",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        ).field_attribute(
            "State.apiVersion",
            "#[internal_field_attr(#[serde(rename = \"apiVersion\")])]",
        )
        .field_attribute("State.workloads","#[internal_mandatory]")
        .field_attribute("State.workloads", "#[internal_field_attr(#[serde(default)])]")
        .field_attribute("State.configs","#[internal_mandatory]")
        .field_attribute("State.configs", "#[internal_field_attr(#[serde(default)])]")

        .message_attribute("WorkloadMap", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "WorkloadMap",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute("WorkloadMap", "#[internal_type_attr(#[serde(transparent)])]")
        .field_attribute("WorkloadMap.workloads", "#[internal_field_attr(#[serde(serialize_with = \"serialize_to_ordered_map\")])]")
}

/// This function is used to create and configure the following structs:
/// - FilesInternal
/// - FileInternal
/// - FileContentInternal
pub fn setup_internal_files(builder: Builder) -> Builder {
    builder
        .message_attribute("Files", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "Files",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .field_attribute("Files.files", "#[internal_field_attr(#[serde(default)])]")
        .message_attribute("File", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "File",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .message_attribute(
            "File",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .enum_attribute("FileContent", "#[derive(internal_derive_macros::Internal)]")
        .enum_attribute(
            "FileContent",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .enum_attribute("FileContent", "#[internal_type_attr(#[serde(untagged)])]")
        .field_attribute("File.FileContent", "#[internal_mandatory]")
        .field_attribute(
            "File.FileContent",
            "#[internal_field_attr(#[serde(flatten)])]",
        )
        .field_attribute("File.FileContent.data", "#[internal_enum_named]")
        .field_attribute(
            "File.FileContent.data",
            "#[internal_field_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .field_attribute("File.FileContent.binaryData", "#[internal_enum_named]")
        .field_attribute(
            "File.FileContent.binaryData",
            "#[internal_field_attr(#[serde(rename_all = \"camelCase\")])]",
        )
}

/// This function is used to create and configure the following structs:
/// - WorkloadInternal
/// - DependenciesInternal
/// - TagsInternal
pub fn setup_internal_workload(builder: Builder) -> Builder {
    builder
        .message_attribute("Workload", "#[derive(internal_derive_macros::Internal)]")
        // .message_attribute(
        //     "Workload",
        //     "#[internal_skip_try_from]",
        // )
        // .message_attribute(
        //     "Workload",
        //     "#[internal_type_attr(#[internal_derive_macros::add_field(name = \"instance_name\", ty = \"WorkloadInstanceNameInternal\")])]",
        // )
        .message_attribute(
            "Workload",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute(
            "Workload",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )

        .field_attribute("Workload.agent", "#[internal_mandatory]")
        .field_attribute("Workload.restartPolicy", "#[internal_mandatory]")
        .field_attribute("Workload.dependencies", "#[internal_mandatory]")
        .field_attribute("Workload.tags", "#[internal_mandatory]")
        .field_attribute("Workload.runtime", "#[internal_mandatory]")
        .field_attribute("Workload.runtimeConfig", "#[internal_mandatory]")
        .field_attribute("Workload.controlInterfaceAccess", "#[internal_mandatory]")
        .field_attribute("Workload.configs", "#[internal_mandatory]")
        .field_attribute("Workload.files", "#[internal_mandatory]")

        .field_attribute("Workload.dependencies", "#[internal_field_attr(#[serde(flatten)])]")
        .field_attribute("Workload.tags", "#[internal_field_attr(#[serde(flatten)])]")
        .field_attribute("Workload.configs", "#[internal_field_attr(#[serde(flatten)])]")
        .field_attribute("Workload.files", "#[internal_field_attr(#[serde(flatten)])]")

        .field_attribute("Workload.restartPolicy", "#[internal_field_attr(#[serde(default)])]")
        .field_attribute("Workload.controlInterfaceAccess", "#[internal_field_attr(#[serde(default)])]")

        .message_attribute(
            "Dependencies",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "Dependencies",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .field_attribute("Dependencies.dependencies", "#[internal_field_attr(#[serde(default, serialize_with = \"serialize_to_ordered_map\")])]")

        .message_attribute("Tags", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "Tags",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]",
        )
        // The custom deserializer is added to support the deprecated apiVersions: v0.1
        .field_attribute("Tags.tags", "#[internal_field_attr(#[serde(default, serialize_with = \"serialize_to_ordered_map\", deserialize_with = \"tag_adapter_deserializer\")])]")
}

/// This function is used to create and configure the following structs:
/// - WorkloadInstanceNameInternal
pub fn setup_internal_workload_instance_name(builder: Builder) -> Builder {
    builder.message_attribute(
        "WorkloadInstanceName",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .message_attribute(
        "WorkloadInstanceName",
        "#[internal_derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]",
    )
    .message_attribute(
        "WorkloadInstanceName",
        "#[internal_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    )
}

/// This function is used to create and configure the following structs:
/// - ControlInterfaceAccessInternal
/// - AccessRightsRuleInternal
/// - AccessRightsRuleEnumInternal
/// - StateRuleInternal
/// - LogRuleInternal
pub fn setup_internal_control_interface_access(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "ControlInterfaceAccess",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .field_attribute(
            "ControlInterfaceAccess.allowRules",
            "#[internal_field_attr(#[serde(default)])]",
        )
        .field_attribute(
            "ControlInterfaceAccess.denyRules",
            "#[internal_field_attr(#[serde(default)])]",
        )

        .message_attribute(
            "AccessRightsRule",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "AccessRightsRule",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .field_attribute("AccessRightsRule.AccessRightsRuleEnum", "#[serde(flatten)]")
        .field_attribute("AccessRightsRule.AccessRightsRuleEnum", "#[internal_field_attr(#[serde(flatten)])]")
        .field_attribute(
            "AccessRightsRule.AccessRightsRuleEnum",
            "#[internal_mandatory]",
        )
        .enum_attribute(
            "AccessRightsRuleEnum",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .enum_attribute(
            "AccessRightsRule.AccessRightsRuleEnum",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]")
        .enum_attribute("AccessRightsRule.AccessRightsRuleEnum", "#[internal_type_attr(#[serde(tag = \"type\")])]")
        .enum_attribute(
            "AccessRightsRule.AccessRightsRuleEnum",
            "#[serde(tag = \"type\")]",
        )

        .message_attribute("StateRule", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "StateRule",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .message_attribute(
            "StateRule",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        // The alias to filterMask is added to support the deprecated apiVersions: v0.1
        .field_attribute("StateRule.filterMasks", "#[internal_field_attr(#[serde(alias = \"filterMask\")])]")

        .message_attribute("LogRule", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "LogRule",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .message_attribute(
            "LogRule",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
}

/// This function is used to create and configure the following structs:
/// - ConfigMappingsInternal
/// - ConfigMapInternal
/// - ConfigItemInternal
/// - ConfigItemEnumInternal
/// - ConfigObjectInternal
/// - ConfigArrayInternal
pub fn setup_internal_configs(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "ConfigMappings",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "ConfigMappings",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]",
        )
        .field_attribute("ConfigMappings.configs", "#[internal_field_attr(#[serde(default, serialize_with = \"serialize_to_ordered_map\")])]")

        .message_attribute("ConfigMap", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "ConfigMap",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]",
        )
        .message_attribute("ConfigMap", "#[internal_type_attr(#[serde(transparent)])]")
        // .field_attribute("ConfigMap.configs", "#[internal_field_attr(#[serde(flatten)])]")
        .message_attribute("ConfigItem", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "ConfigItem",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )

        .message_attribute("ConfigItem", "#[internal_type_attr(#[serde(transparent)])]")

        .field_attribute("ConfigItem.ConfigItemEnum", "#[internal_mandatory]")
        .field_attribute("ConfigItem.ConfigItemEnum", "#[internal_field_attr(#[serde(flatten)])]")
        .enum_attribute("ConfigItemEnum", "#[derive(internal_derive_macros::Internal)]")
        .enum_attribute(
            "ConfigItemEnum",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[internal_type_attr(#[serde(untagged)])]",
        )
        .message_attribute("ConfigArray", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "ConfigArray",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .message_attribute(
            "ConfigArray",
            "#[internal_type_attr(#[serde(transparent)])]",
        )
        .message_attribute("ConfigObject", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "ConfigObject",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .field_attribute("ConfigObject.fields", "#[internal_field_attr(#[serde(flatten)])]")
}

/// This function is used to create and configure the following structs:
/// - AgentMapInternal
/// - AgentAttributesInternal
/// - AgentStatusInternal
/// - CpuUsageInternal
/// - FreeMemoryInternal
pub fn setup_internal_agent_map(builder: Builder) -> Builder {
    builder
        .message_attribute("AgentMap", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "AgentMap",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .message_attribute(
            "AgentAttributes",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "AgentAttributes",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .message_attribute("AgentStatus", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "AgentStatus",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .field_attribute("AgentStatus.cpu_usage", "#[internal_field_attr(#[serde(flatten)])]")
        .field_attribute("AgentStatus.free_memory", "#[internal_field_attr(#[serde(flatten)])]")

        .message_attribute("CpuUsage", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "CpuUsage",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .message_attribute("FreeMemory", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "FreeMemory",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
}

/// This function is used to create and configure the following structs:
/// - WorkloadStateInternal
/// - WorkloadStatesMapInternal
/// - ExecutionsStatesOfWorkloadInternal
/// - ExecutionsStatesForIdInternal
/// - ExecutionStateInternal
/// - ExecutionStateEnumInternal
pub fn setup_internal_workload_states(builder: Builder) -> Builder {
    builder
    .message_attribute(
        "WorkloadState",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .message_attribute(
        "WorkloadState",
        "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .message_attribute(
        "WorkloadState",
        "#[internal_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    ).message_attribute(
        "WorkloadState",
        "#[internal_type_attr(#[serde(rename = \"workloadState\")])]",
    )
    .field_attribute("WorkloadState.instanceName", "#[internal_mandatory]")
    .field_attribute("WorkloadState.executionState", "#[internal_mandatory]")

    .message_attribute(
        "WorkloadStatesMap",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .message_attribute(
        "WorkloadStatesMap",
        "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .field_attribute(
        "WorkloadStatesMap.agentStateMap",
        "#[internal_field_attr(#[serde(flatten)])]",
    )
    .message_attribute(
        "ExecutionsStatesOfWorkload",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .message_attribute(
        "ExecutionsStatesOfWorkload",
        "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .field_attribute(
        "ExecutionsStatesOfWorkload.wlNameStateMap",
        "#[internal_field_attr(#[serde(flatten)])]",
    )
    .message_attribute(
        "ExecutionsStatesForId",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .message_attribute(
        "ExecutionsStatesForId",
        "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .field_attribute(
        "ExecutionsStatesForId.idStateMap",
        "#[internal_field_attr(#[serde(flatten)])]",
    )

    .message_attribute(
        "ExecutionState",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .message_attribute(
        "ExecutionState",
        "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
    )
    .message_attribute(
        "ExecutionState",
        "#[internal_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    )
    .field_attribute("ExecutionState.additionalInfo", "#[internal_mandatory]")
    .field_attribute("ExecutionState.ExecutionStateEnum", "#[internal_mandatory]")
    .field_attribute(
        "ExecutionState.ExecutionStateEnum",
        "#[internal_field_attr(#[serde(flatten)])]",
    )
    .enum_attribute(
        "ExecutionStateEnum",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .enum_attribute(
        "ExecutionStateEnum",
        "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
    )
    .enum_attribute(
        "ExecutionStateEnum",
        "#[internal_type_attr(#[serde(tag = \"state\", content = \"subState\")])]",
    )
}

/// This function is used to create and configure the following structs:
/// - LogsRequestInternal
/// - LogsCancelRequestInternal
/// - UpdateStateRequestInternal
/// - CompleteStateRequestInternal
/// - RequestContentInternal
/// - RequestInternal
pub fn setup_internal_requests(builder: Builder) -> Builder {
    builder
        .message_attribute("LogsRequest", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "LogsRequest",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .field_attribute("LogsRequest.follow", "#[internal_mandatory]")
        .field_attribute("LogsRequest.tail", "#[internal_mandatory]")
        .message_attribute(
            "LogsCancelRequest",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "LogsCancelRequest",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .message_attribute(
            "UpdateStateRequest",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "UpdateStateRequest",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .field_attribute("UpdateStateRequest.newState", "#[internal_mandatory]")
        .message_attribute(
            "CompleteStateRequest",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .message_attribute(
            "CompleteStateRequest",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .enum_attribute(
            "RequestContent",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .enum_attribute(
            "RequestContent",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .message_attribute("Request", "#[derive(internal_derive_macros::Internal)]")
        .message_attribute(
            "Request",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]",
        )
        .field_attribute("Request.RequestContent", "#[internal_mandatory]")
}
