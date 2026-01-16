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
${new_state_yaml_file}

*** Test Cases ***

# [stest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
# [stest->swdd~agent-workload-control-loop-executes-retry~1]
Test Ankaios containerd retry creation of a workload on creation failure
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/containerd_invalid_image.yaml"
    And Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload state of workload "invalid_image_workload" shall contain an additional info signaling retries within "5" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
# [stest->swdd~agent-workload-control-loop-executes-retry~1]
# [stest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
Test Ankaios containerd retry creation of a workload on creation failure intercepted by update
    [Setup]    Run Keywords    Setup Ankaios
    ...        AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_containerd_invalid_image.yaml

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/containerd_invalid_image.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload state of workload "invalid_image_workload" shall contain an additional info signaling retries within "5" seconds
    # Actions
    When user triggers "ank -k get state > ${new_state_yaml_file}"
    And user triggers "ank -k set state ${new_state_yaml_file} desiredState.workloads.invalid_image_workload"
    And user updates the state "${new_state_yaml_file}" with "desiredState.workloads.invalid_image_workload.runtimeConfig.image=ghcr.io/eclipse-ankaios/tests/alpine:latest"
    And user triggers "ank -k set state desiredState.workloads.invalid_image_workload ${new_state_yaml_file}"
    # Asserts
    Then the workload "invalid_image_workload" shall have the execution state "Succeeded(Ok)" from agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-workload-control-loop-retries-workload-creation-on-create-failure~1]
# [stest->swdd~agent-workload-control-loop-executes-retry~1]
# [stest->swdd~agent-workload-control-loop-prevents-retries-on-other-workload-commands~2]
Test Ankaios containerd retry creation of a workload on creation failure intercepted by delete
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/containerd_invalid_image.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload state of workload "invalid_image_workload" shall contain an additional info signaling retries within "5" seconds
    # Actions
    When user triggers "ank -k --no-wait delete workload invalid_image_workload"
    # Asserts
    Then the workload "invalid_image_workload" shall be removed and not exist on agent "agent_A" within "20" seconds
    And containerd shall not have a container for workload "invalid_image_workload" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios
