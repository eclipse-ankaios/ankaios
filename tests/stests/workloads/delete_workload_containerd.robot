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
Documentation       Tests to verify that Ankaios can delete containerd workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***

# [stest->swdd~agent-supports-containerd~1]
# [stest->swdd~containerd-delete-workload-stops-and-removes-workload~1]
Test Ankaios containerd remove workloads
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given containerd has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default_containerd.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_B"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A and agent_B" have left the initial execution state
    # Actions
    When user triggers "ank -k delete workload sleepy"
    # Asserts
    Then the workload "sleepy" shall not exist on agent "agent_A" within "20" seconds
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And containerd shall have a container for workload "hello2" on agent "agent_B"
    And containerd shall have a container for workload "hello3" on agent "agent_B"
    And containerd shall not have a container for workload "sleepy" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios
