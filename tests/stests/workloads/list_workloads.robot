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
Documentation    Tests to verify that ank cli lists workloads correctly.
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios CLI get workloads
    [Setup]        Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_B"
    And Ankaios agent is started with name "agent_A"
    And the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Failed(Lost)" on agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    # Actions
    When user triggers "ank -k get workloads"
    # Asserts
    Then in the last result, the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    And in the last result, the workload "hello1" shall have the execution state "Failed(Lost)" on agent "agent_B"
    And in the last result, the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And in the last result, the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B"

    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-loads-config-file~1]
Test Ankaios CLI get workloads with config files
    [Setup]    Setup Ankaios    mtls_enabled=True
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    And the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Failed(Lost)" on agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    # Actions
    When user triggers "ank -x ${CONFIGS_DIR}/ank.conf get workloads"
    # Asserts
    Then in the last result, the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    And in the last result, the workload "hello1" shall have the execution state "Failed(Lost)" on agent "agent_B"
    And in the last result, the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And in the last result, the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    [Teardown]    Clean up Ankaios
