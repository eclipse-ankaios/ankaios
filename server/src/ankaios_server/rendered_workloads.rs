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

use std::collections::HashMap;

use ankaios_api::ank_base::WorkloadNamed;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedWorkloads(pub HashMap<String, WorkloadNamed>);

impl RenderedWorkloads {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn insert(&mut self, key: String, value: WorkloadNamed) -> Option<WorkloadNamed> {
        self.0.insert(key, value)
    }

    pub fn get(&self, key: &str) -> Option<&WorkloadNamed> {
        self.0.get(key)
    }

    // [impl->swdd~server-state-triggers-validation-of-workload-fields~1]
    pub fn validate(&self) -> Result<(), String> {
        for workload in self.0.values() {
            workload.validate_fields_format()?;
        }
        Ok(())
    }
}

impl std::ops::Deref for RenderedWorkloads {
    type Target = HashMap<String, WorkloadNamed>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for RenderedWorkloads {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for RenderedWorkloads
where
    T: IntoIterator<Item = (String, WorkloadNamed)>,
{
    fn from(pairs: T) -> Self {
        RenderedWorkloads(pairs.into_iter().collect())
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
mod tests {
    use super::RenderedWorkloads;
    use ankaios_api::{
        ank_base::{AccessRightsRuleEnumSpec, WorkloadNamed, WorkloadSpec},
        test_utils::{vars, generate_test_workload, generate_test_workload_instance_name_with_name},
    };

    #[test]
    fn test_rendered_workloads_insert_get() {
        let mut rendered_workloads = RenderedWorkloads::new();
        let workload_name = vars::WORKLOAD_NAMES[0].to_string();
        let workload_named = WorkloadNamed {
            instance_name: generate_test_workload_instance_name_with_name(&workload_name),
            workload: generate_test_workload(),
        };

        rendered_workloads.insert(workload_name.clone(), workload_named.clone());
        let retrieved = rendered_workloads.get(&workload_name);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &workload_named);
    }

    #[test]
    fn test_rendered_workloads_deref() {
        let mut rendered_workloads = RenderedWorkloads::new();
        let workload_name = vars::WORKLOAD_NAMES[0].to_string();
        let workload_named = WorkloadNamed {
            instance_name: generate_test_workload_instance_name_with_name(&workload_name),
            workload: generate_test_workload(),
        };

        rendered_workloads.insert(workload_name.clone(), workload_named.clone());
        let map_ref: &std::collections::HashMap<String, WorkloadNamed> = &rendered_workloads;

        assert_eq!(map_ref.get(&workload_name), Some(&workload_named));
    }

    // The following tests cover the validation logic of RenderedWorkloads and
    // really test as this is easier then to mock WorkloadNamed validation in this context.
    #[test]
    fn test_rendered_workloads_validation_success() {
        let wl_name = "a_valid-Name_1";
        let workload = WorkloadNamed {
            instance_name: generate_test_workload_instance_name_with_name(wl_name),
            workload: generate_test_workload(),
        };

        let mut rendered_workloads = RenderedWorkloads::new();
        rendered_workloads.insert(wl_name.to_string(), workload);

        assert!(rendered_workloads.validate().is_ok());
    }

    #[test]
    fn test_rendered_workloads_validation_failure_on_name() {
        let wl_name = "!nvalid+Name_1";
        let workload = WorkloadNamed {
            instance_name: generate_test_workload_instance_name_with_name(wl_name),
            workload: generate_test_workload(),
        };

        let rendered_workloads = RenderedWorkloads::from([(wl_name.to_string(), workload)]);

        assert_eq!(
            rendered_workloads.validate().unwrap_err(),
            format!(
                "Unsupported workload name '{wl_name}'. Expected to have characters in [a-zA-Z0-9_-]."
            )
        );
    }

    #[test]
    fn test_rendered_workloads_validation_failure_on_agent_name() {
        let wl_name = "valid_Name-1";
        let agent_name = "invalid@Agent#Name";
        let mut workload: WorkloadSpec = generate_test_workload();
        workload.agent = agent_name.to_string();

        let workload_named = WorkloadNamed {
            instance_name: generate_test_workload_instance_name_with_name(wl_name),
            workload,
        };

        let rendered_workloads = RenderedWorkloads::from([(wl_name.to_string(), workload_named)]);

        assert_eq!(
            rendered_workloads.validate().unwrap_err(),
            format!(
                "Unsupported agent name. Received '{agent_name}', expected to have characters in [a-zA-Z0-9_-]"
            )
        );
    }

    #[test]
    fn test_rendered_workloads_validation_failure_on_control_interface_access() {
        let wl_name = "valid_Name-1";
        let mut workload: WorkloadSpec = generate_test_workload();

        if let Some(access_rule) = workload.control_interface_access.allow_rules.first_mut() {
            match &mut access_rule.access_rights_rule_enum {
                AccessRightsRuleEnumSpec::StateRule(rule) => {
                    rule.filter_masks = vec!["".to_string()];
                }
                _ => panic!("Test setup error: expected StateRule variant"),
            }
        }

        let workload_named = WorkloadNamed {
            instance_name: generate_test_workload_instance_name_with_name(wl_name),
            workload,
        };

        let rendered_workloads = RenderedWorkloads::from([(wl_name.to_string(), workload_named)]);

        assert_eq!(
            rendered_workloads.validate().unwrap_err(),
            format!("Empty filter masks are not allowed in Control Interface access rules")
        );
    }
}
