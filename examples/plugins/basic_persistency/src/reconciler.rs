// Copyright (c) 2026 Red Hat LLC
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

//! Pure reconciliation logic: turns [`EventEntry`] notifications into explicit
//! persistence [`Action`]s. Performs no I/O; the caller executes the actions
//! against a [`crate::store::PersistenceStore`] and reports the outcome back
//! via [`Reconciler::confirm_persisted`] / [`Reconciler::confirm_removed`].

use std::collections::{HashMap, HashSet};

use ankaios_sdk::{CompleteState, EventEntry, Workload, WorkloadStateEnum};
use serde_yaml::Value;

use crate::persist_mode::PersistMode;

const WORKLOADS_PREFIX: &str = "desiredState.workloads.";

/// An action the caller should perform against the persistence store.
#[derive(Debug)]
pub enum Action {
    /// Persist (or re-persist) a workload definition, together with the configs it references.
    Persist {
        name: String,
        workload: Box<Workload>,
        configs: HashMap<String, Value>,
    },
    /// Remove a previously persisted workload.
    Remove { name: String },
}

/// Tracks `ON_RUNNING` workloads waiting to reach the Running state, and which
/// workloads are currently known to be persisted on disk.
#[derive(Default)]
pub struct Reconciler {
    on_running: HashMap<String, (Workload, HashMap<String, Value>)>,
    pending_retry: HashMap<String, (Workload, HashMap<String, Value>)>,
    persisted_names: HashSet<String>,
}

impl Reconciler {
    pub fn new(persisted_names: HashSet<String>) -> Self {
        Self {
            on_running: HashMap::new(),
            pending_retry: HashMap::new(),
            persisted_names,
        }
    }

    pub fn tracked_on_running_count(&self) -> usize {
        self.on_running.len()
    }

    /// Seeds reconciler state from the startup snapshot and returns the `Persist` actions
    /// needed for workloads whose persist condition is already satisfied at startup — an
    /// `ALWAYS` workload not yet on disk, or an `ON_RUNNING` workload already `Running` —
    /// e.g. because they were applied, or reached Running, before the plugin last started.
    /// `ON_RUNNING` workloads that are not yet running are only tracked, not persisted.
    pub fn seed_from_initial_state(&mut self, state: &CompleteState) -> Vec<Action> {
        let mut actions = Vec::new();

        for workload in state.get_workloads() {
            let name = workload.name.clone();
            if self.persisted_names.contains(&name) {
                continue;
            }

            match PersistMode::from_tags(&workload.get_tags()) {
                Some(PersistMode::Always) => {
                    let configs = resolve_configs(&workload, state);
                    actions.push(Action::Persist {
                        name,
                        workload: Box::new(workload),
                        configs,
                    });
                }
                Some(PersistMode::OnRunning) => {
                    let configs = resolve_configs(&workload, state);
                    if is_running(&name, state) {
                        actions.push(Action::Persist {
                            name,
                            workload: Box::new(workload),
                            configs,
                        });
                    } else {
                        self.on_running.insert(name, (workload, configs));
                    }
                }
                None => {}
            }
        }

        actions
    }

    pub fn reconcile(&mut self, event: &EventEntry) -> Vec<Action> {
        let mut actions = Vec::new();

        // Collect unique workload names first: a single state update can touch several
        // fields of the same workload at once (e.g. `tags` and `runtimeConfig` together),
        // and each workload must only be reconciled once per event.
        let mut changed_names: HashSet<String> = HashSet::new();
        for field in event.added_fields.iter().chain(event.updated_fields.iter()) {
            if let Some(name) = workload_name_from_definition_field(field) {
                changed_names.insert(name);
            }
        }
        for name in &changed_names {
            self.handle_workload_definition_change(name, event, &mut actions);
        }

        for field in &event.removed_fields {
            if let Some(name) = workload_name_from_definition_field(field) {
                self.on_running.remove(&name);
                self.pending_retry.remove(&name);
                if self.persisted_names.contains(&name) {
                    actions.push(Action::Remove { name });
                }
            }
        }

        if !self.on_running.is_empty() {
            let mut state_changed_names: HashSet<String> = HashSet::new();
            for field in event.added_fields.iter().chain(event.updated_fields.iter()) {
                if let Some(name) = workload_name_from_state_field(field) {
                    state_changed_names.insert(name);
                }
            }
            for name in &state_changed_names {
                self.handle_state_transition(name, event, &mut actions);
            }
        }

        actions
    }

