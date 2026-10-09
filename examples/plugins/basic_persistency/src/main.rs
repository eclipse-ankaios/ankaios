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

//! Basic Persistence Plugin for Ankaios
//!
//! Watches workload state changes via the Ankaios Events API (through `ankaios_sdk`)
//! and persists workloads marked with a `persist` tag to a runtime state directory.
//!
//! Persistence modes (configured via the `persist` tag, see [`persist_mode::PersistMode`]):
//! - `ALWAYS`: Persist the workload as soon as the server accepts it (in desired state)
//! - `ON_RUNNING`: Persist only once the workload reaches the Running execution state
//!
//! A failed persist attempt is retried every [`RETRY_INTERVAL`] instead of being dropped
//! until the workload's desired state happens to change again.

mod persist_mode;
mod reconciler;
mod store;

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use ankaios_sdk::Ankaios;

use reconciler::{Action, Reconciler};
use store::PersistenceStore;

const WORKLOAD_STATES_FIELD_MASK: &str = "workloadStates.*.*.*.state";
const DESIRED_WORKLOADS_FIELD_MASK: &str = "desiredState.workloads.*";
const DESIRED_CONFIGS_FIELD_MASK: &str = "desiredState.configs";

/// Interval at which a failed persist attempt is retried.
const RETRY_INTERVAL: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting Basic Persistence Plugin...");

    let persistence_dir = PathBuf::from(
        env::var("PERSISTENCE_DIR").unwrap_or_else(|_| "/var/lib/ankaios".to_owned()),
    );
    log::info!("Persistence directory: {persistence_dir:?}");

    let store = PersistenceStore::new(&persistence_dir);
    store.init().await?;

    let mut ankaios = Ankaios::new().await?;
    log::info!("Connected to Ankaios control interface");

    restore_persisted_state(&store, &mut ankaios).await;

    let mut events = ankaios
        .register_event(vec![
            WORKLOAD_STATES_FIELD_MASK.to_owned(),
            DESIRED_WORKLOADS_FIELD_MASK.to_owned(),
            DESIRED_CONFIGS_FIELD_MASK.to_owned(),
        ])
        .await?;
    log::info!("Subscribed to Ankaios events");

    let persisted_names = store.persisted_names().await;
    log::info!(
        "Initialized persisted workload tracking: {} workload(s) on disk",
        persisted_names.len()
    );
    let mut reconciler = Reconciler::new(persisted_names);

    // `register_event` always delivers the startup snapshot as the first event. Workloads
    // whose persist condition is already satisfied (e.g. they were applied, or reached
    // Running, before the plugin last started) are persisted immediately from it.
    match events.events_receiver.recv().await {
        Some(initial) => {
            for action in reconciler.seed_from_initial_state(&initial.complete_state) {
                apply_action(&store, &mut reconciler, action).await;
            }
            log::info!(
                "Initial state processed, tracking {} ON_RUNNING workload(s)",
                reconciler.tracked_on_running_count()
            );
        }
        None => return Err("Event stream closed before initial state was received".into()),
    }

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut retry_interval = tokio::time::interval(RETRY_INTERVAL);
    retry_interval.tick().await; // first tick fires immediately; nothing to retry yet

    loop {
        tokio::select! {
            event = events.events_receiver.recv() => {
                let Some(event) = event else {
                    log::error!("Event stream closed unexpectedly");
                    return Err("Event stream closed".into());
                };
                for action in reconciler.reconcile(&event) {
                    apply_action(&store, &mut reconciler, action).await;
                }
            }
            _ = retry_interval.tick() => {
                for action in reconciler.retry_pending() {
                    apply_action(&store, &mut reconciler, action).await;
                }
            }
            _ = sigterm.recv() => {
                log::info!("Received SIGTERM, shutting down gracefully");
                return Ok(());
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received SIGINT, shutting down gracefully");
                return Ok(());
            }
        }
    }
}

/// Executes a single reconciler action against the store and feeds the outcome back into
/// the reconciler. A failed persist is queued for a later retry (see [`RETRY_INTERVAL`])
/// instead of being dropped.
async fn apply_action(store: &PersistenceStore, reconciler: &mut Reconciler, action: Action) {
    match action {
        Action::Persist { name, workload, configs } => match store.persist(&name, &workload, &configs).await {
            Ok(()) => reconciler.confirm_persisted(&name),
            Err(e) => {
                log::error!("Failed to persist workload '{name}': {e}, will retry");
                reconciler.mark_retry(&name, *workload, configs);
            }
        },
        Action::Remove { name } => match store.remove(&name).await {
            Ok(()) => reconciler.confirm_removed(&name),
            Err(e) => log::error!("Failed to remove persisted workload '{name}': {e}"),
        },
    }
}

/// Applies any persisted workloads on top of the startup manifest. Best-effort: a failure
/// here is logged but must not prevent the plugin from starting and watching for events.
async fn restore_persisted_state(store: &PersistenceStore, ankaios: &mut Ankaios) {
    let Some(manifest) = store.load_manifest().await else {
        log::info!("No persisted workloads found, nothing to restore");
        return;
    };

    match ankaios.apply_manifest(manifest).await {
        Ok(result) => log::info!(
            "Restored persisted workloads: {} added, {} deleted",
            result.added_workloads.len(),
            result.deleted_workloads.len()
        ),
        Err(e) => log::error!("Failed to restore persisted workloads: {e}"),
    }
}
