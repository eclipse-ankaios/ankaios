use std::{collections::HashMap, sync::Arc};

use crate::{
    podman::PodmanKubeWorkloadId,
    runtime::{Runtime, RuntimeConfig},
    stoppable_state_checker::StoppableStateChecker,
    workload::NewWorkload,
    workload_id::WorkloadId,
};

type BoxedRuntime = Arc<
    dyn Runtime<
        Id = dyn WorkloadId,
        Rc = dyn RuntimeConfig,
        StateChecker = dyn StoppableStateChecker,
    >,
>;

pub struct WorkloadFactory {
    runtime_map: HashMap<String, BoxedRuntime>,
}

impl WorkloadFactory {
    pub fn create_workload(&self, runtime_id: String) -> NewWorkload {
        let runtime = self.runtime_map.get(&runtime_id).unwrap().clone();

        let wl_id = PodmanKubeWorkloadId {
            manifest: "bla bla".to_string(),
        };

        NewWorkload {
            workload_id: Box::new(wl_id),
            runtime,
        }
    }
}
