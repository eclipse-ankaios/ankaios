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

use ankaios_api::ank_base::{
    AddCondition, DeleteCondition, DeletedWorkload, ExecutionStateSpec, WorkloadNamed,
    WorkloadStateSpec, WorkloadStatesMapSpec,
};

#[cfg(test)]
use mockall::automock;

// [impl->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
fn is_add_condition_fulfilled_by_any_state(
    add_cond: &AddCondition,
    state: &[WorkloadStateSpec],
) -> bool {
    state
        .iter()
        .any(|wl_state| is_add_condition_fulfilled_by_state(add_cond, &wl_state.execution_state))
}
fn is_add_condition_fulfilled_by_state(
    add_cond: &AddCondition,
    state: &ExecutionStateSpec,
) -> bool {
    match add_cond {
        AddCondition::AddCondRunning => (*state).is_running(),
        AddCondition::AddCondSucceeded => (*state).is_succeeded(),
        AddCondition::AddCondFailed => (*state).is_failed(),
    }
}

// [impl->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
fn is_del_condition_fulfilled_by_any_state(
    del_cond: &DeleteCondition,
    state: &[WorkloadStateSpec],
) -> bool {
    state
        .iter()
        .all(|wl_state| is_del_condition_fulfilled_by_state(del_cond, &wl_state.execution_state))
}
fn is_del_condition_fulfilled_by_state(
    del_cond: &DeleteCondition,
    state: &ExecutionStateSpec,
) -> bool {
    if state.is_waiting_to_start() {
        return true;
    }

    match del_cond {
        DeleteCondition::DelCondNotPendingNorRunning => (*state).is_not_pending_nor_running(),
        DeleteCondition::DelCondRunning => (*state).is_running(),
    }
}

pub struct DependencyStateValidator {}

#[cfg_attr(test, automock)]
impl DependencyStateValidator {
    pub fn create_fulfilled(
        workload: &WorkloadNamed,
        workload_state_db: &WorkloadStatesMapSpec,
    ) -> bool {
        workload
            .workload
            .dependencies
            .dependencies
            .iter()
            // [impl->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
            .all(|(dependency_name, add_condition)| {
                // [impl->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
                is_add_condition_fulfilled_by_any_state(
                    add_condition,
                    &workload_state_db.get_states_for_workload_name(dependency_name),
                )
            })
    }

    pub fn delete_fulfilled(
        workload: &DeletedWorkload,
        workload_state_db: &WorkloadStatesMapSpec,
    ) -> bool {
        workload
            .dependencies
            .iter()
            // [impl->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
            .all(|(dependency_name, delete_condition)| {
                // [impl->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
                is_del_condition_fulfilled_by_any_state(
                    delete_condition,
                    &workload_state_db.get_states_for_workload_name(dependency_name),
                )
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

    use ankaios_api::ank_base::{DeleteCondition, ExecutionStateSpec, WorkloadStatesMapSpec};

    use ankaios_api::test_utils::{
        fixtures, generate_test_deleted_workload_with_dependencies,
        generate_test_deleted_workload_with_params, generate_test_workload_named,
        generate_test_workload_states_map_with_data,
    };

    use std::collections::HashMap;

    // [utest->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_create_fulfilled() {
        let mut workload = generate_test_workload_named();
        workload.workload.dependencies.dependencies = HashMap::from([(
            fixtures::WORKLOAD_NAMES[0].to_owned(),
            ankaios_api::ank_base::AddCondition::AddCondRunning,
        )]);

        let wl_state_store = generate_test_workload_states_map_with_data(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
            fixtures::WORKLOAD_IDS[0],
            ExecutionStateSpec::running(),
        );

        assert!(DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store
        ));
    }

    // [utest->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
    #[test]
    fn utest_create_fulfilled_no_dependencies() {
        let mut workload = generate_test_workload_named();

        workload.workload.dependencies.dependencies.clear(); // no inter-workload dependencies

        let wl_state_store = WorkloadStatesMapSpec::default();

        assert!(DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store
        ));
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_create_fulfilled_no_workload_state_known() {
        let workload = generate_test_workload_named();

        let wl_state_store = WorkloadStatesMapSpec::default();

        assert!(!DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store
        ));
    }

    // [utest->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_create_fulfilled_unfulfilled_execution_state() {
        let workload = generate_test_workload_named();

        let wl_state_store = generate_test_workload_states_map_with_data(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
            fixtures::WORKLOAD_IDS[0],
            ExecutionStateSpec::succeeded(),
        );

        assert!(!DependencyStateValidator::create_fulfilled(
            &workload,
            &wl_state_store
        ));
    }

    // [utest->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_fulfilled() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            fixtures::AGENT_NAMES[0].to_string(),
            fixtures::WORKLOAD_NAMES[0].to_string(),
            HashMap::from([(
                fixtures::WORKLOAD_NAMES[1].to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let wl_state_store = generate_test_workload_states_map_with_data(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[1],
            fixtures::WORKLOAD_IDS[1],
            ExecutionStateSpec::succeeded(),
        );

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &wl_state_store
        ));
    }

    // [utest->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_fulfilled_unfulfilled_execution_state() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            fixtures::AGENT_NAMES[0].to_string(),
            fixtures::WORKLOAD_NAMES[0].to_string(),
            HashMap::from([(
                fixtures::WORKLOAD_NAMES[1].to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let wl_state_store = generate_test_workload_states_map_with_data(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[1],
            fixtures::WORKLOAD_IDS[1],
            ExecutionStateSpec::running(),
        );

        assert!(!DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &wl_state_store
        ));
    }

    // [utest->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
    #[test]
    fn utest_delete_fulfilled_no_dependencies() {
        let mut deleted_workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
        );

        deleted_workload.dependencies.clear(); // no inter-workload dependencies

        let wl_state_store = WorkloadStatesMapSpec::default();

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload,
            &wl_state_store
        ));
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_fulfilled_no_workload_state_known() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            fixtures::AGENT_NAMES[0].to_owned(),
            fixtures::WORKLOAD_NAMES[0].to_owned(),
            HashMap::from([(
                fixtures::WORKLOAD_NAMES[1].to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let wl_state_store = WorkloadStatesMapSpec::default();

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &wl_state_store
        ));
    }
}
