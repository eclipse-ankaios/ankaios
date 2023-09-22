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
                x.delete().await
            }
        }

        for workload in added_workloads {
            self.workloads.insert(
                workload.workload.name.clone(),
                self.wl_factory.create_workload(workload.runtime.clone()),
            );
        }
    }
}
