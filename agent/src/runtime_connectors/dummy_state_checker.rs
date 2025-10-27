// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use std::str::FromStr;

use async_trait::async_trait;
use api::ank_base::WorkloadInternal;

use crate::workload_state::WorkloadStateSender;

use super::{RuntimeStateGetter, StateChecker};

// [impl->swdd~agent-skips-unknown-runtime~2]
pub struct DummyStateChecker<WorkloadId>(std::marker::PhantomData<WorkloadId>);

impl<WorkloadId> DummyStateChecker<WorkloadId> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

// [impl->swdd~agent-skips-unknown-runtime~2]
#[async_trait]
impl<WorkloadId> StateChecker<WorkloadId> for DummyStateChecker<WorkloadId>
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
{
    fn start_checker(
        _workload_spec: &WorkloadInternal,
        _workload_id: WorkloadId,
        _manager_interface: WorkloadStateSender,
        _state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self {
        Self(std::marker::PhantomData)
    }
    async fn stop_checker(self) {}
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
    use crate::runtime_connectors::MockRuntimeStateGetter;
    use api::test_utils::generate_test_workload_with_param;

    // [utest->swdd~agent-skips-unknown-runtime~2]
    #[tokio::test]
    async fn utest_dummy_state_checker() {
        let workload_spec = generate_test_workload_with_param(
            "agent_name".to_string(),
            "workload_name".to_string(),
            "runtime_name".to_string(),
        );
        let workload_id = "test_id".to_string();
        let (state_sender, _) = tokio::sync::mpsc::channel(10);
        let state_getter = MockRuntimeStateGetter::default();

        let checker = DummyStateChecker::start_checker(
            &workload_spec,
            workload_id,
            state_sender,
            state_getter,
        );

        checker.stop_checker().await;
    }
}
