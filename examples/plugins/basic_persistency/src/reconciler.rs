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

//! Turns [`EventEntry`] notifications into persistence changes, applying them to a
//! [`PersistenceStore`] it owns. A failed write (persist or remove) is kept in a single
//! pending-actions map and retried via [`Reconciler::retry_pending`] instead of being
//! dropped until the workload's desired state happens to change again.

use std::collections::{HashMap, HashSet};

use ankaios_sdk::{CompleteState, EventEntry, Workload, WorkloadStateEnum};
use serde_yaml::Value;

use crate::persist_mode::PersistMode;
use crate::store::PersistenceStore;

const WORKLOADS_PREFIX: &str = "desiredState.workloads.";

/// A write that failed and is waiting to be retried via [`Reconciler::retry_pending`].
enum PendingAction {
    Persist {
        workload: Box<Workload>,
        configs: HashMap<String, Value>,
    },
    Remove,
}

/// Owns the persistence store and reconciles it against the desired/runtime state reported
/// by Ankaios events.
pub struct Reconciler {
    store: PersistenceStore,
    on_running: HashMap<String, (Workload, HashMap<String, Value>)>,
    pending: HashMap<String, PendingAction>,
    persisted_names: HashSet<String>,
}

impl Reconciler {
    pub fn new(store: PersistenceStore, persisted_names: HashSet<String>) -> Self {
        Self {
            store,
            on_running: HashMap::new(),
            pending: HashMap::new(),
            persisted_names,
        }
    }

    pub fn tracked_on_running_count(&self) -> usize {
        self.on_running.len()
    }

    #[cfg(test)]
    fn is_persisted(&self, name: &str) -> bool {
        self.persisted_names.contains(name)
    }

    /// Seeds reconciler state from the startup snapshot, persisting workloads whose persist
    /// condition is already satisfied — an `ALWAYS` workload not yet on disk, or an
    /// `ON_RUNNING` workload already `Running` — e.g. because they were applied, or reached
    /// Running, before the plugin last started. `ON_RUNNING` workloads that are not yet
    /// running are only tracked, not persisted.
    pub async fn seed(&mut self, state: &CompleteState) {
        let mut to_persist = Vec::new();

        for workload in state.get_workloads() {
            let name = workload.name.clone();
            if self.persisted_names.contains(&name) {
                continue;
            }

            match PersistMode::from_tags(&workload.get_tags()) {
                Some(PersistMode::Always) => {
                    let configs = resolve_configs(&workload, state);
                    to_persist.push((name, workload, configs));
                }
                Some(PersistMode::OnRunning) => {
                    let configs = resolve_configs(&workload, state);
                    if is_running(&name, state) {
                        to_persist.push((name, workload, configs));
                    } else {
                        self.on_running.insert(name, (workload, configs));
                    }
                }
                None => {}
            }
        }

        for (name, workload, configs) in to_persist {
            self.persist_workload(&name, workload, configs).await;
        }
    }

    pub async fn reconcile(&mut self, event: &EventEntry) {
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
            self.handle_workload_definition_change(name, event).await;
        }