    /// Call once a `Persist` action has been successfully written to disk.
    pub fn confirm_persisted(&mut self, name: &str) {
        self.on_running.remove(name);
        self.pending_retry.remove(name);
        self.persisted_names.insert(name.to_owned());
    }

    /// Call once a `Remove` action has been successfully applied to disk.
    pub fn confirm_removed(&mut self, name: &str) {
        self.persisted_names.remove(name);
    }

    /// Call when a `Persist` action failed, so it can be retried later via [`Self::retry_pending`]
    /// instead of silently dropping the workload until its desired state changes again.
    pub fn mark_retry(&mut self, name: &str, workload: Workload, configs: HashMap<String, Value>) {
        self.pending_retry.insert(name.to_owned(), (workload, configs));
    }

    /// Returns `Persist` actions for every workload with a failed persist attempt still pending.
    pub fn retry_pending(&self) -> Vec<Action> {
        self.pending_retry
            .iter()
            .map(|(name, (workload, configs))| Action::Persist {
                name: name.clone(),
                workload: Box::new(workload.clone()),
                configs: configs.clone(),
            })
            .collect()
    }

    fn handle_workload_definition_change(
        &mut self,
        name: &str,
        event: &EventEntry,
        actions: &mut Vec<Action>,
    ) {
        let Some(workload) = event.complete_state.get_workload(name) else {
            log::error!("Workload '{name}' is missing from the event's complete state");
            return;
        };

        match PersistMode::from_tags(&workload.get_tags()) {
            Some(PersistMode::Always) => {
                let configs = resolve_configs(&workload, &event.complete_state);
                actions.push(Action::Persist {
                    name: name.to_owned(),
                    workload: Box::new(workload),
                    configs,
                });
            }
            Some(PersistMode::OnRunning) => {
                let configs = resolve_configs(&workload, &event.complete_state);
                if self.persisted_names.contains(name) {
                    // Already reached Running before; keep the persisted file in sync.
                    actions.push(Action::Persist {
                        name: name.to_owned(),
                        workload: Box::new(workload),
                        configs,
                    });
                } else {
                    self.on_running.insert(name.to_owned(), (workload, configs));
                }
            }
            None => {
                // Tag removed or set to an invalid value: stop persisting this workload.
                self.on_running.remove(name);
                self.pending_retry.remove(name);
                if self.persisted_names.contains(name) {
                    actions.push(Action::Remove {
                        name: name.to_owned(),
                    });
                }
            }
        }
    }

    fn handle_state_transition(&mut self, name: &str, event: &EventEntry, actions: &mut Vec<Action>) {
        if self.persisted_names.contains(name) {
            return;
        }
        let Some((workload, configs)) = self.on_running.get(name) else {
            return;
        };
        if is_running(name, &event.complete_state) {
            actions.push(Action::Persist {
                name: name.to_owned(),
                workload: Box::new(workload.clone()),
                configs: configs.clone(),
            });
        }
        // Intentionally do not stop tracking on Failed/Succeeded: a workload using the
        // ON_FAILURE/ALWAYS restart policy may still reach Running later. Tracking only
        // ends when the workload is removed from desired state (handled above) or once
        // persistence succeeds (confirm_persisted).
    }
}

