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

pub fn setup_schema_annotations(mut builder: Builder) -> Builder {
    // Setup the State schema annotations
    builder = builder
        .message_attribute(
            "State",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "State",
            "#[internal_type_attr(#[serde(rename = \"desiredState\")])]",
        )
        .message_attribute(
            "WorkloadMap",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "WorkloadMap",
            "#[internal_type_attr(#[serde(rename = \"workloadMap\")])]",
        )
        .message_attribute(
            "Workload",
            "#[internal_type_attr(#[serde(rename = \"workload\")])]",
        )
        .message_attribute(
            "ConfigMap",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigMap",
            "#[internal_type_attr(#[serde(rename = \"configMap\")])]",
        )
        .message_attribute(
            "ConfigItem",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigItem",
            "#[internal_type_attr(#[serde(rename = \"configItem\")])]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[internal_type_attr(#[serde(rename = \"configItemEnum\")])]",
        )
        .message_attribute(
            "ConfigObject",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigObject",
            "#[internal_type_attr(#[serde(rename = \"configObject\")])]",
        )
        .message_attribute(
            "ConfigArray",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        ).message_attribute(
            "ConfigArray",
            "#[internal_type_attr(#[serde(rename = \"configArray\")])]",
        );

    // Setup the Workload related schema annotations
    builder
        .enum_attribute("RestartPolicy", "#[derive(schemars::JsonSchema)]")
        .enum_attribute(
            "RestartPolicy",
            "#[serde(rename = \"restartPolicy\")]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[internal_type_attr(#[serde(rename = \"controlInterfaceAccess\")])]",
        )
        .message_attribute(
            "AccessRightsRule",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "AccessRightsRule",
            "#[internal_type_attr(#[serde(rename = \"accessRightsRule\")])]",
        )
        .enum_attribute(
            "AccessRightsRuleEnum",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "StateRule",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "StateRule",
            "#[internal_type_attr(#[serde(rename = \"stateRule\")])]",
        )
        .enum_attribute("ReadWriteEnum", "#[derive(schemars::JsonSchema)]")
        .enum_attribute(
            "ReadWriteEnum",
            "#[serde(rename = \"readWriteEnum\")]",
        )
        .message_attribute(
            "LogRule",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "LogRule",
            "#[internal_type_attr(#[serde(rename = \"logRule\")])]",
        )
        .message_attribute(
            "ConfigMappings",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "File",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "File",
            "#[internal_type_attr(#[serde(rename = \"file\")])]",
        )
        .message_attribute(
            "Files",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "FileContent",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "Tags",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "Dependencies",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute("AddCondition", "#[derive(schemars::JsonSchema)]")
        .enum_attribute(
            "AddCondition",
            "#[serde(rename = \"addCondition\")]",
        )
        .message_attribute(
            "Workload",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
}
