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

#[cfg_attr(test, mockall_double::double)]
use crate::workload_state::workload_state_store::WorkloadStateStore;

use ankaios_api::ank_base::{DeletedWorkload, FulfilledBy, WorkloadNamed};

#[cfg(test)]
use mockall::automock;

pub struct DependencyStateValidator {}

#[cfg_attr(test, automock)]
impl DependencyStateValidator {
    pub fn create_fulfilled(
        workload: &WorkloadNamed,
        workload_state_db: &WorkloadStateStore,
    ) -> bool {
        workload
            .workload
            .dependencies
            .dependencies
            .iter()
            // [impl->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
            .all(|(dependency_name, add_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .is_some_and(|wl_state| {
                        // [impl->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
                        add_condition.fulfilled_by(wl_state)
                    })
            })
    }

    pub fn delete_fulfilled(
        workload: &DeletedWorkload,
        workload_state_db: &WorkloadStateStore,
    ) -> bool {
        workload
            .dependencies
            .iter()
            // [impl->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
            .all(|(dependency_name, delete_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .is_none_or(|wl_state| {
                        // [impl->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
                        delete_condition.fulfilled_by(wl_state)
                    })
            })
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
    use super::DependencyStateValidator;
    use crate::workload_state::workload_state_store::MockWorkloadStateStore;

    use ankaios_api::ank_base::{DeleteCondition, ExecutionStateSpec};
    use ankaios_api::test_utils::{
        generate_test_deleted_workload_with_dependencies,
        generate_test_deleted_workload_with_params, generate_test_workload_named,
    };

    use std::collections::HashMap;

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";

    // [utest->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_create_fulfilled() {
        let mut workload = generate_test_workload_named();
        workload.workload.dependencies.dependencies = HashMap::from([(
            WORKLOAD_NAME_1.to_owned(),
            ankaios_api::ank_base::AddCondition::AddCondRunning,
        )]);

        let execution_state = ExecutionStateSpec::running();
        let mut wl_state_store_mock = MockWorkloadStateStore::default();
        wl_state_store_mock
            .states_storage
            .insert(WORKLOAD_NAME_1.to_owned(), execution_state);

        assert!(DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
    #[test]
    fn utest_create_fulfilled_no_dependencies() {
        let mut workload = generate_test_workload_named();

        workload.workload.dependencies.dependencies.clear(); // no inter-workload dependencies

        let wl_state_store_mock = MockWorkloadStateStore::default();

        assert!(DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_create_fulfilled_no_workload_state_known() {
        let workload = generate_test_workload_named();

        let wl_state_store_mock = MockWorkloadStateStore::default();

        assert!(!DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_create_fulfilled_unfulfilled_execution_state() {
        let workload = generate_test_workload_named();

        let execution_state = ExecutionStateSpec::succeeded();
        let mut wl_state_store_mock = MockWorkloadStateStore::default();
        wl_state_store_mock
            .states_storage
            .insert(WORKLOAD_NAME_1.to_owned(), execution_state);

        assert!(!DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_fulfilled() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let execution_state = ExecutionStateSpec::succeeded();
        let mut wl_state_store_mock = MockWorkloadStateStore::default();
        wl_state_store_mock
            .states_storage
            .insert(WORKLOAD_NAME_2.to_owned(), execution_state);

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_fulfilled_unfulfilled_execution_state() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let execution_state = ExecutionStateSpec::running();
        let mut wl_state_store_mock = MockWorkloadStateStore::default();
        wl_state_store_mock
            .states_storage
            .insert(WORKLOAD_NAME_2.to_owned(), execution_state);

        assert!(!DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
    #[test]
    fn utest_delete_fulfilled_no_dependencies() {
        let mut deleted_workload =
            generate_test_deleted_workload_with_params(AGENT_A, WORKLOAD_NAME_1);

        deleted_workload.dependencies.clear(); // no inter-workload dependencies

        let wl_state_store_mock = MockWorkloadStateStore::default();

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload,
            &wl_state_store_mock
        ));
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_fulfilled_no_workload_state_known() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let wl_state_store_mock = MockWorkloadStateStore::default();

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &wl_state_store_mock
        ));
    }
}
