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
Documentation       Tests to verify that ank get events command works correctly.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***

# [stest->swdd~cli-provides-get-events-command~1]
# [stest->swdd~cli-subscribes-for-events~1]
# [stest->swdd~cli-receives-events~1]
# [stest->swdd~cli-supports-multiple-output-types-for-events~1]
Test Ankaios CLI get events with field mask filter
    [Documentation]    Subscribe to events with field mask and verify filtered output
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started without manifest successfully
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user starts the CLI to subscribe to events with format "yaml" and field mask "desiredState.workloads" in background
    And user triggers "ank -k apply ${CONFIGS_DIR}/nginx.yaml"
    And the user waits "3" seconds
    # Asserts
    Then the event output shall contain workload "nginx"
    And the event output shall be valid yaml format
    And the event output shall contain only desiredState workloads
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-provides-get-events-command~1]
Test Ankaios CLI get events with initial complete state output
    [Documentation]    Subscribe to events and output initial complete state response
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/nginx.yaml"
    And Ankaios server is available
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user starts the CLI to subscribe to events with format "yaml", field mask "desiredState.workloads" and current state output enabled in background
    And the user waits "1" seconds
    # Asserts
    Then the event output shall contain workload "nginx"
    And the event output shall be valid yaml format
    And the event output shall contain only desiredState workloads
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-receives-events~1]
# [stest->swdd~cli-outputs-events-with-timestamp~1]
# [stest->swdd~cli-supports-multiple-output-types-for-events~1]
Test Ankaios CLI get events receives multiple sequential events
    [Documentation]    Verify multiple events are received when state changes occur
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/simple.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload "simple" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    # Actions
    When user starts the CLI to subscribe to events with format "json" and field mask "" in background
    And the user waits "3" seconds
    And user triggers "ank -k apply ${CONFIGS_DIR}/nginx.yaml"
    And the user waits "3" seconds
    And user triggers "ank -k apply ${CONFIGS_DIR}/manifest1.yaml"
    And the user waits "5" seconds
    # Asserts
    Then the event output shall contain at least "2" events
    And all events shall contain timestamp
    And the event output shall be valid json format
    [Teardown]    Clean up Ankaios


# [stest->swdd~cli-handles-event-subscription-errors~1]
Test Ankaios CLI get events handles connection errors gracefully
    [Documentation]    Verify error handling when server is not available
    [Setup]    Setup Ankaios

    # Preconditions
    # Actions
    When user triggers "ank -k get events"
    # Asserts
    Then the last command shall finish with an error
    [Teardown]    Clean up Ankaios


# [stest->swdd~cli-receives-events~1]
Test Ankaios CLI get events with workload state changes
    [Documentation]    Verify events are received when workload states change
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/simple.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload "simple" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    # Actions
    When user starts the CLI to subscribe to events with format "yaml" and field mask "workloadStates" in background
    And the user waits "2" seconds
    And user triggers "ank -k apply ${CONFIGS_DIR}/nginx.yaml"
    And the user waits "3" seconds
    # Asserts
    Then the event output shall contain field name "workloadStates"
    And the event output shall be valid yaml format
    [Teardown]    Clean up Ankaios


# [stest->swdd~cli-outputs-events-with-timestamp~1]
Test Ankaios CLI get events includes timestamp in output
    [Documentation]    Verify all events include RFC3339 formatted timestamps
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/simple.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload "simple" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    # Actions
    When user starts the CLI to subscribe to events with format "json" and field mask "" in background
    And the user waits "3" seconds
    And user triggers "ank -k apply ${CONFIGS_DIR}/nginx.yaml"
    And the user waits "5" seconds
    # Asserts
    Then the event output shall contain timestamp in RFC3339 format
    [Teardown]    Clean up Ankaios


# [stest->swdd~cli-receives-events~1]
Test Ankaios CLI get events with empty field mask
    [Documentation]    Verify events with empty field mask return complete state
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    # Actions
    When user starts the CLI to subscribe to events with format "yaml" and field mask "" in background
    And the user waits "3" seconds
    And user triggers "ank -k apply ${CONFIGS_DIR}/nginx.yaml"
    And the user waits "5" seconds
    # Asserts
    Then the event output shall contain field name "desiredState"
    And the event output shall contain field name "workloadStates"
    [Teardown]    Clean up Ankaios


# [stest->swdd~cli-receives-events~1]
Test Ankaios CLI get events with workload deletion
    [Documentation]    Verify events are received when workloads are deleted
    [Setup]    Setup Ankaios

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with manifest "${CONFIGS_DIR}/default.yaml"
    And the CLI listens for workload states
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    And the workload "sleepy" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Failed(Lost)" on agent "agent_B"
    # Actions
    When user starts the CLI to subscribe to events with format "json" and field mask "desiredState.workloads" in background
    And the user waits "3" seconds
    And user triggers "ank -k delete workload sleepy hello1"
    And the user waits "5" seconds
    # Asserts
    Then the event output shall contain altered fields with removed workloads
    And the event output shall be valid json format
    [Teardown]    Clean up Ankaios

