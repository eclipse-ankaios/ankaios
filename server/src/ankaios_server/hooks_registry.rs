// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

use grpc::UpdateWorkload;
use prost::Message;

use crate::ankaios_server::rendered_workloads::RenderedWorkloads;
use crate::ankaios_server::server_state::{AddedDeletedWorkloads, UpdateStateError};
use crate::server_config::MutatingHook;

#[cfg_attr(test, mockall::automock)]
pub trait HookMutator {
    fn mutate_with_hooks(
        &self,
        added_deleted_workloads: &mut AddedDeletedWorkloads,
    ) -> Result<(), UpdateStateError>;
}

#[derive(Debug, Default)]
pub struct HooksRegistry {
    mutating_hooks: Vec<MutatingHook>,
}

impl HooksRegistry {
    // [impl->swdd~server-sorts-mutating-hooks-by-priority~1]
    pub fn new(mut mutating_hooks: Vec<MutatingHook>) -> Self {
        log::info!(
            "Initializing HooksRegistry with mutating hooks: {:?}",
            mutating_hooks
        );

        mutating_hooks.sort_by_key(|h| h.prio);
        HooksRegistry { mutating_hooks }
    }
}

impl HookMutator for HooksRegistry {
    // [impl->swdd~server-executes-mutating-hooks-as-subprocess~1]
    fn mutate_with_hooks(
        &self,
        added_deleted_workloads: &mut AddedDeletedWorkloads,
    ) -> Result<(), UpdateStateError> {
        for hook in &self.mutating_hooks {
            log::debug!("Running mutating hook '{}' (prio={})", hook.name, hook.prio);

            let proto_msg = UpdateWorkload::from(added_deleted_workloads.clone());
            let input_bytes = proto_msg.encode_to_vec();

            let executable = hook.path.join(&hook.name);
            let mut child = Command::new(&executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|err| UpdateStateError::MutatingHookError {
                    hook: hook.name.clone(),
                    reason: format!("Failed to execute hook '{}': {err}", executable.display()),
                })?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&input_bytes).map_err(|err| {
                    UpdateStateError::MutatingHookError {
                        hook: hook.name.clone(),
                        reason: format!("Failed to write to hook '{}' stdin: {err}", executable.display()),
                    }
                })?;
            }

            let output =
                child
                    .wait_with_output()
                    .map_err(|err| UpdateStateError::MutatingHookError {
                        hook: hook.name.clone(),
                        reason: format!("Failed to wait for hook '{}': {err}", executable.display()),
                    })?;

            // [impl->swdd~server-mutating-hook-veto~1]
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // [impl->swdd~server-traces-mutating-hook-stderr~1]
                log::debug!(
                    "Mutating hook '{}' failed with stderr: {}",
                    hook.name,
                    stderr.trim()
                );
                return Err(UpdateStateError::MutatingHookVeto {
                    hook: hook.name.clone(),
                    reason: format!(
                        "Hook '{}' rejected the change: {}",
                        hook.name,
                        stderr.trim()
                    ),
                });
            }

            // [impl->swdd~server-traces-mutating-hook-stderr~1]
            log::debug!(
                "Mutating hook '{}' completed successfully. Output: '{}'",
                hook.name,
                String::from_utf8_lossy(&output.stderr).trim()
            );

            // [impl->swdd~server-mutating-hook-skip-unchanged-response~1]
            if output.stdout != input_bytes {
                let mutated_proto =
                    UpdateWorkload::decode(output.stdout.as_slice()).map_err(|err| {
                        UpdateStateError::ResultInvalid(format!(
                            "Failed to decode protobuf output from hook '{}': {err}",
                            hook.name
                        ))
                    })?;

                *added_deleted_workloads =
                    AddedDeletedWorkloads::try_from(mutated_proto).map_err(|err| {
                        UpdateStateError::ResultInvalid(format!(
                            "Failed to convert output from hook '{}': {err}",
                            hook.name
                        ))
                    })?;
            }
        }

        Ok(())
    }
}

