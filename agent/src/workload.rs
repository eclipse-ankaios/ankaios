use crate::{
    runtime::{Runtime, RuntimeConfig},
    stoppable_state_checker::StoppableStateChecker,
    workload_id::WorkloadId,
};
use common::objects::WorkloadSpec;

// #[derive(Debug)]
pub struct NewWorkload<'a> {
    // channel: CommandChannel,
    // workload_spec: WorkloadSpec,
    pub workload_id: Box<dyn WorkloadId>,
    pub runtime: &'a Box<
        dyn Runtime<
            Id = dyn WorkloadId,
            Rc = dyn RuntimeConfig,
            StateChecker = dyn StoppableStateChecker,
        >,
    >,
}
