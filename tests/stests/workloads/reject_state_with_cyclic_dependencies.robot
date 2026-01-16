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
Documentation       Tests to verify that Ankaios rejects a state with a cycle in the interworkload dependencies config.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${default_state_yaml_file}      ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***

# [stest->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
# [stest->swdd~server-fails-on-invalid-startup-state~1]
Test Ankaios reject startup config with cyclic interworkload dependencies
    [Documentation]    The cycle is workload_A <-> workload_B inside startup config.
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    # The startup config is invalid because it contains a cycle inside the interworkload dependencies config
    When Ankaios server is started with an invalid config "${CONFIGS_DIR}/state_with_dependency_cycle.yaml"
    # Asserts
    Then the Ankaios server shall exit with an error code
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
# [stest->swdd~server-continues-on-invalid-updated-state~1]
# [stest->swdd~cycle-detection-ignores-non-existing-workloads~1]
Test Ankaios CLI update state with cycle in interworkload dependencies is rejected by Ankaios server
    [Documentation]    The cycle is workload_A -> workload_B -> workload_C -> workload_A inside the updated state.
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/default_config_with_dependencies.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}    ${CONFIGS_DIR}/update_state_config_with_dependency_cycle.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${default_state_yaml_file}"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_B"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A and agent_B" have left the initial execution state
    # Actions
    And user triggers "ank -k set state ${new_state_yaml_file} desiredState.workloads.workload_C"
    # Asserts
    Then the workload "workload_C" shall not exist
    And podman shall not have a container for workload "workload_C" on agent "agentA" within "5" seconds
    [Teardown]    Clean up Ankaios
