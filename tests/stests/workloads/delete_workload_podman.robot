*** Comments ***
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
Documentation       Tests to verify that Ankaios can delete Podman workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***

# [stest->swdd~agent-supports-podman~2]
# [stest->swdd~podman-delete-workload-stops-and-removes-workload~1]
Test Ankaios Podman remove workloads
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    And the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    # Actions
    When user triggers "ank --no-wait -k delete workload sleepy"
    # Asserts
    Then the workload "sleepy" shall be removed and not exist on agent "agent_A" within "20" seconds
    And the workload "hello1" shall have the execution state "Failed(Lost)" on agent "agent_B" within "20" seconds
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B" within "20" seconds
    And podman shall not have a container for workload "hello1" on agent "agent_B"
    And podman shall have a container for workload "hello2" on agent "agent_B"
    And podman shall have a container for workload "hello3" on agent "agent_B"
    And podman shall not have a container for workload "sleepy" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios
