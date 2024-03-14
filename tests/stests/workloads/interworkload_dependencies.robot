# Copyright (c) 2024 Elektrobit Automotive GmbH
#
# This program and the accompanying materials are made available under the
# terms of the Apache License, Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
#
# SPDX-License-Identifier: Apache-2.0


*** Settings ***
Documentation       Tests to verify that Ankaios creates and deletes workloads
...                 with inter-workload dependencies properly.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${default_state_yaml_file}      ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***
# [stest->swdd~agent-enqueues-workload-operations-with-unfulfilled-dependencies~1]
# [stest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
# [stest->swdd~agent-reports-pending-create-workload-state~1]
Test Ankaios observes the inter-workload dependencies when creating workloads
    [Documentation]    Perform a create of an workload only if its start dependencies are fulfilled.
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/create_workloads_with_dependencies_config.yaml"
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    # Asserts
    Then the workload "logger" shall have the execution state "Pending(WaitingToStart)" on agent "agent_A"
    And Then the workload "error_notifier" shall have the execution state "Pending(WaitingToStart)" on agent "agent_A"
    And the workload "storage_provider" shall have the execution state "Pending(WaitingToStart)" on agent "agent_B"
    And the workload "filesystem_init" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And the workload "storage_provider" shall have the execution state "Running(Ok)" on agent "agent_B"
    And the workload "logger" shall have the execution state "Running(Ok)" on agent "agent_B"
    And the workload "storage_provider" shall have the execution state "Failed(ExecFailed)" on agent "agent_B"
    And the workload "error_notifier" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-reports-pending-delete-workload-state~1]
# [stest->swdd~agent-enqueues-workload-operations-with-unfulfilled-dependencies~1]
Test Ankaios observes the inter-workload dependencies when deleting workloads
    [Documentation]    Perform a delete of an workload only when its delete dependencies are fulfilled.
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/delete_workloads_with_dependencies.yaml"
    And Ankaios agent is started with name "agent_A"
    And the workload "frontend" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    # Actions
    When user triggers "ank delete workload backend"
    And the workload "backend" shall have the execution state "Stopping(WaitingToStop)" on agent "agent_A"
    And user triggers "ank delete workload frontend"
    # Asserts
    Then the workload "backend" shall not exist on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-enqueues-workload-operations-with-unfulfilled-dependencies~1]
# [stest->swdd~agent-reports-pending-delete-workload-state-on-pending-update-delete~1]
# [stest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
Test Ankaios CLI update workload with pending delete
    [Documentation]    Perform an update with pending delete only when the delete dependencies are fulfilled.
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/update_workloads_pending_delete.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_update_workload_pending_delete_new_state.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${default_state_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And the workload "frontend" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    # Actions
    When user triggers "ank get state > ${new_state_yaml_file}"
    And user updates the state "${new_state_yaml_file}" with "desiredState.workloads.backend.runtimeConfig.commandOptions=['-p', '8084:80']"
    And user triggers "ank set state -f ${new_state_yaml_file} desiredState.workloads.backend"
    And the workload "backend" shall have the execution state "Stopping(WaitingToStop)" on agent "agent_A" within "20" seconds
    And user triggers "ank delete workload frontend"
    # Asserts
    Then the workload "frontend" shall not exist on agent "agent_A" within "20" seconds
    And the workload "backend" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-enqueues-workload-operations-with-unfulfilled-dependencies~1]
# [stest->swdd~agent-reports-pending-create-workload-state-on-pending-update-create~1]
# [stest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
Test Ankaios CLI update workload with pending create
    [Documentation]    Perform the delete part of an update immediately but wait for the create until the create dependencies are fulfilled.
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/update_workloads_pending_create.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}    ${CONFIGS_DIR}/update_state_pending_create.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${default_state_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And the workload "after_backend" shall have the execution state "Succeeded(Ok)" on agent "agent_A" within "20" seconds
    # Actions
    When user triggers "ank set state -f ${new_state_yaml_file} desiredState.workloads.after_backend"
    And the workload "after_backend" shall have the execution state "Pending(WaitingToStart)" on agent "agent_A" within "3" seconds
    And user triggers "ank set state -f ${new_state_yaml_file} desiredState.workloads.backend"
    # Asserts
    Then the workload "backend" shall have the execution state "Succeeded(Ok)" on agent "agent_A" within "5" seconds
    And the workload "after_backend" shall have the execution state "Succeeded(Ok)" on agent "agent_A" within "5" seconds
    [Teardown]    Clean up Ankaios
