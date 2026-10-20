*** Comments ***
# Copyright (c) 2026 TRATON AB
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
Documentation       Tests to verify that the Ankaios CLI shows the changes of an update in dry run mode without applying them.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~cli-provides-dry-run-option~1]
# [stest->swdd~cli-sends-update-state-dry-run~1]
# [stest->swdd~cli-presents-dry-run-result~1]
Test Ankaios apply dry run shows added workloads without applying them
    [Setup]           Setup Ankaios

    # Preconditions
    Given Ankaios server is started without manifest successfully
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank apply --dry-run ${CONFIGS_DIR}/manifest1.yaml"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the dry run result shall contain the workload "sleepy_from_manifest1" on agent "agent_A" with action "added"
    And the workload "sleepy_from_manifest1" shall not exist
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-provides-dry-run-option~1]
# [stest->swdd~cli-sends-update-state-dry-run~1]
# [stest->swdd~cli-presents-dry-run-result~1]
Test Ankaios apply dry run shows a workload moved to another agent without applying it
    [Setup]           Setup Ankaios

    # Preconditions
    Given Ankaios server is started with manifest "${CONFIGS_DIR}/manifest1.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload "sleepy_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    # Actions
    When user triggers "ank apply --dry-run --agent agent_B ${CONFIGS_DIR}/manifest1.yaml"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the dry run result shall contain the workload "sleepy_from_manifest1" on agent "agent_A -> agent_B" with action "updated"
    And the workload "sleepy_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-provides-dry-run-option~1]
# [stest->swdd~cli-sends-update-state-dry-run~1]
# [stest->swdd~cli-presents-dry-run-result~1]
Test Ankaios set state dry run shows deleted workloads without applying it
    [Setup]           Setup Ankaios

    # Preconditions
    Given Ankaios server is started with manifest "${CONFIGS_DIR}/manifest1.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload "sleepy_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    # Actions
    When user triggers "ank set state --dry-run desiredState.workloads.sleepy_from_manifest1 ${CONFIGS_DIR}/empty.yaml"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the dry run result shall contain the workload "sleepy_from_manifest1" on agent "agent_A" with action "deleted"
    And the workload "sleepy_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios
