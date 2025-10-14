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

use tonic_prost_build::Builder;

/// This function is used to create and configure the following structs:
/// - FilesInternal
/// - FileInternal
/// - FileContentInternal
pub fn setup_internal_files(builder: Builder) -> Builder {
    builder
        .type_attribute("Files", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "Files",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .type_attribute("File", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "File",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .type_attribute(
            "File",
            "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        )
        .type_attribute("FileContent", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "FileContent",
            "#[internal_derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]",
        )
        .type_attribute("FileContent", "#[internal_type_attr(#[serde(untagged)])]")
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
/// - TagInternal
pub fn setup_internal_workload(builder: Builder) -> Builder {
    builder
    // .type_attribute("Workload", "#[derive(internal_derive_macros::Internal)]")
}

/// This function is used to create and configure the following structs:
/// - WorkloadInstanceNameInternal
pub fn setup_internal_workload_instance_name(builder: Builder) -> Builder {
    builder.type_attribute(
        "WorkloadInstanceName",
        "#[derive(internal_derive_macros::Internal)]",
    )
    .type_attribute(
        "WorkloadInstanceName",
        "#[internal_derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]",
    )
    .type_attribute(
        "WorkloadInstanceName",
        "#[internal_type_attr(#[serde(default, rename_all = \"camelCase\")])]",
    )
}

/// This function is used to create and configure the following structs:
/// - ControlInterfaceAccessInternal
/// - AccessRightsRuleInternal
/// - StateRuleInternal
/// - ReadWriteEnumInternal
/// - LogRuleInternal
pub fn setup_internal_control_interface_access(builder: Builder) -> Builder {
    // TODO: Implement in code once the ControlInterfaceAccessInternal is done
    builder
        // .type_attribute(
        //     "ControlInterfaceAccess",
        //     "#[derive(internal_derive_macros::Internal)]",
        // )
        // .type_attribute(
        //     "AccessRightsRule",
        //     "#[derive(internal_derive_macros::Internal)]",
        // )
        // .type_attribute(
        //     "AccessRightsRule",
        //     "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        // )
        // .field_attribute(
        //     "AccessRightsRule.AccessRightsRuleEnum",
        //     "#[internal_mandatory]",
        // )
        // .type_attribute(
        //     "AccessRightsRuleEnum",
        //     "#[derive(internal_derive_macros::Internal)]",
        // )
        // .type_attribute(
        //     "AccessRightsRuleEnum",
        //     "#[internal_type_attr(#[serde(tag = \"type\")])]",
        // )
        // .type_attribute("StateRule", "#[derive(internal_derive_macros::Internal)]")
        // .type_attribute(
        //     "StateRule",
        //     "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        // )
        // .type_attribute(
        //     "StateRule",
        //     "#[internal_type_attr(#[serde(rename_all = \"camelCase\")])]",
        // )
        .type_attribute(
            "ReadWriteEnum",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .type_attribute(
            "ReadWriteEnum",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .type_attribute("LogRule", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "LogRule",
            "#[internal_derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]",
        )
        .type_attribute(
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
    // .type_attribute(
    //     "ConfigMappings",
    //     "#[derive(internal_derive_macros::Internal)]",
    // )
    // .type_attribute("ConfigMap", "#[derive(internal_derive_macros::Internal)]")
    // .type_attribute("ConfigItem", "#[derive(internal_derive_macros::Internal)]")
    // .type_attribute(
    //     "ConfigItemEnum",
    //     "#[derive(internal_derive_macros::Internal)]",
    // )
    // .type_attribute(
    //     "ConfigObject",
    //     "#[derive(internal_derive_macros::Internal)]",
    // )
    // .type_attribute("ConfigArray", "#[derive(internal_derive_macros::Internal)]")
}

/// This function is used to create and configure the following structs:
/// - AgentMapInternal
/// - AgentAttributesInternal
/// - CpuUsageInternal
/// - FreeMemoryInternal
pub fn setup_internal_agent_map(builder: Builder) -> Builder {
    builder
        .type_attribute("AgentMap", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "AgentMap",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .type_attribute(
            "AgentAttributes",
            "#[derive(internal_derive_macros::Internal)]",
        )
        .type_attribute(
            "AgentAttributes",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .type_attribute("CpuUsage", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "CpuUsage",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .field_attribute("CpuUsage.cpu_usage", "#[internal_mandatory]")
        .type_attribute("FreeMemory", "#[derive(internal_derive_macros::Internal)]")
        .type_attribute(
            "FreeMemory",
            "#[internal_derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default, PartialEq, Eq)]",
        )
        .field_attribute("FreeMemory.free_memory", "#[internal_mandatory]")
}

/// This function is used to create and configure the following structs:
/// - WorkloadStateInternal
/// - ExecutionStateInternal
/// - ExecutionStateEnumInternal
pub fn setup_internal_workload_state(builder: Builder) -> Builder {
    builder
    // .type_attribute(
    //     "WorkloadState",
    //     "#[derive(internal_derive_macros::Internal)]",
    // )
    // .type_attribute(
    //     "ExecutionState",
    //     "#[derive(internal_derive_macros::Internal)]",
    // )
    // .type_attribute(
    //     "ExecutionStateEnum",
    //     "#[derive(internal_derive_macros::Internal)]",
    // )
}
