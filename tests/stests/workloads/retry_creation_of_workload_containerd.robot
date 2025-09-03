# Copyright (c) 2025 Elektrobit Automotive GmbH
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

# [stest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
# [stest->swdd~agent-workload-control-loop-executes-retry~1]
Test Ankaios containerd retry creation of a workload on creation failure
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_containerd_alpine_sleep.yaml

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/containerd_alpine_sleep.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank -k get state > ${new_state_yaml_file}"
    And user triggers "ank -k delete workload hello1"
    And the workload "hello1" shall not exist on agent "agent_A" within "20" seconds
    And containerd shall not have a container for workload "hello1" on agent "agent_A" within "20" seconds
    And user triggers "ank -k set state desiredState.workloads.hello1 ${new_state_yaml_file}"
    # Asserts
    Then the workload "hello1" shall have the execution state "Running(Ok)" from agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
# [stest->swdd~agent-workload-control-loop-executes-retry~1]
# [stest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
Test Ankaios containerd retry creation of a workload on creation failure intercepted by update
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_containerd_alpine_sleep.yaml

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/containerd_alpine_sleep.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank -k get state > ${new_state_yaml_file}"
    And user triggers "ank -k delete workload hello1"
    And the workload "hello1" shall not exist on agent "agent_A" within "20" seconds
    And containerd shall not have a container for workload "hello1" on agent "agent_A" within "20" seconds
    And user triggers "ank -k set state ${new_state_yaml_file} desiredState.workloads.hello1"
    And user updates the state "${new_state_yaml_file}" with "desiredState.workloads.hello1.runtimeConfig.commandArgs=['3']"
    And user triggers "ank -k set state desiredState.workloads.hello1 ${new_state_yaml_file}"
    # Asserts
    Then the workload "hello1" shall have the execution state "Succeeded(Ok)" from agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
# [stest->swdd~agent-workload-control-loop-executes-retry~1]
# [stest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
Test Ankaios containerd retry creation of a workload on creation failure intercepted by delete
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_containerd_alpine_sleep.yaml

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/containerd_alpine_sleep.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank -k get state > ${new_state_yaml_file}"
    And user triggers "ank -k delete workload hello1"
    And the workload "hello1" shall not exist on agent "agent_A" within "20" seconds
    And containerd shall not have a container for workload "hello1" on agent "agent_A" within "20" seconds
    And user triggers "ank -k set state desiredState.workloads.hello1 ${new_state_yaml_file}"
    And the user waits "1" seconds
    And user triggers "ank -k delete workload hello1"
    # Asserts
    Then containerd shall not have a container for workload "hello1" on agent "agent_A" within "20" seconds
    And the workload "hello1" shall not exist on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios
