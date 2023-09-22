use common::objects::WorkloadSpec;

use crate::workload::NewWorkload;

pub struct WorkloadFactory {}

impl WorkloadFactory {
    pub fn create_workload(
        &self,
        runtime_id: String,
        workload_spec: WorkloadSpec,
    ) -> Box<dyn NewWorkload> {
        todo!()
    }
}
