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

/// Add annotations required for generating the JSON schema for the spec objects.
///
/// The Json schema will be generated using the `ank_schema` crate.
pub fn setup_schema_annotations(mut builder: Builder) -> Builder {
    builder = setup_state_annotations(builder);
    builder = setup_workload_related_annotations(builder);

    builder
}

// TODO 313 - unify accepted char sets
const ACCEPTED_CHAR_SET: &str = "[a-zA-Z0-9_-]";

fn setup_state_annotations(builder: Builder) -> Builder {
    builder
        .message_attribute(
            "State",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "State",
            "#[spec_type_attr(#[serde(rename = \"desiredState\")])]",
        )
        // TODO 313
        .field_attribute("State.apiVersion", r#"#[spec_field_attr(#[schemars(regex(pattern = r"^v1$"))])]"#)
        .message_attribute(
            "WorkloadMap",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "WorkloadMap",
            "#[spec_type_attr(#[serde(rename = \"workloadMap\")])]",
        )
        // TODO 313
        .field_attribute(
            "WorkloadMap.workloads",
            "#[spec_field_attr(#[schemars(schema_with = \"constrained_map_schema::<WorkloadSpec>\")])]",
        )
        .message_attribute(
            "Workload",
            "#[spec_type_attr(#[serde(rename = \"workload\")])]",
        )
        // TODO 313
        .field_attribute(
            "Workload.agent",
            format!(r#"#[spec_field_attr(#[schemars(regex(pattern = r"^(?:{ACCEPTED_CHAR_SET}*|\{{\{{{ACCEPTED_CHAR_SET}+(?:\.{ACCEPTED_CHAR_SET}+)*\}}\}})$"),length(min = 0, max = 63))])]"#)
        )
        // TODO 313
        .field_attribute(
            "ConfigMappings.configs",
            "#[spec_field_attr(#[schemars(schema_with = \"constrained_config_map\")])]",
        )
        // TODO 313
        .field_attribute(
            "ConfigMap.configs",
            "#[spec_field_attr(#[schemars(schema_with = \"constrained_map_schema::<ConfigItemSpec>\")])]",
        )
        // TODO 313
        .field_attribute(
            "ConfigObject.fields",
            "#[spec_field_attr(#[schemars(schema_with = \"constrained_map_schema::<ConfigItemSpec>\")])]",
        )
        .message_attribute(
            "ConfigMap",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigMap",
            "#[spec_type_attr(#[serde(rename = \"configMap\")])]",
        )
        .message_attribute(
            "ConfigItem",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigItem",
            "#[spec_type_attr(#[serde(rename = \"configItem\")])]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "ConfigItemEnum",
            "#[spec_type_attr(#[serde(rename = \"configItemEnum\")])]",
        )
        .message_attribute(
            "ConfigObject",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigObject",
            "#[spec_type_attr(#[serde(rename = \"configObject\")])]",
        )
        .message_attribute(
            "ConfigArray",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ConfigArray",
            "#[spec_type_attr(#[serde(rename = \"configArray\")])]",
        )
}

fn setup_workload_related_annotations(builder: Builder) -> Builder {
    builder
        .enum_attribute("RestartPolicy", "#[derive(schemars::JsonSchema)]")
        .enum_attribute("RestartPolicy", "#[serde(rename = \"restartPolicy\")]")
        .message_attribute(
            "ControlInterfaceAccess",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "ControlInterfaceAccess",
            "#[spec_type_attr(#[serde(rename = \"controlInterfaceAccess\")])]",
        )
        .message_attribute(
            "AccessRightsRule",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "AccessRightsRule",
            "#[spec_type_attr(#[serde(rename = \"accessRightsRule\")])]",
        )
        .enum_attribute(
            "AccessRightsRuleEnum",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "StateRule",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "StateRule",
            "#[spec_type_attr(#[serde(rename = \"stateRule\")])]",
        )
        .enum_attribute("ReadWriteEnum", "#[derive(schemars::JsonSchema)]")
        .enum_attribute("ReadWriteEnum", "#[serde(rename = \"readWriteEnum\")]")
        .message_attribute(
            "LogRule",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute(
            "LogRule",
            "#[spec_type_attr(#[serde(rename = \"logRule\")])]",
        )
        .message_attribute(
            "ConfigMappings",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute("File", "#[spec_type_attr(#[derive(schemars::JsonSchema)])]")
        .message_attribute("File", "#[spec_type_attr(#[serde(rename = \"file\")])]")
        .message_attribute(
            "Files",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute(
            "FileContent",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .message_attribute("Tags", "#[spec_type_attr(#[derive(schemars::JsonSchema)])]")
        .message_attribute(
            "Dependencies",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
        .enum_attribute("AddCondition", "#[derive(schemars::JsonSchema)]")
        .enum_attribute("AddCondition", "#[serde(rename = \"addCondition\")]")
        .message_attribute(
            "Workload",
            "#[spec_type_attr(#[derive(schemars::JsonSchema)])]",
        )
}
