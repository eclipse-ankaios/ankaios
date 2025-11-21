// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use super::WorkloadInstanceNameSpec;
use ankaios_api::ank_base::{
    ConfigItemSpec, FileContentSpec, FileSpec, FilesSpec, WorkloadNamed, WorkloadSpec,
};

use handlebars::{Handlebars, RenderError};
use std::{collections::HashMap, fmt};

pub type RenderedWorkloads = HashMap<String, WorkloadNamed>;

#[cfg(test)]
use mockall::mock;

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigRenderError {
    Field(String, String),
    NotExistingConfigKey(String),
}

impl fmt::Display for ConfigRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigRenderError::Field(field, reason) => {
                write!(f, "Failed to render field '{field}': '{reason}'")
            }
            ConfigRenderError::NotExistingConfigKey(config_key) => {
                write!(
                    f,
                    "Workload references config key '{config_key}' that does not exist"
                )
            }
        }
    }
}

impl ConfigRenderError {
    pub fn for_field(field: &str) -> impl Fn(RenderError) -> Self + '_ {
        move |err| ConfigRenderError::Field(field.to_owned(), err.to_string())
    }
    pub fn for_files(mount_point: &str) -> impl Fn(RenderError) -> Self + '_ {
        move |err| {
            ConfigRenderError::Field(
                format!("files with mount point {mount_point}"),
                err.to_string(),
            )
        }
    }
}

// [impl->swdd~server-delegate-template-render-to-external-library~1]
pub struct ConfigRenderer {
    template_engine: Handlebars<'static>,
}

impl Default for ConfigRenderer {
    fn default() -> Self {
        let mut template_engine = Handlebars::new();
        template_engine.set_strict_mode(true); // enable throwing render errors if context data is valid
        template_engine.register_escape_fn(handlebars::no_escape); // prevent escaping like double quotes to &quot; ...

        // [impl->swdd~config-renderer-supports-rendering-with-keeping-line-indent~1]
        template_engine
            .register_partial("indent", "{{content}}")
            .unwrap();
        Self { template_engine }
    }
}

