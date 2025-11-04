// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use std::{collections::HashSet, fmt::Display};

use api::ank_base;
use api::ank_base::{ExecutionStateEnumInternal, Pending, WorkloadInstanceNameInternal};
use common::objects::WorkloadState;

#[cfg(test)]
use mockall::mock;

use crate::output_update;

#[derive(Debug)]
pub struct ParsedUpdateStateSuccess {
    pub added_workloads: Vec<WorkloadInstanceNameInternal>,
    pub deleted_workloads: Vec<WorkloadInstanceNameInternal>,
}

impl TryFrom<ank_base::UpdateStateSuccess> for ParsedUpdateStateSuccess {
    type Error = String;

    fn try_from(value: ank_base::UpdateStateSuccess) -> Result<Self, Self::Error> {
        Ok(Self {
            added_workloads: value
                .added_workloads
                .iter()
                .map(|x| WorkloadInstanceNameInternal::try_from(x.as_ref()))
                .collect::<Result<_, String>>()?,

            deleted_workloads: value
                .deleted_workloads
                .iter()
                .map(|x| WorkloadInstanceNameInternal::try_from(x.as_ref()))
                .collect::<Result<_, String>>()?,
        })
    }
}

pub trait WaitListDisplayTrait: Display {
    fn update(&mut self, workload_state: &WorkloadState);
    fn set_complete(&mut self, workload: &WorkloadInstanceNameInternal);
    fn step_spinner(&mut self);
}

#[cfg(test)]
mock! {
    pub MyWaitListDisplay{

    }
    impl Display for MyWaitListDisplay {
        fn fmt<'a>(&self, formater: &mut std::fmt::Formatter<'a>) -> std::result::Result<(), std::fmt::Error>;
    }

    impl WaitListDisplayTrait for MyWaitListDisplay {
        fn update(&mut self, workload_state: &WorkloadState);
        fn set_complete(&mut self, workload: &WorkloadInstanceNameInternal);
        fn step_spinner(&mut self);
    }
}

pub struct WaitList<T> {
    pub added_workloads: HashSet<WorkloadInstanceNameInternal>,
    pub deleted_workloads: HashSet<WorkloadInstanceNameInternal>,
    connected_agents: HashSet<String>,
    display: T,
}

impl<T: WaitListDisplayTrait> WaitList<T> {
    pub fn new(
        value: ParsedUpdateStateSuccess,
        connected_agents: HashSet<String>,
        display: T,
    ) -> Self {
        Self {
            added_workloads: value.added_workloads.into_iter().collect(),
            deleted_workloads: value.deleted_workloads.into_iter().collect(),
            connected_agents,
            display,
        }
    }

    // [impl->swdd~cli-checks-for-final-workload-state~3]
    pub fn update(&mut self, values: impl IntoIterator<Item = WorkloadState>) {
        for workload_state in values.into_iter() {
            self.display.update(&workload_state);
            match workload_state.execution_state.state() {
                ExecutionStateEnumInternal::Running(_)
                | ExecutionStateEnumInternal::Succeeded(_)
                | ExecutionStateEnumInternal::Failed(_)
                | ExecutionStateEnumInternal::NotScheduled(_) => {
                    if self.added_workloads.remove(&workload_state.instance_name) {
                        self.display.set_complete(&workload_state.instance_name)
                    }
                }
                ExecutionStateEnumInternal::Pending(Pending::StartingFailed) => {
                    if self.added_workloads.remove(&workload_state.instance_name) {
                        self.display.set_complete(&workload_state.instance_name)
                    }
                }
                ExecutionStateEnumInternal::Removed(_) => {
                    if self.deleted_workloads.remove(&workload_state.instance_name) {
                        self.display.set_complete(&workload_state.instance_name)
                    }
                }
                ExecutionStateEnumInternal::AgentDisconnected(_) => {
                    if self.added_workloads.remove(&workload_state.instance_name) {
                        self.display.set_complete(&workload_state.instance_name)
                    }

                    if self.deleted_workloads.remove(&workload_state.instance_name) {
                        self.display.set_complete(&workload_state.instance_name)
                    }
                }
                _ => {}
            };
        }

        // prevent infinite waiting for added workloads with disconnected agent
        Self::retain_workloads_of_connected_agents(
            &mut self.added_workloads,
            &mut self.display,
            &self.connected_agents,
        );

        // prevent infinite waiting for deleted workloads with disconnected agent
        Self::retain_workloads_of_connected_agents(
            &mut self.deleted_workloads,
            &mut self.display,
            &self.connected_agents,
        );

        output_update!("{}", &self.display);
    }

