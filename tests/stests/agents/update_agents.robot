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
Documentation       Tests to verify that agent tags can be updated through state updates

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Variables ***
${agent_name}       agent_A

*** Test Cases ***

# [stest->swdd~server-state-updates-agent-tags~1]
Test Ankaios updates only agent tags through state updates
    [Setup]           Run Keywords    Setup Ankaios for Control Interface test

    # Preconditions: Start server and agent with initial tags
    Given Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "${agent_name}" and tags "type=AI-agent location=online"
    And the CLI listens for workload states
    # Verify initial tags via CLI
    When user triggers "ank -k get state"
    Then the agent "${agent_name}" shall have tag "type" with value "AI-agent"
    And the agent "${agent_name}" shall have tag "location" with value "online"

    # Action: Update state via control interface tester
    And the controller workload is allowed to write on agents
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/update_agent_tags.yaml" and update mask agents
    And the controller workload shall execute successfully on running system

    # Verify updated state
    When user triggers "ank -k get agents"
    Then the last command shall list exactly "1" agents
    And the last command shall list the connected agent "${agent_name}"

    When user triggers "ank -k get state"
    Then the agent "${agent_name}" shall have tag "location" with value "on-car"
    And the agent "${agent_name}" shall have tag "new_tag" with value "value"
    And the agent "${agent_name}" shall not have tag "type"
    And the agent "agent_C" shall not exist

    [Teardown]    Clean up Ankaios
