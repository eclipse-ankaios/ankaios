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
Documentation       Tests to verify that the Ankaios CLI lists connected agents

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~cli-provides-list-of-agents~1]
Test Ankaios CLI lists connected agents
    [Setup]        Setup Ankaios
    # Preconditions
    # This test assumes that all containers in Podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    # The agent_B is not started and thus not connected
    And the workload "hello1" shall have the execution state "Pending(Initial)" on agent "agent_B"
    And the workload "hello2" shall have the execution state "Pending(Initial)" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Pending(Initial)" on agent "agent_B"
    # Actions
    When user triggers "ank -k get agents"
    # Asserts
    Then the last command shall list the connected agent "agent_A"

    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-naming-convention~1]
# [stest->swdd~agent-loads-config-file~2]
Test Ankaios CLI enforces agent naming convention
    [Setup]        Setup Ankaios
    # Preconditions
    # This test assumes that all containers in Podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "agent.A"
    And Ankaios agent is started with config file "${CONFIGS_DIR}/ank-agent_invalid_agent_name.conf"
    # Actions
    When user triggers "ank -k get agents"
    ${result_cli}=  Run Keyword And Return Status    the last command shall list the connected agent "agent.A"
    ${result_config}=  Run Keyword And Return Status    the last command shall list the connected agent "Invalid@gent.name"
    # Asserts
    Pass Execution If    ${result_cli} == False    The agent name "agent.A" is not allowed
    Pass Execution If    ${result_config} == False    The agent name "Invalid@gent.name" is not allowed
    [Teardown]    Clean up Ankaios
