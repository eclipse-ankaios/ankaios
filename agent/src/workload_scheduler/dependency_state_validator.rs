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

use common::objects::{DeletedWorkload, FulfilledBy, WorkloadSpec};

#[cfg_attr(test, mockall_double::double)]
use crate::parameter_storage::ParameterStorage;
use crate::workload_operation::WorkloadOperation;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Clone, PartialEq)]
pub enum DependencyState {
    PendingCreate,
    PendingDelete,
    Fulfilled,
}

impl DependencyState {
    pub fn is_fulfilled(&self) -> bool {
        *self == DependencyState::Fulfilled
    }

    pub fn is_pending(&self) -> bool {
        !self.is_fulfilled()
    }

    pub fn is_pending_delete(&self) -> bool {
        *self == DependencyState::PendingDelete
    }
}

pub struct DependencyStateValidator {}

#[cfg_attr(test, automock)]
impl DependencyStateValidator {
    fn create_fulfilled(
        workload: &WorkloadSpec,
        workload_state_db: &ParameterStorage,
    ) -> DependencyState {
        if workload
            .dependencies
            .iter()
            .all(|(dependency_name, add_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .map_or(false, |wl_state| add_condition.fulfilled_by(&wl_state))
            })
        {
            DependencyState::Fulfilled
        } else {
            DependencyState::PendingCreate
        }
    }

    fn delete_fulfilled(
        workload: &DeletedWorkload,
        workload_state_db: &ParameterStorage,
    ) -> DependencyState {
        if workload
            .dependencies
            .iter()
            .all(|(dependency_name, delete_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .map_or(true, |wl_state| delete_condition.fulfilled_by(&wl_state))
            })
        {
            DependencyState::Fulfilled
        } else {
            DependencyState::PendingDelete
        }
    }

    pub fn dependencies_for_workload_fulfilled(
        workload_operation: &WorkloadOperation,
        workload_state_db: &ParameterStorage,
    ) -> DependencyState {
        match workload_operation {
            WorkloadOperation::Create(workload_spec) => {
                Self::create_fulfilled(workload_spec, workload_state_db)
            }
            WorkloadOperation::Update(_, deleted_workload) => {
                /* The update operation is only blocked when a delete is pending.
                If the create operation is pending the delete can still be done.*/
                Self::delete_fulfilled(deleted_workload, workload_state_db)
            }
            WorkloadOperation::Delete(deleted_workload) => {
                Self::delete_fulfilled(deleted_workload, workload_state_db)
            }
        }
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
    use common::{
        objects::{
            generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
            AddCondition, DeleteCondition, ExecutionState,
        },
        test_utils::generate_test_deleted_workload_with_dependencies,
    };
    use mockall::predicate;
    use std::collections::HashMap;

    use crate::{
        parameter_storage::MockParameterStorage, workload_operation::WorkloadOperation,
        workload_scheduler::dependency_state_validator::DependencyState,
    };

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const RUNTIME: &str = "runtime";

    #[test]
    fn utest_state_validator_dependencies_for_workload_fulfilled_create() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        assert_eq!(
            DependencyState::Fulfilled,
            DependencyStateValidator::dependencies_for_workload_fulfilled(
                &WorkloadOperation::Create(workload_with_dependencies),
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_dependencies_for_workload_fulfilled_delete() {
        let deleted_workload = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_string(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::succeeded()));

        assert_eq!(
            DependencyState::Fulfilled,
            DependencyStateValidator::dependencies_for_workload_fulfilled(
                &WorkloadOperation::Delete(deleted_workload),
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_dependencies_for_workload_fulfilled_update() {
        let new_workload = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let deleted_workload = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            HashMap::from([(
                WORKLOAD_NAME_3.to_string(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        assert_eq!(
            DependencyState::PendingDelete,
            DependencyStateValidator::dependencies_for_workload_fulfilled(
                &WorkloadOperation::Update(new_workload, deleted_workload),
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_create_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .with(predicate::eq(WORKLOAD_NAME_2.to_owned()))
            .return_const(Some(ExecutionState::running()));

        assert_eq!(
            DependencyState::Fulfilled,
            DependencyStateValidator::create_fulfilled(
                &workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_create_fulfilled_no_dependencies() {
        let mut workload_with_dependencies = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        workload_with_dependencies.dependencies.clear(); // no inter-workload dependencies

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .never();

        assert_eq!(
            DependencyState::Fulfilled,
            DependencyStateValidator::create_fulfilled(
                &workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_create_fulfilled_no_workload_state_known() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        assert_eq!(
            DependencyState::PendingCreate,
            DependencyStateValidator::create_fulfilled(
                &workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_create_fulfilled_unfulfilled_execution_state() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::succeeded()));

        assert_eq!(
            DependencyState::PendingCreate,
            DependencyStateValidator::create_fulfilled(
                &workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_delete_fulfilled() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .with(predicate::eq(WORKLOAD_NAME_2.to_owned()))
            .return_const(Some(ExecutionState::succeeded()));

        assert_eq!(
            DependencyState::Fulfilled,
            DependencyStateValidator::delete_fulfilled(
                &deleted_workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_delete_fulfilled_unfulfilled_execution_state() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        assert_eq!(
            DependencyState::PendingDelete,
            DependencyStateValidator::delete_fulfilled(
                &deleted_workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }

    #[test]
    fn utest_state_validator_delete_fulfilled_no_workload_state_known() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        assert_eq!(
            DependencyState::Fulfilled,
            DependencyStateValidator::delete_fulfilled(
                &deleted_workload_with_dependencies,
                &parameter_storage_mock
            )
        );
    }
}
