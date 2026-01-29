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
Documentation       Test the Control Interface can subscribe to events

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

Test Setup       Setup Ankaios for Control Interface test
Test Teardown    Clean up Ankaios

*** Test Cases ***

# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events no wildcard
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.simple
    Then the last result contains exactly the workloads simple
    And the last result has added fields desiredState.workloads.simple
    And the last result has no updated fields
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_different_agent_add_dependencies_and_toplevel_configs.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.simple
    Then the last result contains exactly the workloads simple
    And the last result has added fields desiredState.workloads.simple.dependencies
    And the last result has updated fields desiredState.workloads.simple.agent
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/empty.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.simple
    Then The last result contains no workloads
    And the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields desiredState.workloads.simple

    When the controller workload cancels events for fields desiredState.workloads.simple
    Then the controller workload requests shall all succeed

# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events workload name wildcard only get agent
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields desiredState.workloads.*.agent
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.*.agent
    Then the last result contains exactly the workloads simple
    And the last result has added fields desiredState.workloads.simple.agent
    And the last result has no updated fields
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_different_agent_add_dependencies_and_toplevel_configs.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.*.agent
    Then the last result contains exactly the workloads simple
    And the last result has no added fields
    And the last result has updated fields desiredState.workloads.simple.agent
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/empty.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.*.agent
    Then The last result contains no workloads
    And the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields desiredState.workloads.simple.agent

    When the controller workload cancels events for fields desiredState.workloads.*.agent
    Then the controller workload requests shall all succeed

# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events with multiple wildcards
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields desiredState.workloads.*.*.some_other_workload
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_different_agent_add_dependencies_and_toplevel_configs.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.*.*.some_other_workload
    Then the last result contains exactly the workloads simple
    And the last result has added fields desiredState.workloads.simple.dependencies.some_other_workload
    And the last result has no updated fields
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/empty.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields desiredState.workloads.*.*.some_other_workload
    Then The last result contains no workloads
    And the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields desiredState.workloads.simple.dependencies.some_other_workload

    When the controller workload cancels events for fields desiredState.workloads.*.*.some_other_workload
    Then the controller workload requests shall all succeed

# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events on configs
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields desiredState.configs
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_different_agent_add_dependencies_and_toplevel_configs.yaml" and update mask desiredState.configs
    And the controller workload gets event for fields desiredState.configs
    And the last result has added fields desiredState.configs
    And the last result has no updated fields
    And the last result has no removed fields

    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_update_toplevel_configs.yaml" and update mask desiredState.configs
    And the controller workload gets event for fields desiredState.configs
    And the last result has no added fields
    And the last result has updated fields desiredState.configs.some_config.some_sub_config.sub_param1 and desiredState.configs.some_config.param1
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/empty.yaml" and update mask desiredState.configs
    And the controller workload gets event for fields desiredState.configs
    Then The last result contains no workloads
    And the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields desiredState.configs

    When the controller workload cancels events for fields desiredState.configs
    Then the controller workload requests shall all succeed

# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events on configs two wildcards
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields desiredState.configs.*.*
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_different_agent_add_dependencies_and_toplevel_configs.yaml" and update mask desiredState.configs
    And the controller workload gets event for fields desiredState.configs.*.*
    Then the last result has added fields desiredState.configs.some_config.param1, desiredState.configs.some_config.param2 and desiredState.configs.some_config.some_sub_config
    And the last result has no updated fields
    And the last result has no removed fields

    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state_update_toplevel_configs.yaml" and update mask desiredState.configs
    And the controller workload gets event for fields desiredState.configs.*.*
    And the last result has no added fields
    And the last result has updated fields desiredState.configs.some_config.param1 and desiredState.configs.some_config.some_sub_config.sub_param1
    And the last result has no removed fields

    When the controller workload updates the state with manifest "${CONFIGS_DIR}/empty.yaml" and update mask desiredState.configs
    And the controller workload gets event for fields desiredState.configs.*.*
    Then The last result contains no workloads
    And the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields desiredState.configs.some_config.param1, desiredState.configs.some_config.param2 and desiredState.configs.some_config.some_sub_config

    When the controller workload cancels events for fields desiredState.configs.*.*
    Then the controller workload requests shall all succeed

# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events on workload states
    Given the controller workload is allowed to read and write on *
    And The controller workload wait for 1000 milliseconds

    When the controller workload subscribes to the state of fields workloadStates
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/update_state_create_one_workload.yaml" and update mask desiredState.workloads.sleepy
    Then the controller workload gets event for fields workloadStates
    And the last result has added fields workloadStates.agent_A.sleepy
    And the last result has no updated fields
    And the last result has no removed fields

    When the controller workload gets event for fields workloadStates
    Then the last result has no added fields
    And the last result has updated fields workloadStates.agent_A.sleepy.*.subState and workloadStates.agent_A.sleepy.*.additionalInfo
    And the last result has no removed fields

    When the controller workload gets event for fields workloadStates
    Then the last result has no added fields
    And the last result has updated fields workloadStates.agent_A.sleepy.*.state, workloadStates.agent_A.sleepy.*.subState and workloadStates.agent_A.sleepy.*.additionalInfo
    And the last result has no removed fields


    When the controller workload updates the state with manifest "${CONFIGS_DIR}/empty.yaml" and update mask desiredState.workloads.sleepy
    And the controller workload gets event for fields workloadStates
    Then the last result has no added fields
    And the last result has updated fields workloadStates.agent_A.sleepy.*.state and workloadStates.agent_A.sleepy.*.subState
    And the last result has no removed fields

    When the controller workload gets event for fields workloadStates
    Then the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields workloadStates.agent_A.sleepy

    When the controller workload cancels events for fields workloadStates
    Then the controller workload requests shall all succeed


# [stest->swdd~server-stores-new-event-subscription~1]
# [stest->swdd~server-removes-event-subscription~1]
# [stest->swdd~server-sends-state-differences-as-events~1]
# [stest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
Test events on agents
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields agents.agent_B
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload gets event for fields agents.agent_B
    Then the last result has added fields agents.agent_B
    And the last result has no updated fields
    And the last result has no removed fields

    When the controller workload gets event for fields agents.agent_B
    Then the last result has added fields agents.agent_B.status
    And the last result has no updated fields
    And the last result has no removed fields

    When the controller workload gets event for fields agents.agent_B
    Then the last result has no added fields
    And the last result has no updated fields
    And the last result has removed fields agents.agent_B

    When the controller workload cancels events for fields agents.agent_B
    Then the controller workload requests shall all succeed with second agent