        let mut removed_names: HashSet<String> = HashSet::new();
        for field in &event.removed_fields {
            if let Some(name) = workload_name_from_definition_field(field) {
                removed_names.insert(name);
            }
        }
        for name in &removed_names {
            self.on_running.remove(name);
            self.pending.remove(name);
            if self.persisted_names.contains(name) {
                self.remove_workload(name).await;
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
                self.handle_state_transition(name, event).await;
            }
        }
    }

    /// Retries every write (persist or remove) that previously failed.
    pub async fn retry_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for (name, action) in pending {
            match action {
                PendingAction::Persist { workload, configs } => {
                    self.persist_workload(&name, *workload, configs).await;
                }
                PendingAction::Remove => self.remove_workload(&name).await,
            }
        }
    }

    async fn handle_workload_definition_change(&mut self, name: &str, event: &EventEntry) {
        let Some(workload) = event.complete_state.get_workload(name) else {
            log::error!("Workload '{name}' is missing from the event's complete state");
            return;
        };

        match PersistMode::from_tags(&workload.get_tags()) {
            Some(PersistMode::Always) => {
                let configs = resolve_configs(&workload, &event.complete_state);
                self.persist_workload(name, workload, configs).await;
            }
            Some(PersistMode::OnRunning) => {
                let configs = resolve_configs(&workload, &event.complete_state);
                if self.persisted_names.contains(name) {
                    // Already reached Running before; keep the persisted file in sync.
                    self.persist_workload(name, workload, configs).await;
                } else {
                    self.on_running.insert(name.to_owned(), (workload, configs));
                }
            }
            None => {
                // Tag removed or set to an invalid value: stop persisting this workload.
                self.on_running.remove(name);
                self.pending.remove(name);
                if self.persisted_names.contains(name) {
                    self.remove_workload(name).await;
                }
            }
        }
    }

    async fn handle_state_transition(&mut self, name: &str, event: &EventEntry) {
        if self.persisted_names.contains(name) {
            return;
        }
        let Some((workload, configs)) = self.on_running.get(name) else {
            return;
        };
        if is_running(name, &event.complete_state) {
            let (workload, configs) = (workload.clone(), configs.clone());
            self.persist_workload(name, workload, configs).await;
        }
        // Intentionally do not stop tracking on Failed/Succeeded: a workload using the
        // ON_FAILURE/ALWAYS restart policy may still reach Running later. Tracking only
        // ends when the workload is removed from desired state (handled above) or once
        // persistence succeeds.
    }

    /// Writes a workload to the store, updating tracking on success or queuing a retry
    /// (see [`Self::retry_pending`]) on failure.
    async fn persist_workload(&mut self, name: &str, workload: Workload, configs: HashMap<String, Value>) {
        match self.store.persist(name, &workload, &configs).await {
            Ok(()) => {
                self.on_running.remove(name);
                self.pending.remove(name);
                self.persisted_names.insert(name.to_owned());
            }
            Err(e) => {
                log::error!("Failed to persist workload '{name}': {e}, will retry");
                self.pending.insert(
                    name.to_owned(),
                    PendingAction::Persist {
                        workload: Box::new(workload),
                        configs,
                    },
                );
            }
        }
    }

    /// Removes a persisted workload from the store, updating tracking on success or queuing
    /// a retry (see [`Self::retry_pending`]) on failure.
    async fn remove_workload(&mut self, name: &str) {
        match self.store.remove(name).await {
            Ok(()) => {
                self.persisted_names.remove(name);
                self.pending.remove(name);
            }
            Err(e) => {
                log::error!("Failed to remove persisted workload '{name}': {e}, will retry");
                self.pending.insert(name.to_owned(), PendingAction::Remove);
            }
        }
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

    fn test_store() -> (tempfile::TempDir, PersistenceStore) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = PersistenceStore::new(temp_dir.path());
        (temp_dir, store)
    }

    fn persisted_file(temp_dir: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
        temp_dir.path().join("workloads").join(format!("{name}.yaml"))
    }

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

    #[tokio::test]
    async fn utest_always_tag_persists_immediately() {
        let (temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));
        let event = event_for_added("nginx", state);

        reconciler.reconcile(&event).await;

        assert!(persisted_file(&temp_dir, "nginx").exists());
        assert!(reconciler.is_persisted("nginx"));
    }

    #[tokio::test]
    async fn utest_on_running_tag_defers_until_running() {
        let (temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ON_RUNNING"));
        let event = event_for_added("nginx", state);

        reconciler.reconcile(&event).await;

        assert!(!persisted_file(&temp_dir, "nginx").exists());
        assert_eq!(reconciler.tracked_on_running_count(), 1);
    }

    #[tokio::test]
    async fn utest_removing_persist_tag_removes_persisted_file() {
        let (temp_dir, store) = test_store();
        store
            .persist("nginx", &workload_with_tag("nginx", "agent_A", "ALWAYS"), &HashMap::new())
            .await
            .unwrap();
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(store, persisted);

        let state = complete_state_with_workload("nginx", "agent_A", None);
        let event = EventEntry {
            complete_state: state,
            added_fields: vec![],
            updated_fields: vec!["desiredState.workloads.nginx.tags".to_owned()],
            removed_fields: vec![],
        };

        reconciler.reconcile(&event).await;

        assert!(!persisted_file(&temp_dir, "nginx").exists());
        assert!(!reconciler.is_persisted("nginx"));
    }

    #[tokio::test]
    async fn utest_removed_workload_cleans_up_persisted_file() {
        let (temp_dir, store) = test_store();
        store
            .persist("nginx", &workload_with_tag("nginx", "agent_A", "ALWAYS"), &HashMap::new())
            .await
            .unwrap();
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(store, persisted);

        let event = EventEntry {
            complete_state: CompleteState::default(),
            added_fields: vec![],
            updated_fields: vec![],
            removed_fields: vec!["desiredState.workloads.nginx".to_owned()],
        };

        reconciler.reconcile(&event).await;

        assert!(!persisted_file(&temp_dir, "nginx").exists());
        assert!(!reconciler.is_persisted("nginx"));
    }

    #[tokio::test]
    async fn utest_failed_state_does_not_stop_on_running_tracking() {
        let (_temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        let workload = workload_with_tag("flaky", "agent_A", "ON_RUNNING");
        reconciler.on_running.insert("flaky".to_owned(), (workload, HashMap::new()));

        let state = complete_state_with_workload("flaky", "agent_A", Some("ON_RUNNING"));
        let event = EventEntry {
            complete_state: state,
            added_fields: vec!["workloadStates.agent_A.flaky.id1.state".to_owned()],
            updated_fields: vec![],
            removed_fields: vec![],
        };

        reconciler.reconcile(&event).await;

        assert_eq!(reconciler.tracked_on_running_count(), 1);
    }

    #[tokio::test]
    async fn utest_seed_persists_unpersisted_always_workload() {
        let (temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));

        reconciler.seed(&state).await;

        assert!(persisted_file(&temp_dir, "nginx").exists());
    }

    #[tokio::test]
    async fn utest_seed_skips_already_persisted_always_workload() {
        let (temp_dir, store) = test_store();
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(store, persisted);
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));

        reconciler.seed(&state).await;

        // Nothing was written by seed(); the file only exists if this test wrote it.
        assert!(!persisted_file(&temp_dir, "nginx").exists());
    }

    #[tokio::test]
    async fn utest_seed_tracks_not_yet_running_on_running_workload() {
        let (_temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ON_RUNNING"));

        reconciler.seed(&state).await;

        assert_eq!(reconciler.tracked_on_running_count(), 1);
    }

    #[tokio::test]
    async fn utest_seed_skips_already_persisted_on_running_workload() {
        let (_temp_dir, store) = test_store();
        let mut persisted = HashSet::new();
        persisted.insert("nginx".to_owned());
        let mut reconciler = Reconciler::new(store, persisted);
        let state = complete_state_with_workload("nginx", "agent_A", Some("ON_RUNNING"));

        reconciler.seed(&state).await;

        assert_eq!(reconciler.tracked_on_running_count(), 0);
    }

    #[tokio::test]
    async fn utest_reconcile_dedups_multiple_fields_for_same_workload() {
        let (temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
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

        reconciler.reconcile(&event).await;

        // A single persist overwrites the same file; this just confirms reconcile()
        // doesn't error out or panic when handling duplicate fields for one workload.
        assert!(persisted_file(&temp_dir, "nginx").exists());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn utest_persist_failure_is_retried() {
        let (temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        let state = complete_state_with_workload("nginx", "agent_A", Some("ALWAYS"));
        let event = event_for_added("nginx", state);

        // Make the workloads directory read-only so the write fails, then restore it.
        use std::os::unix::fs::PermissionsExt;

        let workloads_dir = temp_dir.path().join("workloads");
        tokio::fs::create_dir_all(&workloads_dir).await.unwrap();
        let original_perms = tokio::fs::metadata(&workloads_dir).await.unwrap().permissions();
        tokio::fs::set_permissions(&workloads_dir, std::fs::Permissions::from_mode(0o500))
            .await
            .unwrap();

        reconciler.reconcile(&event).await;
        assert!(!reconciler.is_persisted("nginx"));

        tokio::fs::set_permissions(&workloads_dir, original_perms).await.unwrap();

        reconciler.retry_pending().await;

        assert!(persisted_file(&temp_dir, "nginx").exists());
        assert!(reconciler.is_persisted("nginx"));
    }

    #[tokio::test]
    async fn utest_removed_workload_clears_pending_retry() {
        let (_temp_dir, store) = test_store();
        let mut reconciler = Reconciler::new(store, HashSet::new());
        reconciler.pending.insert(
            "nginx".to_owned(),
            PendingAction::Persist {
                workload: Box::new(workload_with_tag("nginx", "agent_A", "ALWAYS")),
                configs: HashMap::new(),
            },
        );

        let event = EventEntry {
            complete_state: CompleteState::default(),
            added_fields: vec![],
            updated_fields: vec![],
            removed_fields: vec!["desiredState.workloads.nginx".to_owned()],
        };
        reconciler.reconcile(&event).await;

        assert!(reconciler.pending.is_empty());
    }
}
