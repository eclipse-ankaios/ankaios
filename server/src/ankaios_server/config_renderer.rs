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

use std::{collections::HashMap, fmt};

use common::objects::{ConfigItem, StoredWorkloadSpec, WorkloadInstanceName, WorkloadSpec};
use handlebars::Handlebars;

pub type RenderedWorkloads = HashMap<String, WorkloadSpec>;

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
                write!(f, "Failed to render field '{}': '{}'", field, reason)
            }
            ConfigRenderError::NotExistingConfigKey(config_key) => {
                write!(
                    f,
                    "Workload references config key '{}' that does not exist",
                    config_key
                )
            }
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
        Self { template_engine }
    }
}

impl ConfigRenderer {
    // [impl->swdd~config-renderer-renders-workload-configuration~1]
    pub fn render_workloads(
        &self,
        workloads: &HashMap<String, StoredWorkloadSpec>,
        configs: &HashMap<String, ConfigItem>,
    ) -> Result<RenderedWorkloads, ConfigRenderError> {
        let mut rendered_workloads = HashMap::new();
        for (workload_name, stored_workload) in workloads {
            let workload_spec = if stored_workload.configs.is_empty() {
                log::debug!(
                    "Skipping to render workload '{}' as no config is assigned to the workload",
                    workload_name
                );
                WorkloadSpec::from((workload_name.to_owned(), stored_workload.clone()))
            } else {
                let wl_config_map =
                    self.create_config_map_for_workload(stored_workload, configs)?;
                log::debug!(
                    "Rendering workload '{}' with config '{:?}'",
                    workload_name,
                    wl_config_map
                );
                self.render_workload_fields(workload_name, stored_workload, &wl_config_map)?
            };

            rendered_workloads.insert(workload_name.clone(), workload_spec);
        }
        log::debug!("Rendered CompleteState: {:?}", rendered_workloads);
        Ok(rendered_workloads)
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~1]
    fn create_config_map_for_workload<'a>(
        &self,
        workload_spec: &'a StoredWorkloadSpec,
        configs: &'a HashMap<String, ConfigItem>,
    ) -> Result<HashMap<&'a String, &'a ConfigItem>, ConfigRenderError> {
        let mut wl_config_map = HashMap::new();
        for (config_alias, config_key) in &workload_spec.configs {
            if let Some(config_value) = configs.get(config_key) {
                wl_config_map.insert(config_alias, config_value);
            } else {
                return Err(ConfigRenderError::NotExistingConfigKey(config_key.clone()));
            }
        }
        Ok(wl_config_map)
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~1]
    fn render_workload_fields(
        &self,
        workload_name: &str,
        workload: &StoredWorkloadSpec,
        wl_config_map: &HashMap<&String, &ConfigItem>,
    ) -> Result<WorkloadSpec, ConfigRenderError> {
        let rendered_runtime_config = self
            .template_engine
            .render_template(&workload.runtime_config, &wl_config_map)
            .map_err(|err| ConfigRenderError::Field("runtimeConfig".to_owned(), err.to_string()))?;

        let rendered_agent_name = self
            .template_engine
            .render_template(&workload.agent, &wl_config_map)
            .map_err(|err| ConfigRenderError::Field("agent".to_owned(), err.to_string()))?;

        Ok(WorkloadSpec {
            instance_name: WorkloadInstanceName::builder()
                .workload_name(workload_name)
                .agent_name(rendered_agent_name)
                .config(&rendered_runtime_config)
                .build(),
            runtime: workload.runtime.clone(),
            runtime_config: rendered_runtime_config,
            tags: workload.tags.clone(),
            dependencies: workload.dependencies.clone(),
            restart_policy: workload.restart_policy.clone(),
            control_interface_access: workload.control_interface_access.clone(),
        })
    }
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
            workloads: &HashMap<String, StoredWorkloadSpec>,
            configs: &HashMap<String, ConfigItem>,
        ) -> Result<RenderedWorkloads, ConfigRenderError>;
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigRenderError, ConfigRenderer, RenderedWorkloads};
    use std::collections::HashMap;

    use common::objects::{
        generate_test_configs, generate_test_stored_workload_spec_with_config,
        generate_test_workload_spec_with_runtime_config,
    };

    const WORKLOAD_NAME_1: &str = "workload_1";
    const AGENT_A: &str = "agent_A";
    const RUNTIME: &str = "runtime";

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_render_required_fields_successfully() {
        let templated_runtime_config =
            "some_value_1: {{ref1.values.value_1}}\nsome_value_2: {{ref1.values.value_2.0}}";
        let templated_agent_name = "{{ref1.agent_name}}";
        let stored_workload = generate_test_stored_workload_spec_with_config(
            templated_agent_name,
            RUNTIME,
            templated_runtime_config,
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let expected_workload_spec = generate_test_workload_spec_with_runtime_config(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
            "some_value_1: value123\nsome_value_2: list_value_1".to_owned(),
        );

        let result = renderer.render_workloads(&workloads, &configs);

        assert_eq!(
            Ok(RenderedWorkloads::from([(
                WORKLOAD_NAME_1.to_owned(),
                expected_workload_spec
            )])),
            result
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_fails_field_uses_config_key_instead_of_alias() {
        let templated_runtime_config = "config_1: {{config_1.values.value_1}}";
        let stored_workload = generate_test_stored_workload_spec_with_config(
            AGENT_A,
            RUNTIME,
            templated_runtime_config,
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        assert!(renderer.render_workloads(&workloads, &configs).is_err());
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_not_rendering_workloads_with_no_referenced_configs() {
        let templated_runtime_config = "config_1: {{config_1.values.value_1}}";
        let templated_agent_name = "{{config_1.agent_name}}";
        let mut stored_workload = generate_test_stored_workload_spec_with_config(
            templated_agent_name,
            RUNTIME,
            templated_runtime_config,
        );

        stored_workload.configs.clear(); // no configs assigned

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let expected_workload_spec = generate_test_workload_spec_with_runtime_config(
            templated_agent_name.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
            templated_runtime_config.to_owned(),
        );

        let result = renderer.render_workloads(&workloads, &configs);

        assert_eq!(
            Ok(RenderedWorkloads::from([(
                WORKLOAD_NAME_1.to_owned(),
                expected_workload_spec
            )])),
            result
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_fails_workload_references_not_existing_config_key() {
        let templated_runtime_config = "config_1: {{ref1.values.value_1}}";
        let mut stored_workload = generate_test_stored_workload_spec_with_config(
            AGENT_A,
            RUNTIME,
            templated_runtime_config,
        );

        stored_workload.configs =
            HashMap::from([("ref1".to_owned(), "not_existing_config_key".to_owned())]);

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();
        let result = renderer.render_workloads(&workloads, &configs);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::NotExistingConfigKey(config_key) if config_key == "not_existing_config_key")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_fails_workload_references_unused_not_existing_config_key() {
        let mut stored_workload =
            generate_test_stored_workload_spec_with_config(AGENT_A, RUNTIME, "some runtime config");

        stored_workload.configs = HashMap::from([(
            "ref1".to_owned(),
            "not_existing_unused_config_key".to_owned(),
        )]);

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();
        let result = renderer.render_workloads(&workloads, &configs);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::NotExistingConfigKey(config_key) if config_key == "not_existing_unused_config_key")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_fails_runtime_config_contains_non_existing_config() {
        let templated_runtime_config = "config_1: {{config_1.values.not_existing_key}}";
        let stored_workload = generate_test_stored_workload_spec_with_config(
            AGENT_A,
            RUNTIME,
            templated_runtime_config,
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let result = renderer.render_workloads(&workloads, &configs);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::Field(field, _) if field == "runtimeConfig")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_fails_agent_contains_non_existing_config() {
        let stored_workload = generate_test_stored_workload_spec_with_config(
            "{{config_1.not_existing_key}}",
            RUNTIME,
            "some runtime config",
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = generate_test_configs();
        let renderer = ConfigRenderer::default();

        let result = renderer.render_workloads(&workloads, &configs);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ConfigRenderError::Field(field, _) if field == "agent")
        );
    }

    // [utest->swdd~config-renderer-renders-workload-configuration~1]
    #[test]
    fn utest_render_workloads_fails_workload_references_empty_configs() {
        let templated_runtime_config = "config_1: {{config_1.values.value_1}}";
        let stored_workload = generate_test_stored_workload_spec_with_config(
            AGENT_A,
            RUNTIME,
            templated_runtime_config,
        );

        let workloads = HashMap::from([(WORKLOAD_NAME_1.to_owned(), stored_workload)]);
        let configs = HashMap::default();
        let renderer = ConfigRenderer::default();

        assert!(renderer.render_workloads(&workloads, &configs).is_err());
    }
}