/// Resolves the actual values of the configs referenced by a workload, keyed by the
/// global config name (as used in a manifest's top-level `configs:` block).
fn resolve_configs(workload: &Workload, state: &CompleteState) -> HashMap<String, Value> {
    let global_configs = state.get_configs();
    workload
        .get_configs()
        .into_values()
        .filter_map(|config_name| {
            global_configs
                .get(&config_name)
                .map(|value| (config_name.clone(), value.clone()))
        })
        .collect()
}

fn is_running(workload_name: &str, state: &CompleteState) -> bool {
    state.get_workload_states().as_list().into_iter().any(|ws| {
        ws.workload_instance_name.workload_name == workload_name
            && ws.execution_state.state == WorkloadStateEnum::Running
    })
}

/// Extracts the workload name from a top-level or nested `desiredState.workloads.<name>[...]` field.
fn workload_name_from_definition_field(field: &str) -> Option<String> {
    let rest = field.strip_prefix(WORKLOADS_PREFIX)?;
    rest.split('.').next().map(str::to_owned)
}

/// Extracts the workload name from a workload-state field, e.g.
/// `workloadStates.<agent>.<name>.<id>.state`.
fn workload_name_from_state_field(field: &str) -> Option<String> {
    if !field.starts_with("workloadStates.") || !field.ends_with(".state") {
        return None;
    }
    field.split('.').nth(2).map(str::to_owned)
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
    use ankaios_sdk::Workload as SdkWorkload;
    use std::collections::HashMap;

    fn workload_with_tag(name: &str, agent: &str, persist: &str) -> SdkWorkload {
        SdkWorkload::builder()
            .workload_name(name)
            .agent_name(agent)
            .runtime("podman")
            .runtime_config("image: nginx")
            .add_tag("persist", persist)
            .build()
            .expect("valid workload")
    }

    /// Builds a `CompleteState` with a single workload, via a manifest YAML string
    /// (mirrors how the SDK is actually used; `CompleteState::new_from_workloads` is
    /// crate-private).
    fn complete_state_with_workload(name: &str, agent: &str, persist: Option<&str>) -> CompleteState {
        let tags = match persist {
            Some(mode) => format!("\n    tags:\n      persist: {mode}"),
            None => String::new(),
        };
        let yaml = format!(
            "apiVersion: v1\nworkloads:\n  {name}:\n    runtime: podman\n    agent: {agent}\n    runtimeConfig: |\n      image: nginx{tags}\n"
        );
        let manifest = ankaios_sdk::Manifest::from_string(yaml).expect("valid manifest");
        CompleteState::new_from_manifest(manifest)
    }

    fn event_for_added(name: &str, state: CompleteState) -> EventEntry {
        EventEntry {
            complete_state: state,
            added_fields: vec![format!("desiredState.workloads.{name}")],
            updated_fields: vec![],
            removed_fields: vec![],
        }
    }

    #[test]
    fn utest_always_tag_persists_immediately() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));
        let event = event_for_added("nginx", state);

        let actions = reconciler.reconcile(&event);

        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Persist { name, .. } if name == "nginx"));
    }

    #[test]
    fn utest_on_running_tag_defers_until_running() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ON_RUNNING"));
        let event = event_for_added("nginx", state);

        let actions = reconciler.reconcile(&event);

        assert!(actions.is_empty());
        assert_eq!(reconciler.tracked_on_running_count(), 1);
    }

    #[test]
    fn utest_removing_persist_tag_removes_persisted_file() {
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(persisted);

        let state = complete_state_with_workload("nginx", "agent_A", None);
        let event = EventEntry {
            complete_state: state,
            added_fields: vec![],
            updated_fields: vec!["desiredState.workloads.nginx.tags".to_owned()],
            removed_fields: vec![],
        };

        let actions = reconciler.reconcile(&event);

        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Remove { name } if name == "nginx"));
    }

    #[test]
    fn utest_removed_workload_cleans_up_persisted_file() {
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(persisted);

        let event = EventEntry {
            complete_state: CompleteState::default(),
            added_fields: vec![],
            updated_fields: vec![],
            removed_fields: vec!["desiredState.workloads.nginx".to_owned()],
        };

        let actions = reconciler.reconcile(&event);

        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Remove { name } if name == "nginx"));
    }

    #[test]
    fn utest_failed_state_does_not_stop_on_running_tracking() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let workload = workload_with_tag("flaky", "agent_A", "ON_RUNNING");
        reconciler.on_running.insert(
            "flaky".to_owned(),
            (workload, HashMap::new()),
        );

        let state = complete_state_with_workload("flaky", "agent_A", Some("ON_RUNNING"));
        let event = EventEntry {
            complete_state: state,
            added_fields: vec!["workloadStates.agent_A.flaky.id1.state".to_owned()],
            updated_fields: vec![],
            removed_fields: vec![],
        };

        let actions = reconciler.reconcile(&event);

        assert!(actions.is_empty());
        assert_eq!(reconciler.tracked_on_running_count(), 1);
    }

    #[test]
    fn utest_confirm_persisted_stops_on_running_tracking() {
        let mut reconciler = Reconciler::new(HashSet::new());
        reconciler.on_running.insert(
            "nginx".to_owned(),
            (
                workload_with_tag("nginx", "agent_A", "ON_RUNNING"),
                HashMap::new(),
            ),
        );

        reconciler.confirm_persisted("nginx");

        assert_eq!(reconciler.tracked_on_running_count(), 0);
        assert!(reconciler.persisted_names.contains("nginx"));
    }

    #[test]
    fn utest_seed_persists_unpersisted_always_workload() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));

        let actions = reconciler.seed_from_initial_state(&state);

        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Persist { name, .. } if name == "nginx"));
    }

    #[test]
    fn utest_seed_skips_already_persisted_always_workload() {
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(persisted);
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));

        let actions = reconciler.seed_from_initial_state(&state);

        assert!(actions.is_empty());
    }

    #[test]
    fn utest_seed_tracks_not_yet_running_on_running_workload() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ON_RUNNING"));

        let actions = reconciler.seed_from_initial_state(&state);

        assert!(actions.is_empty());
        assert_eq!(reconciler.tracked_on_running_count(), 1);
    }

    #[test]
    fn utest_seed_skips_already_persisted_on_running_workload() {
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(persisted);
        let state = complete_state_with_workload("nginx", "agent_A", Some("ON_RUNNING"));

        let actions = reconciler.seed_from_initial_state(&state);

        assert!(actions.is_empty());
        assert_eq!(reconciler.tracked_on_running_count(), 0);
    }

    #[test]
    fn utest_reconcile_dedups_multiple_fields_for_same_workload() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));
        let event = EventEntry {
            complete_state: state,
            added_fields: vec![],
            updated_fields: vec![
                "desiredState.workloads.nginx.tags".to_owned(),
                "desiredState.workloads.nginx.runtimeConfig".to_owned(),
            ],
            removed_fields: vec![],
        };

        let actions = reconciler.reconcile(&event);

        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn utest_mark_retry_then_confirm_persisted_clears_pending_retry() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let workload = workload_with_tag("nginx", "agent_A", "ALWAYS");

        reconciler.mark_retry("nginx", workload, HashMap::new());
        assert_eq!(reconciler.retry_pending().len(), 1);

        reconciler.confirm_persisted("nginx");

        assert!(reconciler.retry_pending().is_empty());
    }

    #[test]
    fn utest_removed_workload_clears_pending_retry() {
        let mut reconciler = Reconciler::new(HashSet::new());
        let workload = workload_with_tag("nginx", "agent_A", "ALWAYS");
        reconciler.mark_retry("nginx", workload, HashMap::new());

        let event = EventEntry {
            complete_state: CompleteState::default(),
            added_fields: vec![],
            updated_fields: vec![],
            removed_fields: vec!["desiredState.workloads.nginx".to_owned()],
        };
        reconciler.reconcile(&event);

        assert!(reconciler.retry_pending().is_empty());
    }
}
