use std::collections::HashMap;

use common::objects::{DeletedWorkload, WorkloadSpec};

use crate::{workload::Workload, workload_factory::WorkloadFactory};

struct RuntimeManager {
    workloads: HashMap<String, Workload>,
    workload_factory_map: HashMap<String, Box<dyn WorkloadFactory>>,
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
            let workload_name = workload_spec.workload.name.clone();
            let workload = self
                .workload_factory_map
                .get(&workload_spec.runtime)
                .unwrap() // TODO
                .create_workload(workload_spec.workload)
                .await
                .unwrap();
            self.workloads.insert(workload_name, workload);
        }
    }
}
