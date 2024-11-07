*** Comments ***
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
Documentation       Tests the authorization of the Control Interface access from workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

Test Setup       Setup Ankaios for Control Interface test
Test Teardown    Clean up Ankaios

*** Variables ***
${simple_yaml_file}      ${EMPTY}
${manifest12_yaml_file}  ${EMPTY}
${manifest1_yaml_file}   ${EMPTY}
${manifest2_yaml_file}   ${EMPTY}
${manifest_no_agent_name_yaml_file}    ${EMPTY}

# [stest->swdd~agent-checks-request-for-authorization~1]
*** Test Cases ***

No rules
    Given the controller workload has no access rights

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags
    And the controller workload gets the state
    And the controller workload gets the state of fields desiredState.workloads.simple_existing
    And the controller workload gets the state of fields desiredState.workloads.simple_existing.tags
    And the controller workload gets the state of fields desiredState.workloads.controller

    Then the controller workload has no access to Control Interface

Allow write rule with empty string allows all writes
    Given the controller workload is allowed to write on ${EMPTY}

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all succeed

Allow write rule with empty string denies all reads
    Given the controller workload is allowed to write on ${EMPTY}

    When the controller workload gets the state
    And the controller workload gets the state of fields desiredState.workloads.simple_existing
    And the controller workload gets the state of fields desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all fail

Allow read rule with empty string denies all writes
    Given the controller workload is allowed to read on ${EMPTY}

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all fail

Allow read rule with empty string allows all reads
    Given the controller workload is allowed to read on ${EMPTY}

    When the controller workload gets the state
    And the controller workload gets the state of fields desiredState.workloads.simple_existing
    And the controller workload gets the state of fields desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all succeed

Allow read write rule with empty string allows allow read and writes
    Given the controller workload is allowed to read and write on ${EMPTY}

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags
    And the controller workload gets the state
    And the controller workload gets the state of fields desiredState.workloads.simple_existing
    And the controller workload gets the state of fields desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all succeed

Allow write rule for only tags allows write to tags
    Given the controller workload is allowed to write on desiredState.workloads.*.tags

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all succeed

Allow write rule for only tags denies everythings execept write to tags
    Given the controller workload is allowed to write on desiredState.workloads.*.tags

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets the state
    And the controller workload gets the state of fields desiredState.workloads.simple_existing
    And the controller workload gets the state of fields desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all fail

Allow read rule for only tags allows read from tags
    Given the controller workload is allowed to read on desiredState.workloads.*.tags

    When the controller workload gets the state of fields desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all succeed

Allow read rule for only tags denies everythings execept read from tags
    Given the controller workload is allowed to read on desiredState.workloads.*.tags

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags
    And the controller workload gets the state
    And the controller workload gets the state of fields desiredState.workloads.simple_existing

    Then the controller workload requests shall all fail


Allow read write rule for workloads except write to simple_existing allows all read and write on workloads execept write to simple_existing
    Given the controller workload is allowed to read and write on desiredState.workloads
    And the controller workload is forbidden to to write on desiredState.workloads.simple_existing

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets the state of fields desiredState.workloads.simple_existing
    And the controller workload gets the state of fields desiredState.workloads.simple_existing.tags

    Then the controller workload requests shall all succeed

Allow read write rule for workloads except write to simple_existing forbids writing to simple_existing and reading whole state
    Given the controller workload is allowed to read and write on desiredState.workloads
    And the controller workload is forbidden to to write on desiredState.workloads.simple_existing

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple_existing.tags
    And the controller workload gets the state

    Then the controller workload requests shall all fail
