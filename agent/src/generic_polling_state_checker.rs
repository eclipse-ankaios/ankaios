// Copyright (c) 2023 Elektrobit Automotive GmbH
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

use async_trait::async_trait;
use std::{str::FromStr, time::Duration};
use tokio::{task::JoinHandle, time};

use crate::{
    runtime_connectors::{RuntimeStateGetter, StateChecker},
    workload_state::{WorkloadStateSender, WorkloadStateSenderInterface},
};
use common::objects::{ExecutionState, ExecutionStateEnum, WorkloadSpec};

// [impl->swdd~agent-provides-generic-state-checker-implementation~1]
const STATUS_CHECK_INTERVAL_MS: u64 = 500;

#[derive(Debug)]
pub struct GenericPollingStateChecker {
    workload_name: String,
    task_handle: JoinHandle<()>,
}

#[async_trait]
impl<WorkloadId> StateChecker<WorkloadId> for GenericPollingStateChecker
where
    WorkloadId: ToString + FromStr + Clone + Send + Sync + 'static,
{
    // [impl->swdd~agent-provides-generic-state-checker-implementation~1]
    fn start_checker(
        workload_spec: &WorkloadSpec,
        workload_id: WorkloadId,
        workload_state_sender: WorkloadStateSender,
        state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self {
        let workload_spec = workload_spec.clone();
        let workload_name = workload_spec.instance_name.workload_name().to_owned();
        let task_handle = tokio::spawn(async move {
            let mut last_state = ExecutionState::unknown("Never received an execution state.");
            let mut interval = time::interval(Duration::from_millis(STATUS_CHECK_INTERVAL_MS));
            loop {
                interval.tick().await;
                let current_state = state_getter.get_state(&workload_id).await;

                if current_state != last_state {
                    log::debug!(
                        "The workload {} has changed its state to {:?}",
                        workload_spec.instance_name.workload_name(),
                        current_state
                    );
                    last_state = current_state.clone();

                    // [impl->swdd~generic-state-checker-sends-workload-state~2]
                    workload_state_sender
                        .report_workload_execution_state(
                            &workload_spec.instance_name,
                            current_state,
                        )
                        .await;

                    if last_state.state == ExecutionStateEnum::Removed {
                        break;
                    }
                }
            }
        });

        GenericPollingStateChecker {
            workload_name,
            task_handle,
        }
    }

    async fn stop_checker(self) {
        drop(self);
    }
}

impl Drop for GenericPollingStateChecker {
    fn drop(&mut self) {
        self.task_handle.abort();
        log::trace!("Over and out for workload '{}'", self.workload_name);
    }
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
    use std::time::Duration;

    use common::{objects::generate_test_workload_spec_with_param, objects::ExecutionState};

    use crate::{
        generic_polling_state_checker::GenericPollingStateChecker,
        runtime_connectors::{MockRuntimeStateGetter, StateChecker},
    };

    const RUNTIME_NAME: &str = "runtime1";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_ID: &str = "some strange Id";

    // [utest->swdd~agent-provides-generic-state-checker-implementation~1]
    #[tokio::test]
    async fn utest_generic_polling_state_checker_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_runtime_getter = MockRuntimeStateGetter::default();

        mock_runtime_getter
            .expect_get_state()
            .times(2)
            .returning(|_: &String| Box::pin(async { ExecutionState::running() }));

        let (state_sender, mut state_receiver) = tokio::sync::mpsc::channel(20);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let generic_state_state_checker = GenericPollingStateChecker::start_checker(
            &workload_spec,
            WORKLOAD_ID.to_string(),
            state_sender.clone(),
            mock_runtime_getter,
        );

        tokio::time::sleep(Duration::from_millis(1200)).await;

        <GenericPollingStateChecker as StateChecker<String>>::stop_checker::<'_>(
            generic_state_state_checker,
        )
        .await;

        let expected_state = common::objects::generate_test_workload_state_with_workload_spec(
            &workload_spec,
            ExecutionState::running(),
        );

        // [utest->swdd~generic-state-checker-sends-workload-state~2]
        let state_update_1 = state_receiver.recv().await.unwrap();
        assert_eq!(state_update_1, expected_state);
    }
}