impl ConfigRenderer {
    // [impl->swdd~config-renderer-renders-workload-configuration~2]
    pub fn render_workloads(
        &self,
        workloads: &HashMap<String, WorkloadSpec>,
        configs: &HashMap<String, ConfigItemSpec>,
    ) -> Result<RenderedWorkloads, ConfigRenderError> {
        let mut rendered_workloads = HashMap::new();
        for (workload_name, workload) in workloads {
            let rendered_workload = if workload.configs.configs.is_empty() {
                log::debug!(
                    "Skipping to render workload '{workload_name}' as no config is assigned to the workload"
                );
                WorkloadNamed::from((workload_name.to_owned(), workload.clone()))
            } else {
                let wl_config_map = self.create_config_map_for_workload(workload, configs)?;
                log::debug!("Rendering workload '{workload_name}' with config '{wl_config_map:?}'");
                self.render_workload_fields(workload_name, workload, &wl_config_map)?
            };

            rendered_workloads.insert(workload_name.clone(), rendered_workload);
        }
        log::trace!("Rendered CompleteState: {rendered_workloads:?}");
        Ok(rendered_workloads)
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~2]
    fn create_config_map_for_workload<'a>(
        &self,
        workload: &'a WorkloadSpec,
        configs: &'a HashMap<String, ConfigItemSpec>,
    ) -> Result<HashMap<&'a String, &'a ConfigItemSpec>, ConfigRenderError> {
        let mut wl_config_map = HashMap::new();
        for (config_alias, config_key) in &workload.configs.configs {
            if let Some(config_value) = configs.get(config_key) {
                wl_config_map.insert(config_alias, config_value);
            } else {
                return Err(ConfigRenderError::NotExistingConfigKey(config_key.clone()));
            }
        }
        Ok(wl_config_map)
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~2]
    fn render_workload_fields(
        &self,
        workload_name: &str,
        workload: &WorkloadSpec,
        wl_config_map: &HashMap<&String, &ConfigItemSpec>,
    ) -> Result<WorkloadNamed, ConfigRenderError> {
        let rendered_runtime_config = self
            .template_engine
            .render_template(&workload.runtime_config, &wl_config_map)
            .map_err(ConfigRenderError::for_field("runtimeConfig"))?;

        let rendered_agent_name = self
            .template_engine
            .render_template(&workload.agent, &wl_config_map)
            .map_err(ConfigRenderError::for_field("agent"))?;

        let rendered_files = self.render_files_field(&workload.files.files, wl_config_map)?;

        Ok(WorkloadNamed {
            instance_name: WorkloadInstanceNameSpec::builder()
                .workload_name(workload_name)
                .agent_name(rendered_agent_name.clone())
                .config(&rendered_runtime_config)
                .build(),
            workload: WorkloadSpec {
                agent: rendered_agent_name.clone(),
                runtime: workload.runtime.clone(),
                runtime_config: rendered_runtime_config,
                tags: workload.tags.clone(),
                dependencies: workload.dependencies.clone(),
                restart_policy: workload.restart_policy,
                files: rendered_files,
                configs: Default::default(),
                control_interface_access: workload.control_interface_access.clone(),
            },
        })
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~2]
    fn render_files_field(
        &self,
        files: &[FileSpec],
        wl_config_map: &HashMap<&String, &ConfigItemSpec>,
    ) -> Result<FilesSpec, ConfigRenderError> {
        let mut rendered_files = Vec::new();
        for current_file in files {
            let mut rendered_file = current_file.clone();

            rendered_file.file_content = match rendered_file.file_content {
                FileContentSpec::Data { data } => FileContentSpec::Data {
                    data: self
                        .template_engine
                        .render_template(&data, &wl_config_map)
                        .map_err(ConfigRenderError::for_files(&rendered_file.mount_point))?,
                },
                FileContentSpec::BinaryData { binary_data } => FileContentSpec::BinaryData {
                    binary_data: self
                        .template_engine
                        .render_template(&binary_data, &wl_config_map)
                        .map_err(ConfigRenderError::for_files(&rendered_file.mount_point))?,
                },
            };

            rendered_files.push(rendered_file);
        }
        Ok(FilesSpec {
            files: rendered_files,
        })
    }

    // fn render_file_content(self, file_content: &FileContentSpec, wl_config_map: &HashMap<&String, &ConfigItem>) -> Result<FileContentSpec, ConfigRenderError> {
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mock! {
    pub ConfigRenderer {
        pub fn render_workloads(
            &self,
            workloads: &HashMap<String, WorkloadSpec>,
            configs: &HashMap<String, ConfigItemSpec>,
        ) -> Result<RenderedWorkloads, ConfigRenderError>;
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigRenderError, ConfigRenderer, RenderedWorkloads};
    use ankaios_api::ank_base::{
        ConfigItemEnumSpec, ConfigItemSpec, ConfigMappingsSpec, FileContentSpec, FileSpec,
        WorkloadNamed, WorkloadSpec,
    };
    use ankaios_api::test_utils::{
        generate_test_configs, generate_test_workload_with_param,
        generate_test_workload_with_runtime_config,
    };

    use std::collections::HashMap;

    const WORKLOAD_NAME_1: &str = "workload_A";
    const AGENT_A: &str = "agent_A";
    const RUNTIME: &str = "runtime";

    fn generate_test_templated_workload_files() -> Vec<FileSpec> {
        vec![
            FileSpec {
                mount_point: "/file.json".to_string(),
                file_content: FileContentSpec::Data {
                    data: "{{ref1.config_file}}".into(),
                },
            },
            FileSpec {
                mount_point: "/binary_file".to_string(),
                file_content: FileContentSpec::BinaryData {
                    binary_data: "{{ref1.binary_file}}".into(),
                },
            },
        ]
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_render_agent_and_runtime_config_fields_successfully() {
        let templated_runtime_config =
            "some_value_1: {{ref1.values.value_1}}\nsome_value_2: {{ref1.values.value_2.0}}";
        let templated_agent_name = "{{ref1.agent_name}}";
        let stored_workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            templated_agent_name,
            RUNTIME,
            templated_runtime_config,
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let mut expected_workload: WorkloadNamed = generate_test_workload_with_runtime_config(
            AGENT_A,
            RUNTIME,
            "some_value_1: value123\nsome_value_2: list_value_1",
        );
        expected_workload.workload.configs.configs.clear();

        let result = renderer.render_workloads(&workloads, &configs.configs);

        assert_eq!(
            Ok(RenderedWorkloads::from([(
                WORKLOAD_NAME_1.to_owned(),
                expected_workload
            )])),
            result
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_render_files_fields_successfully() {
        let mut stored_workload: WorkloadSpec = generate_test_workload_with_param(AGENT_A, RUNTIME);
        stored_workload.files.files = generate_test_templated_workload_files();

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let mut expected_workload: WorkloadNamed =
            generate_test_workload_with_param(AGENT_A, RUNTIME);
        expected_workload.workload.configs.configs.clear();

        let result = renderer.render_workloads(&workloads, &configs.configs);

        assert_eq!(
            Ok(RenderedWorkloads::from([(
                WORKLOAD_NAME_1.to_owned(),
                expected_workload
            )])),
            result
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_render_files_fields_text_file_render_error() {
        let mut stored_workload: WorkloadSpec = generate_test_workload_with_param(AGENT_A, RUNTIME);
        stored_workload.files.files = vec![FileSpec {
            mount_point: "/file.json".to_string(),
            file_content: FileContentSpec::Data {
                data: "{{invalid_ref.file_content}}".into(),
            },
        }];

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let result = renderer.render_workloads(&workloads, &configs.configs);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::Field(field, _) if field.starts_with("files"))
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_render_files_fields_binary_file_render_error() {
        let mut stored_workload: WorkloadSpec = generate_test_workload_with_param(AGENT_A, RUNTIME);
        stored_workload.files.files = vec![FileSpec {
            mount_point: "/binary_file".to_string(),
            file_content: FileContentSpec::BinaryData {
                binary_data: "{{invalid_ref.binary_data}}".into(),
            },
        }];

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        assert!(
            renderer
                .render_workloads(&workloads, &configs.configs)
                .is_err()
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_fails_field_uses_config_key_instead_of_alias() {
        let templated_runtime_config = "config_1: {{config_1.values.value_1}}";
        let stored_workload =
            generate_test_workload_with_runtime_config(AGENT_A, RUNTIME, templated_runtime_config);

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        assert!(
            renderer
                .render_workloads(&workloads, &configs.configs)
                .is_err()
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_not_rendering_workloads_with_no_referenced_configs() {
        let templated_runtime_config = "config_1: {{config_1.values.value_1}}";
        let templated_agent_name = "{{config_1.agent_name}}";
        let mut stored_workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            templated_agent_name,
            RUNTIME,
            templated_runtime_config,
        );

        stored_workload.configs.configs.clear(); // no configs assigned

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let mut expected_workload: WorkloadNamed = generate_test_workload_with_runtime_config(
            templated_agent_name.to_owned(),
            RUNTIME.to_owned(),
            templated_runtime_config.to_owned(),
        );

        // Need to clear configs so that the test passes
        // This test does not make sense
        expected_workload.workload.configs.configs.clear();

        let result = renderer.render_workloads(&workloads, &configs.configs);

        assert_eq!(
            Ok(RenderedWorkloads::from([(
                WORKLOAD_NAME_1.to_owned(),
                expected_workload
            )])),
            result
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_fails_workload_references_not_existing_config_key() {
        let templated_runtime_config = "config_1: {{ref1.values.value_1}}";
        let mut stored_workload: WorkloadSpec =
            generate_test_workload_with_runtime_config(AGENT_A, RUNTIME, templated_runtime_config);

        stored_workload.configs = ConfigMappingsSpec {
            configs: HashMap::from([("ref1".to_owned(), "not_existing_config_key".to_owned())]),
        };

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();
        let result = renderer.render_workloads(&workloads, &configs.configs);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::NotExistingConfigKey(config_key) if config_key == "not_existing_config_key")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_fails_workload_references_unused_not_existing_config_key() {
        let mut stored_workload: WorkloadSpec =
            generate_test_workload_with_runtime_config(AGENT_A, RUNTIME, "some runtime config");

        stored_workload.configs = ConfigMappingsSpec {
            configs: HashMap::from([(
                "ref1".to_owned(),
                "not_existing_unused_config_key".to_owned(),
            )]),
        };

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();
        let result = renderer.render_workloads(&workloads, &configs.configs);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::NotExistingConfigKey(config_key) if config_key == "not_existing_unused_config_key")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_fails_runtime_config_contains_non_existing_config() {
        let templated_runtime_config = "config_1: {{config_1.values.not_existing_key}}";
        let stored_workload: WorkloadSpec =
            generate_test_workload_with_runtime_config(AGENT_A, RUNTIME, templated_runtime_config);

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let result = renderer.render_workloads(&workloads, &configs.configs);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::Field(field, _) if field == "runtimeConfig")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_fails_agent_contains_non_existing_config() {
        let stored_workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            "{{config_1.not_existing_key}}",
            RUNTIME,
            "some runtime config",
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let result = renderer.render_workloads(&workloads, &configs.configs);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::Field(field, _) if field == "agent")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_fails_workload_references_empty_configs() {
        let templated_runtime_config = "config_1: {{config_1.values.value_1}}";
        let stored_workload: WorkloadSpec =
            generate_test_workload_with_runtime_config(AGENT_A, RUNTIME, templated_runtime_config);

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = HashMap::default();
        let renderer = ConfigRenderer::default();

        assert!(renderer.render_workloads(&workloads, &configs).is_err());
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    // [utest->swdd~config-renderer-supports-rendering-with-keeping-line-indent~1]
    #[test]
    fn utest_render_workloads_with_keeping_indentation_level_with_partial() {
        let runtime_config_with_partial_template = r#"
        some:
          keys:
            before:
              config_with_indent: |
                {{> indent content=ref1}}"#;

        let mut stored_workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            AGENT_A,
            RUNTIME,
            runtime_config_with_partial_template,
        );

        stored_workload.configs = ConfigMappingsSpec {
            configs: HashMap::from([("ref1".to_owned(), "config_1".to_owned())]),
        };

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let multi_line_config_value = "value_1\nvalue_2\nvalue_3".to_string();
        let configs = HashMap::from([(
            "config_1".to_string(),
            ConfigItemSpec {
                config_item_enum: ConfigItemEnumSpec::String(multi_line_config_value),
            },
        )]);
        let renderer = ConfigRenderer::default();

        let render_result = renderer.render_workloads(&workloads, &configs);
        assert!(render_result.is_ok());
        let rendered_workloads = render_result.unwrap();

        let workload = rendered_workloads.get(WORKLOAD_NAME_1).unwrap();

        let expected_expanded_runtime_config = r#"
        some:
          keys:
            before:
              config_with_indent: |
                value_1
                value_2
                value_3"#;

        assert_eq!(
            workload.workload.runtime_config,
            expected_expanded_runtime_config
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~2]
    #[test]
    fn utest_render_workloads_prevent_escaping_special_characters() {
        const CONFIG_VALUE: &str = "value\"with\"escape\'characters\'";

        let mut stored_workload: WorkloadSpec = generate_test_workload_with_runtime_config(
            AGENT_A,
            RUNTIME,
            "config_of_special_char_sequences: {{special_conf}}",
        );

        stored_workload.configs = ConfigMappingsSpec {
            configs: HashMap::from([("special_conf".into(), "config_special_chars".into())]),
        };

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = HashMap::from([(
            "config_special_chars".to_string(),
            ConfigItemSpec {
                config_item_enum: ConfigItemEnumSpec::String(CONFIG_VALUE.to_owned()),
            },
        )]);

        let renderer = ConfigRenderer::default();

        let mut expected_workload: WorkloadNamed = generate_test_workload_with_runtime_config(
            AGENT_A.to_owned(),
            RUNTIME.to_owned(),
            format!("config_of_special_char_sequences: {CONFIG_VALUE}"),
        );
        expected_workload.workload.configs.configs.clear();

        let result = renderer.render_workloads(&workloads, &configs);

        assert_eq!(
            Ok(RenderedWorkloads::from([(
                WORKLOAD_NAME_1.to_owned(),
                expected_workload
            )])),
            result
        );
    }
}
