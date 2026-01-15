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
Documentation       Tests to verify that ank cli properly supports field mask operations with state commands

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${default_state_yaml_file}      ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios CLI update workload
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}     ${CONFIGS_DIR}/simple.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}         ${CONFIGS_DIR}/minimal_set_state.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${default_state_yaml_file}"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have left the initial execution state
    # Actions
    And user triggers "ank -k --no-wait set state desiredState.workloads.simple.agent ${new_state_yaml_file}"
    # Asserts
    Then the workload "simple" shall have the execution state "Removed" on agent "agent_A" within "20" seconds
    And podman shall not have a container for workload "simple" on agent "agent_A"
    [Teardown]    Clean up Ankaios

#[stest->swdd~server-filters-get-complete-state-result-with-wildcards~1]
Test Ankaios CLI get workloads with wildcard
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}     ${CONFIGS_DIR}/default.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${default_state_yaml_file}"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have left the initial execution state
    # Actions
    And user triggers "ank -k get state -o json 'desiredState.workloads.*.agent' 'desiredState.workloads.*.runtime'"
    # Asserts
    Then the last command shall contain the workload "sleepy"
    And the last command shall contain the workload "hello1"
    And the last command shall contain the workload "hello2"
    And the last command shall contain the workload "hello3"
    And the last command shall only contain agent and runtime
    # And the last command shall only return the agent of the workloads
    [Teardown]    Clean up Ankaios
