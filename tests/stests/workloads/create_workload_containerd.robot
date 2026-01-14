*** Comments ***
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
Documentation       Tests to verify that Ankaios can create containerd workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***

# [stest->swdd~agent-supports-containerd~1]
# [stest->swdd~containerd-create-workload-runs-workload~1]
Test Ankaios containerd create workloads
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default_containerd.yaml"
    And the CLI listens for workload states
    # Actions
    When Ankaios agent is started with name "agent_B"
    And Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And containerd shall have a container for workload "sleepy" on agent "agent_A"
    And containerd shall have a container for workload "hello2" on agent "agent_B"
    And containerd shall have a container for workload "hello3" on agent "agent_B"
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-supports-containerd~1]
# [stest->swdd~containerd-create-workload-sets-optionally-container-name~1]
Test Ankaios containerd create a container with custom name
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/create_workload_custom_name_containerd.yaml"
    And the CLI listens for workload states
    # Actions
    When Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And containerd shall have a container for workload "sleepy" with custom name "test_workload1" on agent "agent_A"
    [Teardown]    Clean up Ankaios