    pub fn step_spinner(&mut self) {
        self.display.step_spinner();
        output_update!("{}", &self.display);
    }

    pub fn is_empty(&self) -> bool {
        self.added_workloads.is_empty() && self.deleted_workloads.is_empty()
    }

    fn retain_workloads_of_connected_agents(
        workload_instance_names: &mut HashSet<WorkloadInstanceNameInternal>,
        display: &mut T,
        connected_agents: &HashSet<String>,
    ) {
        workload_instance_names.retain(|instance_name| {
            if !connected_agents.contains(instance_name.agent_name()) {
                display.set_complete(instance_name);
                false
            } else {
                true
            }
        });
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
fn generate_test_wait_list(
    my_mock: MockMyWaitListDisplay,
    added_workloads: Vec<WorkloadInstanceNameInternal>,
    deleted_workloads: Vec<WorkloadInstanceNameInternal>,
    connected_agents: HashSet<String>,
) -> WaitList<MockMyWaitListDisplay> {
    let update_state_list = ParsedUpdateStateSuccess {
        added_workloads,
        deleted_workloads,
    };

    WaitList::new(update_state_list, connected_agents, my_mock)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use api::ank_base::{ExecutionStateInternal, WorkloadInstanceNameInternal};
    use api::test_utils::generate_test_workload_instance_name;
    use common::objects::WorkloadState;
    use mockall::predicate::eq;

    use crate::cli_commands::wait_list::generate_test_wait_list;

    use super::MockMyWaitListDisplay;

    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";

    fn prepare_test_instance_names() -> (
        WorkloadInstanceNameInternal,
        WorkloadInstanceNameInternal,
        WorkloadInstanceNameInternal,
    ) {
        let i_name_1 = generate_test_workload_instance_name(WORKLOAD_NAME_1);
        let i_name_2 = generate_test_workload_instance_name(WORKLOAD_NAME_2);
        let i_name_3 = generate_test_workload_instance_name(WORKLOAD_NAME_3);
        (i_name_1, i_name_2, i_name_3)
    }

    fn prepare_wait_list_display_mock(
        update_expectation: &WorkloadState,
        set_complete_expectation: &WorkloadInstanceNameInternal,
    ) -> MockMyWaitListDisplay {
        let mut my_mock = MockMyWaitListDisplay::new();

        my_mock
            .expect_update()
            .once()
            .with(eq(update_expectation.clone()))
            .return_const(());
        my_mock.expect_fmt().once().return_const(Ok(()));
        my_mock
            .expect_set_complete()
            .once()
            .with(eq(set_complete_expectation.clone()))
            .return_const(());
        my_mock
    }

    // [utest->swdd~cli-checks-for-final-workload-state~3]
    #[test]
    fn utest_update_wait_list_added_running() {
        let (i_name_1, i_name_2, i_name_3) = prepare_test_instance_names();

        let workload_state = WorkloadState {
            instance_name: i_name_1.clone(),
            execution_state: ExecutionStateInternal::running(),
        };

        let my_mock = prepare_wait_list_display_mock(&workload_state, &i_name_1);

        let mut wait_list = generate_test_wait_list(
            my_mock,
            vec![i_name_1.clone(), i_name_2.clone()],
            vec![i_name_3.clone()],
            HashSet::from(["agent_name".to_string()]),
        );

        wait_list.update(vec![workload_state]);

        assert!(!wait_list.added_workloads.contains(&i_name_1));
        assert!(wait_list.added_workloads.contains(&i_name_2));
        assert!(wait_list.deleted_workloads.contains(&i_name_3));
    }

    // [utest->swdd~cli-checks-for-final-workload-state~3]
    #[test]
    fn utest_update_wait_list_added_succeeded() {
        let (i_name_1, i_name_2, i_name_3) = prepare_test_instance_names();

        let workload_state = WorkloadState {
            instance_name: i_name_1.clone(),
            execution_state: ExecutionStateInternal::succeeded(),
        };

        let my_mock = prepare_wait_list_display_mock(&workload_state, &i_name_1);

        let mut wait_list = generate_test_wait_list(
            my_mock,
            vec![i_name_1.clone(), i_name_2.clone()],
            vec![i_name_3.clone()],
            HashSet::from(["agent_name".to_string()]),
        );

        wait_list.update(vec![workload_state]);

        assert!(!wait_list.added_workloads.contains(&i_name_1));
        assert!(wait_list.added_workloads.contains(&i_name_2));
        assert!(wait_list.deleted_workloads.contains(&i_name_3));
    }

    // [utest->swdd~cli-checks-for-final-workload-state~3]
    #[test]
    fn utest_update_wait_list_added_not_scheduled() {
        let (i_name_1, i_name_2, i_name_3) = prepare_test_instance_names();

        let workload_state = WorkloadState {
            instance_name: i_name_2.clone(),
            execution_state: ExecutionStateInternal::not_scheduled(),
        };

        let my_mock = prepare_wait_list_display_mock(&workload_state, &i_name_2);

        let mut wait_list = generate_test_wait_list(
            my_mock,
            vec![i_name_1.clone(), i_name_2.clone()],
            vec![i_name_3.clone()],
            HashSet::from(["agent_name".to_string()]),
        );

        wait_list.update(vec![workload_state]);

        assert!(wait_list.added_workloads.contains(&i_name_1));
        assert!(!wait_list.added_workloads.contains(&i_name_2));
        assert!(wait_list.deleted_workloads.contains(&i_name_3));
    }

    // [utest->swdd~cli-checks-for-final-workload-state~3]
    #[test]
    fn utest_update_wait_list_added_failed() {
        let (i_name_1, i_name_2, i_name_3) = prepare_test_instance_names();

        let workload_state = WorkloadState {
            instance_name: i_name_2.clone(),
            execution_state: ExecutionStateInternal::failed("some info"),
        };

        let my_mock = prepare_wait_list_display_mock(&workload_state, &i_name_2);

        let mut wait_list = generate_test_wait_list(
            my_mock,
            vec![i_name_1.clone(), i_name_2.clone()],
            vec![i_name_3.clone()],
            HashSet::from(["agent_name".to_string()]),
        );

        wait_list.update(vec![workload_state]);

        assert!(wait_list.added_workloads.contains(&i_name_1));
        assert!(!wait_list.added_workloads.contains(&i_name_2));
        assert!(wait_list.deleted_workloads.contains(&i_name_3));
    }

    // [utest->swdd~cli-checks-for-final-workload-state~3]
    #[test]
    fn utest_update_wait_list_added_starting_failed_no_more_retries() {
        let (i_name_1, i_name_2, i_name_3) = prepare_test_instance_names();

        let workload_state = WorkloadState {
            instance_name: i_name_2.clone(),
            execution_state: ExecutionStateInternal::retry_failed_no_retry("some error"),
        };

        let my_mock = prepare_wait_list_display_mock(&workload_state, &i_name_2);

        let mut wait_list = generate_test_wait_list(
            my_mock,
            vec![i_name_1.clone(), i_name_2.clone()],
            vec![i_name_3.clone()],
            HashSet::from(["agent_name".to_string()]),
        );

        wait_list.update(vec![workload_state]);

        assert!(wait_list.added_workloads.contains(&i_name_1));
        assert!(!wait_list.added_workloads.contains(&i_name_2));
        assert!(wait_list.deleted_workloads.contains(&i_name_3));
    }

    // [utest->swdd~cli-checks-for-final-workload-state~3]
    #[test]
    fn utest_update_wait_list_deleted_removed() {
        let (i_name_1, i_name_2, i_name_3) = prepare_test_instance_names();

        let workload_state = WorkloadState {
            instance_name: i_name_3.clone(),
            execution_state: ExecutionStateInternal::removed(),
        };

        let my_mock = prepare_wait_list_display_mock(&workload_state, &i_name_3);

        let mut wait_list = generate_test_wait_list(
            my_mock,
            vec![i_name_1.clone(), i_name_2.clone()],
            vec![i_name_3.clone()],
            HashSet::from(["agent_name".to_string()]),
        );

        wait_list.update(vec![workload_state]);

        assert!(wait_list.added_workloads.contains(&i_name_1));
        assert!(wait_list.added_workloads.contains(&i_name_2));
        assert!(!wait_list.deleted_workloads.contains(&i_name_3));
    }
}
