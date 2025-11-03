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
    builder = builder
        .message_attribute(
            "State",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "WorkloadMap",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigMap",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigItem",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigObject",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigArray",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        ;

    builder
        .enum_attribute("RestartPolicy", "#[derive(schemars::JsonSchema)]")
        .type_attribute(
            "ControlInterfaceAccess",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "AccessRightsRule",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "AccessRightsRuleEnum",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "StateRule",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute("ReadWriteEnum", "#[derive(schemars::JsonSchema)]")
        .type_attribute(
            "LogRule",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "ConfigMappings",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "File",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "Files",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "FileContent",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "Tags",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .type_attribute(
            "Dependencies",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute("AddCondition", "#[derive(schemars::JsonSchema)]")
        .type_attribute(
            "Workload",
            "#[internal_type_attr(#[derive(schemars::JsonSchema)])]",
        )
}