// [impl->swdd~server-applies-mutated-workloads-to-effective-state~1]
// [impl->swdd~server-removes-dropped-workloads-from-effective-state~1]
// [impl->swdd~server-restores-undeleted-workloads-in-effective-state~1]
pub fn update_effective_state(
    hooks_registry: &impl HookMutator,
    added_deleted_workloads: &mut AddedDeletedWorkloads,
    new_rendered_workloads: &mut RenderedWorkloads,
    old_rendered_workloads: &RenderedWorkloads,
) -> Result<(), UpdateStateError> {
    let original_added_names: HashSet<String> = added_deleted_workloads
        .added_workloads
        .iter()
        .map(|w| w.instance_name.workload_name().to_owned())
        .collect();
    let original_deleted_names: HashSet<String> = added_deleted_workloads
        .deleted_workloads
        .iter()
        .map(|w| w.instance_name.workload_name().to_owned())
        .collect();

    hooks_registry.mutate_with_hooks(added_deleted_workloads)?;

    let mutated_added_names: HashSet<String> = added_deleted_workloads
        .added_workloads
        .iter()
        .map(|w| w.instance_name.workload_name().to_owned())
        .collect();
    let mutated_deleted_names: HashSet<String> = added_deleted_workloads
        .deleted_workloads
        .iter()
        .map(|w| w.instance_name.workload_name().to_owned())
        .collect();

    // Apply mutated added workloads to effective state
    for wl_named in &added_deleted_workloads.added_workloads {
        let name = wl_named.instance_name.workload_name().to_owned();
        new_rendered_workloads.insert(name, wl_named.clone());
    }

    // Workloads removed from added_workloads by hooks -> remove from effective state
    for name in original_added_names.difference(&mutated_added_names) {
        log::debug!(
            "Mutating hook removed workload '{}' from added workloads, removing from effective state",
            name
        );
        new_rendered_workloads.remove(name);
    }

    // Workloads removed from deleted_workloads by hooks -> re-add from old rendered workloads
    for name in original_deleted_names.difference(&mutated_deleted_names) {
        if let Some(old_workload) = old_rendered_workloads.get(name) {
            log::debug!(
                "Mutating hook removed workload '{}' from deleted workloads, re-adding to effective state",
                name
            );
            new_rendered_workloads.insert(name.clone(), old_workload.clone());
        }
    }

    Ok(())
}

