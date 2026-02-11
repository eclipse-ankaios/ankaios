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

use crate::{
    runtime_connectors::{RuntimeStateGetter, StateChecker},
    workload_state::{WorkloadStateSender, WorkloadStateSenderInterface},
};
use ankaios_api::ank_base::{ExecutionStateEnumSpec, ExecutionStateSpec, WorkloadNamed};

use async_trait::async_trait;
use std::{str::FromStr, time::Duration};
use tokio::{task::JoinHandle, time};

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
        workload_named: &WorkloadNamed,
        workload_id: WorkloadId,
        workload_state_sender: WorkloadStateSender,
        state_getter: impl RuntimeStateGetter<WorkloadId>,
    ) -> Self {
        let workload_named = workload_named.clone();
        let workload_name = workload_named.instance_name.workload_name().to_owned();
        let task_handle = tokio::spawn(async move {
            let mut last_state = ExecutionStateSpec::unknown("Never received an execution state.");
            let mut interval = time::interval(Duration::from_millis(STATUS_CHECK_INTERVAL_MS));
            loop {
                interval.tick().await;
                let current_state = state_getter.get_state(&workload_id).await;

                if current_state != last_state {
                    log::debug!(
                        "The workload {} has changed its state to {:?}",
                        workload_named.instance_name.workload_name(),
                        current_state
                    );
                    last_state = current_state.clone();

                    // [impl->swdd~generic-state-checker-sends-workload-state~2]
                    workload_state_sender
                        .report_workload_execution_state(
                            &workload_named.instance_name,
                            current_state,
                        )
                        .await;

                    if matches!(last_state.state(), ExecutionStateEnumSpec::Removed(_)) {
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
    use super::STATUS_CHECK_INTERVAL_MS;
    use crate::{
        generic_polling_state_checker::GenericPollingStateChecker,
        runtime_connectors::{MockRuntimeStateGetter, StateChecker},
    };

    use ankaios_api::ank_base::ExecutionStateSpec;
    use ankaios_api::test_utils::{
        fixtures, generate_test_workload_named, generate_test_workload_state_with_workload_named,
    };
    use mockall::Sequence;
    use std::time::{Duration, SystemTime};

    // [utest->swdd~agent-provides-generic-state-checker-implementation~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_generic_polling_state_checker_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let start_time = SystemTime::now();
        let mut mock_runtime_getter = MockRuntimeStateGetter::default();
        let mut mock_sequence = Sequence::new();

        mock_runtime_getter
            .expect_get_state()
            .times(2)
            .in_sequence(&mut mock_sequence)
            .returning(|_: &String| Box::pin(async { ExecutionStateSpec::running() }));
        mock_runtime_getter
            .expect_get_state()
            .once()
            .in_sequence(&mut mock_sequence)
            .return_once(|_: &String| Box::pin(async { ExecutionStateSpec::removed() }));

        let (state_sender, mut state_receiver) = tokio::sync::mpsc::channel(20);

        let workload = generate_test_workload_named();

        let generic_state_state_checker = GenericPollingStateChecker::start_checker(
            &workload,
            fixtures::WORKLOAD_IDS[0].to_string(),
            state_sender.clone(),
            mock_runtime_getter,
        );

        let expected_running = generate_test_workload_state_with_workload_named(
            &workload,
            ExecutionStateSpec::running(),
        );
        let expected_removed = generate_test_workload_state_with_workload_named(
            &workload,
            ExecutionStateSpec::removed(),
        );

        // [utest->swdd~generic-state-checker-sends-workload-state~2]
        let state_update_1 = state_receiver.recv().await.unwrap();
        let state_update_2 = state_receiver.recv().await.unwrap();

        assert_eq!(state_update_1, expected_running);
        assert_eq!(state_update_2, expected_removed);

        tokio::time::sleep(Duration::from_millis(10)).await; // Needed for making sure the next assert passes
        assert!(generic_state_state_checker.task_handle.is_finished());
        assert!(
            start_time.elapsed().unwrap().as_millis() >= (STATUS_CHECK_INTERVAL_MS * 2) as u128
        );
    }
}
