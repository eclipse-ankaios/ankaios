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
Documentation    Tests to verify that ank cli lists configs correctly.
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***
Test Ankaios CLI get configs
    [Setup]        Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "agent_B"
    And the agent "agent_B" shall have an configuration
    And Ankaios agent is started with name "agent_A"
    And the agent "agent_A" shall have an configuration
    # Actions
    When user triggers "ank -k get workloads"
    # Asserts
    Then the output should contain TODO
    [Teardown]    Clean up Ankaios

Test Ankaios Podman remove workloads
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "agent_B"
    And the agent "agent_B" shall have an configuration
    And Ankaios agent is started with name "agent_A"
    And the agent "agent_A" shall have an configuration
    # Actions
    When user triggers "ank -k delete configuration"
    # Asserts
    Then the workload TODO: configuration shall not exist for agent "agent_A"
    And the workload TODO: configuration shall not exist for agent "agent_B"
    [Teardown]    Clean up Ankaios
