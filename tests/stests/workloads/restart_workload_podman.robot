# Copyright (c) 2023 Elektrobit Automotive GmbH
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
Documentation    Tests to verify that restart behavior on failing create of workload works correctly
Resource    ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Variables ***
${default_state_yaml_file}
${new_state_yaml_file}

*** Test Cases ***

# [itest->swdd~agent-restart-workload-on-create-failure~1]
# [itest->swdd~agent-workload-control-loop-executes-restart~1]
# [itest->swdd~agent-workload-control-loop-request-restarts~1]
Test Ankaios Podman restart of a workload on creation failure
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_restart_workload.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/restart_workload.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get state > ${new_state_yaml_file}"
    And user triggers "ank delete workload hello1"
    And the workload "hello1" shall not exist
    And the user waits "1" seconds
    And podman shall have a container for workload "hello1" on agent "agent_A"
    And user triggers "ank set state -f ${new_state_yaml_file} currentState.workloads.hello1"
    # Asserts
    Then the workload "hello1" shall have the execution state "Running" from agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [itest->swdd~agent-restart-workload-on-create-failure~1]
# [itest->swdd~agent-workload-control-loop-executes-restart~1]
# [itest->swdd~agent-workload-control-loop-request-restarts~1]
# [itest->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
Test Ankaios Podman restart of a workload on creation failure intercepted by update
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_restart_workload.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/restart_workload.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get state > ${new_state_yaml_file}"
    And user triggers "ank delete workload hello1"
    And the workload "hello1" shall not exist
    And the user waits "1" seconds
    And podman shall have a container for workload "hello1" on agent "agent_A"
    And user triggers "ank set state -f ${new_state_yaml_file} currentState.workloads.hello1"
    And user updates the state "${new_state_yaml_file}" with "currentState.workloads.hello1.runtimeConfig.commandArgs=['3']"
    And user triggers "ank set state -f ${new_state_yaml_file} currentState.workloads.hello1"
    # Asserts
    Then the workload "hello1" shall have the execution state "Succeeded" from agent "agent_A" within "30" seconds
    [Teardown]    Clean up Ankaios

# [itest->swdd~agent-restart-workload-on-create-failure~1]
# [itest->swdd~agent-workload-control-loop-executes-restart~1]
# [itest->swdd~agent-workload-control-loop-request-restarts~1]
# [itest->swdd~agent-workload-control-loop-prevent-restarts-on-other-workload-commands~1]
Test Ankaios Podman restart of a workload on creation failure intercepted by delete
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_restart_workload.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/restart_workload.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get state > ${new_state_yaml_file}"
    And user triggers "ank delete workload hello1"
    And the workload "hello1" shall not exist
    And podman shall have a container for workload "hello1" on agent "agent_A"
    And user triggers "ank set state -f ${new_state_yaml_file} currentState.workloads.hello1"
    And the user waits "1" seconds
    And user triggers "ank delete workload hello1"
    # Asserts
    Then the workload "hello1" shall not exist
    podman shall not have a container for workload "hello1" on agent "agent_A" within "10" seconds
    [Teardown]    Clean up Ankaios

# [itest->swdd~agent-restart-workload-on-create-failure~1]
# [itest->swdd~agent-workload-control-loop-executes-restart~1]
# [itest->swdd~agent-workload-control-loop-request-restarts~1]
# [itest->swdd~agent-workload-control-loop-limit-restart-attempts~1]
Test Ankaios Podman stop restarts after reaching the restart attempt limit
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_restart_workload_reach_limit.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/restart_workload_reach_limit.yaml"
    And Ankaios agent is started with name "agent_A"
    # Actions
    When the user waits "22" seconds
    # Asserts
    Then the workload "hello1" shall have the execution state "" on agent "agent_A"
    [Teardown]    Clean up Ankaios