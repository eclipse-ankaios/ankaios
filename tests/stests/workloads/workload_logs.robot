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
Documentation       Tests to verify that Ankaios outputs logs for multiple workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~cli-provides-workload-logs~1]
# [stest->swdd~cli-outputs-logs-in-specific-format~1]
Test Ankaios outputs logs for multiple workloads with disabled follow mode
    [Documentation]    Output logs for multiple workloads with disabled follow mode over multiple agents.
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/workload_logs.yaml"
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    # Asserts
    Then the workload "count_to_five" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    And the workload "count_to_three" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And the cli log collection shall output "8" log lines in total in the expected format for all workloads
    [Teardown]    Clean up Ankaios

