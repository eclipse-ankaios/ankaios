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
Test Ankaios observes the inter-workload dependencies when creating workloads
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/create_workloads_with_dependencies_config.yaml"
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    # Actions
    # Asserts
    Then the workload "logger" shall have the execution state "Pending(WaitingToStart)" on agent "agent_A"
    And Then the workload "error_notifier" shall have the execution state "Pending(WaitingToStart)" on agent "agent_A"
    And the workload "storage_provider" shall have the execution state "Pending(WaitingToStart)" on agent "agent_B"
    And the workload "filesystem_init" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "30" seconds
    And the workload "storage_provider" shall have the execution state "Running(Ok)" on agent "agent_B"
    And the workload "logger" shall have the execution state "Running(Ok)" on agent "agent_B"
    And the workload "storage_provider" shall have the execution state "Failed(ExecFailed)" on agent "agent_B"
    And the workload "error_notifier" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios


Test Ankaios observes the inter-workload dependencies when deleting workloads
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/delete_workloads_with_dependencies.yaml"
    And Ankaios agent is started with name "agent_A"
    And the workload "frontend" shall have the execution state "Running(Ok)" on agent "agent_A" within "30" seconds
    # Actions
    When user triggers "ank delete workload backend"
    And the workload "backend" shall have the execution state "Running(WaitingToStop)" on agent "agent_A"
    And user triggers "ank delete workload frontend"
    # Asserts
    Then the workload "backend" shall not exist on agent "agent_A" within "60" seconds
    [Teardown]    Clean up Ankaios
