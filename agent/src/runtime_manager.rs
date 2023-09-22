use std::collections::HashMap;

use common::objects::{DeletedWorkload, WorkloadSpec};

use crate::{workload::NewWorkload, workload_factory::WorkloadFactory};

struct RuntimeManager {
    workloads: HashMap<String, Box<dyn NewWorkload>>,
    wl_factory: WorkloadFactory,
}

impl RuntimeManager {
    async fn handle_update_workload(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) {
        for workload in deleted_workloads {
            if let Some(x) = self.workloads.remove(&workload.name) {
                if let Err(err) = x.delete().await {
                    log::error!("Failed to delete workload '{}': '{}'", workload.name, err);
                }
            } else {
                log::warn!("Workload '{}' already gone.", workload.name);
            }
        }

        for workload_spec in added_workloads {
            self.workloads.insert(
                workload_spec.workload.name.clone(),
                self.wl_factory
                    .create_workload(workload_spec.runtime.clone(), workload_spec),
            );
        }
    }
}
