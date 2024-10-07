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

use std::collections::HashMap;

use common::objects::{ConfigItem, StoredWorkloadSpec, WorkloadInstanceName, WorkloadSpec};
use handlebars::Handlebars;

pub type RenderedWorkloads = HashMap<String, WorkloadSpec>;

// [impl->swdd~server-delegate-template-render-to-external-library~1]
pub struct ConfigRenderer {
    template_engine: Handlebars<'static>,
}

impl ConfigRenderer {
    pub fn new() -> Self {
        let mut template_engine = Handlebars::new();
        template_engine.set_strict_mode(true); // enable throwing render errors if context data is valid
        Self { template_engine }
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~1]
    pub fn render_workloads(
        &self,
        workloads: &HashMap<String, StoredWorkloadSpec>,
        configs: &HashMap<String, ConfigItem>,
    ) -> Result<RenderedWorkloads, String> {
        let mut rendered_workloads = HashMap::new();
        for (workload_name, stored_workload) in workloads {
            let wl_config_map = self.create_config_map_for_workload(stored_workload, configs);
            let workload_spec = if wl_config_map.is_empty() {
                log::debug!(
                    "Skipping workload '{}' expansion as no config is available",
                    workload_name
                );
                WorkloadSpec::from((workload_name.to_owned(), stored_workload.clone()))
            } else {
                log::debug!(
                    "Expanding workload '{}' with config '{:?}'",
                    workload_name,
                    wl_config_map
                );
                self.render_workload_fields(workload_name, stored_workload, &wl_config_map)?
            };

            rendered_workloads.insert(workload_name.clone(), workload_spec);
        }
        log::debug!("Expanded state: {:?}", rendered_workloads);
        Ok(rendered_workloads)
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~1]
    fn create_config_map_for_workload<'a>(
        &self,
        workload_spec: &'a StoredWorkloadSpec,
        configs: &'a HashMap<String, ConfigItem>,
    ) -> HashMap<&'a String, &'a ConfigItem> {
        workload_spec.configs.iter().fold(
            HashMap::new(),
            |mut wl_config_map, (config_alias, config_key)| {
                if let Some(config_value) = configs.get(config_key) {
                    wl_config_map.insert(config_key, config_value);
                    wl_config_map.insert(config_alias, config_value);
                }
                wl_config_map
            },
        )
    }

    // [impl->swdd~config-renderer-renders-workload-configuration~1]
    fn render_workload_fields(
        &self,
        workload_name: &str,
        workload: &StoredWorkloadSpec,
        wl_config_map: &HashMap<&String, &ConfigItem>,
    ) -> Result<WorkloadSpec, String> {
        let rendered_runtime_config = self
            .template_engine
            .render_template(&workload.runtime_config, &wl_config_map)
            .map_err(|err| {
                format!(
                    "Failed to expand runtime config template for workload '{}': '{}'",
                    workload_name, err
                )
            })?;

        let rendered_agent_name = self
            .template_engine
            .render_template(&workload.agent, &wl_config_map)
            .map_err(|err| {
                format!(
                    "Failed to expand agent name template for workload '{}': '{}'",
                    workload_name, err
                )
            })?;

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