impl From<AddedDeletedWorkloads> for UpdateWorkload {
    fn from(adw: AddedDeletedWorkloads) -> Self {
        UpdateWorkload {
            added_workloads: adw.added_workloads.into_iter().map(Into::into).collect(),
            deleted_workloads: adw.deleted_workloads.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<UpdateWorkload> for AddedDeletedWorkloads {
    type Error = String;

    fn try_from(uw: UpdateWorkload) -> Result<Self, Self::Error> {
        Ok(AddedDeletedWorkloads {
            added_workloads: uw
                .added_workloads
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            deleted_workloads: uw
                .deleted_workloads
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
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
mod tests {
    use super::*;
    use ankaios_api::test_utils::{
        generate_test_deleted_workload, generate_test_deleted_workload_with_params,
        generate_test_workload_named, generate_test_workload_named_with_params,
    };
    use std::path::PathBuf;

    // [utest->swdd~server-sorts-mutating-hooks-by-priority~1]
    #[test]
    fn utest_registry_from_empty_config() {
        let registry = HooksRegistry::new(vec![]);
        assert!(registry.mutating_hooks.is_empty());
    }

    // [utest->swdd~server-sorts-mutating-hooks-by-priority~1]
    #[test]
    fn utest_registry_sorts_by_priority() {
        let configs = vec![
            MutatingHook {
                name: "hook-c".to_string(),
                prio: 30,
                path: PathBuf::from("/usr/libexec/ankaios/hooks"),
            },
            MutatingHook {
                name: "hook-a".to_string(),
                prio: 10,
                path: PathBuf::from("/usr/libexec/ankaios/hooks"),
            },
            MutatingHook {
                name: "hook-b".to_string(),
                prio: 20,
                path: PathBuf::from("/usr/libexec/ankaios/hooks"),
            },
        ];

        let registry = HooksRegistry::new(configs);

        assert_eq!(registry.mutating_hooks.len(), 3);
        assert_eq!(registry.mutating_hooks[0].name, "hook-a");
        assert_eq!(registry.mutating_hooks[0].prio, 10);
        assert_eq!(registry.mutating_hooks[1].name, "hook-b");
        assert_eq!(registry.mutating_hooks[1].prio, 20);
        assert_eq!(registry.mutating_hooks[2].name, "hook-c");
        assert_eq!(registry.mutating_hooks[2].prio, 30);
    }

    // [utest->swdd~server-config-supports-mutating-hooks~1]
    #[test]
    fn utest_registry_uses_custom_hooks_dir() {
        let configs = vec![MutatingHook {
            name: "my-hook".to_string(),
            prio: 5,
            path: PathBuf::from("/opt/hooks"),
        }];

        let registry = HooksRegistry::new(configs);

        assert_eq!(registry.mutating_hooks[0].path, PathBuf::from("/opt/hooks"));
    }

    // [utest->swdd~server-sorts-mutating-hooks-by-priority~1]
    #[test]
    fn utest_registry_stable_sort_equal_priorities() {
        let configs = vec![
            MutatingHook {
                name: "first".to_string(),
                prio: 10,
                path: PathBuf::from("/usr/libexec/ankaios/hooks"),
            },
            MutatingHook {
                name: "second".to_string(),
                prio: 10,
                path: PathBuf::from("/usr/libexec/ankaios/hooks"),
            },
        ];

        let registry = HooksRegistry::new(configs);

        assert_eq!(registry.mutating_hooks[0].name, "first");
        assert_eq!(registry.mutating_hooks[1].name, "second");
    }

    // [utest->swdd~server-executes-mutating-hooks-as-subprocess~1]
    #[test]
    fn utest_update_workload_roundtrip_conversion() {
        let original = AddedDeletedWorkloads {
            added_workloads: vec![generate_test_workload_named()],
            deleted_workloads: vec![generate_test_deleted_workload()],
        };

        let proto: UpdateWorkload = original.clone().into();
        let restored: AddedDeletedWorkloads = proto.try_into().unwrap();

        assert_eq!(original, restored);
    }

    // [utest->swdd~server-applies-mutated-workloads-to-effective-state~1]
    #[test]
    fn utest_update_effective_state_no_mutation_applies_added_workloads() {
        let wl = generate_test_workload_named_with_params("wl_a", "agent_A", "runtime_A");

        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(|_| Ok(()));

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![wl.clone()],
            deleted_workloads: vec![],
        };
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::default();

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert_eq!(new_rendered.get("wl_a").unwrap(), &wl);
    }

    // [utest->swdd~server-applies-mutated-workloads-to-effective-state~1]
    #[test]
    fn utest_update_effective_state_no_mutation_empty_input() {
        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(|_| Ok(()));

        let mut added_deleted = AddedDeletedWorkloads::default();
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::default();

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert!(new_rendered.is_empty());
    }

    // [utest->swdd~server-applies-mutated-workloads-to-effective-state~1]
    #[test]
    fn utest_update_effective_state_passthrough_no_change() {
        let wl = generate_test_workload_named_with_params("wl_x", "agent_A", "runtime_A");

        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(|_| Ok(()));

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![wl.clone()],
            deleted_workloads: vec![],
        };
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::default();

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert_eq!(new_rendered.get("wl_x").unwrap(), &wl);
    }

    // [utest->swdd~server-removes-dropped-workloads-from-effective-state~1]
    #[test]
    fn utest_update_effective_state_hook_removes_from_added_removes_from_effective() {
        let wl_a = generate_test_workload_named_with_params("wl_a", "agent_A", "runtime_A");
        let wl_b = generate_test_workload_named_with_params("wl_b", "agent_A", "runtime_A");
        let wl_b_clone = wl_b.clone();

        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(move |adw| {
            // Hook removes wl_a, keeps only wl_b
            adw.added_workloads = vec![wl_b_clone.clone()];
            Ok(())
        });

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![wl_a.clone(), wl_b.clone()],
            deleted_workloads: vec![],
        };
        let mut new_rendered = RenderedWorkloads::from(vec![
            ("wl_a".to_string(), wl_a),
            ("wl_b".to_string(), wl_b.clone()),
        ]);
        let old_rendered = RenderedWorkloads::default();

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert!(
            new_rendered.get("wl_a").is_none(),
            "wl_a should be removed from effective state"
        );
        assert_eq!(
            new_rendered.get("wl_b").unwrap(),
            &wl_b,
            "wl_b should remain in effective state"
        );
    }

    // [utest->swdd~server-restores-undeleted-workloads-in-effective-state~1]
    #[test]
    fn utest_update_effective_state_hook_removes_from_deleted_readds_from_old() {
        let wl_keep = generate_test_workload_named_with_params("wl_keep", "agent_A", "runtime_A");
        let del_wl = generate_test_deleted_workload_with_params("agent_A", "wl_keep");

        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(|adw| {
            // Hook removes wl_keep from deleted list
            adw.deleted_workloads.clear();
            Ok(())
        });

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![],
            deleted_workloads: vec![del_wl],
        };
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::from(vec![("wl_keep".to_string(), wl_keep.clone())]);

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert_eq!(
            new_rendered.get("wl_keep").unwrap(),
            &wl_keep,
            "wl_keep should be re-added from old rendered workloads"
        );
    }

    // [utest->swdd~server-restores-undeleted-workloads-in-effective-state~1]
    #[test]
    fn utest_update_effective_state_hook_removes_from_deleted_not_in_old_no_crash() {
        let del_wl = generate_test_deleted_workload_with_params("agent_A", "wl_gone");

        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(|adw| {
            // Hook removes wl_gone from deleted list
            adw.deleted_workloads.clear();
            Ok(())
        });

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![],
            deleted_workloads: vec![del_wl],
        };
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::default();

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert!(
            new_rendered.is_empty(),
            "no workload should be added when not found in old rendered"
        );
    }

    // [utest->swdd~server-applies-mutated-workloads-to-effective-state~1]
    #[test]
    fn utest_update_effective_state_hook_adds_new_workload_not_in_old() {
        let wl_orig = generate_test_workload_named_with_params("wl_orig", "agent_A", "runtime_A");
        let wl_new = generate_test_workload_named_with_params("wl_new", "agent_A", "runtime_A");
        let wl_orig_clone = wl_orig.clone();
        let wl_new_clone = wl_new.clone();

        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(move |adw| {
            // Hook adds wl_new alongside wl_orig
            adw.added_workloads = vec![wl_orig_clone.clone(), wl_new_clone.clone()];
            Ok(())
        });

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![wl_orig.clone()],
            deleted_workloads: vec![],
        };
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::default();

        update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered)
            .unwrap();

        assert_eq!(
            new_rendered.get("wl_orig").unwrap(),
            &wl_orig,
            "original workload should be in effective state"
        );
        assert_eq!(
            new_rendered.get("wl_new").unwrap(),
            &wl_new,
            "hook-injected workload should be added to effective state"
        );
    }

    // [utest->swdd~server-mutating-hook-veto~1]
    #[test]
    fn utest_update_effective_state_hook_veto_propagates_error() {
        let mut mock = MockHookMutator::new();
        mock.expect_mutate_with_hooks().returning(|_| {
            Err(UpdateStateError::MutatingHookVeto {
                hook: "veto-hook".to_string(),
                reason: "policy violation".to_string(),
            })
        });

        let mut added_deleted = AddedDeletedWorkloads {
            added_workloads: vec![generate_test_workload_named()],
            deleted_workloads: vec![],
        };
        let mut new_rendered = RenderedWorkloads::default();
        let old_rendered = RenderedWorkloads::default();

        let result =
            update_effective_state(&mock, &mut added_deleted, &mut new_rendered, &old_rendered);

        assert!(matches!(
            result,
            Err(UpdateStateError::MutatingHookVeto { .. })
        ));
    }
}
