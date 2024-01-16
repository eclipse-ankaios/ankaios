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
    Given Ankaios server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "agent_B"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get workloads"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    And podman shall not have a container for workload "hello1" on agent "agent_B" within "30" seconds
    And the workload "hello2" shall have the execution state "Succeeded" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded" on agent "agent_B"
    [Teardown]    Clean up Ankaios
