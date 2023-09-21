use std::collections::HashMap;

use crate::{
    podman::PodmanKubeWorkloadId,
    runtime::{Runtime, RuntimeConfig},
    stoppable_state_checker::StoppableStateChecker,
    workload::NewWorkload,
    workload_id::WorkloadId,
};

type BoxedRuntime = Box<
    dyn Runtime<
        Id = dyn WorkloadId,
        Rc = dyn RuntimeConfig,
        StateChecker = dyn StoppableStateChecker,
    >,
>;

struct WorkloadFactory {
    runtime_map: HashMap<String, BoxedRuntime>,
}

impl WorkloadFactory {
    fn create_workload(&self, runtime_id: String) -> NewWorkload {
        let runtime = self.runtime_map.get(&runtime_id).unwrap();

        let wl_id = PodmanKubeWorkloadId {
            manifest: "bla bla".to_string(),
        };

        NewWorkload {
            workload_id: Box::new(wl_id),
            runtime
        }
    }
}
