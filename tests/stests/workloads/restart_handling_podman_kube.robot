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
Documentation       Tests to verify that Ankaios creates and deletes workloads
...                 with inter-workload dependencies properly.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${default_state_yaml_file}      ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***
Test Ankaios restarts podman kube workloads with restart policy ALWAYS.
    [Documentation]    Restart workloads with restart policy set to ALWAYS on runtime podman-kube
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/state_with_restart_policies.yaml"
    And Ankaios agent is started with name "agent_B"
    And the workload "kube_restarted_always" shall have the execution state "Running(Ok)" on agent "agent_A"
    # Asserts
    Then the workload "kube_restarted_always" shall have the execution state "Pending(Starting)" on agent "agent_A"
    Then the workload "kube_restarted_always" shall have the execution state "Running(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios

Test Ankaios restarts podman kube workloads on device restart with restart policy set to ALWAYS.
    [Documentation]    Restart workloads on runtime podman-kube with restart policy set to ALWAYS on device restart.
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/state_with_restart_policies.yaml"
    And Ankaios agent is started with name "agent_B"
    And the workload "kube_restarted_always" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "default_restarted_never" shall have the execution state "Running(Ok)" on agent "agent_B"
    And Ankaios server is terminated
    And Ankaios agent with name "agent_B" is terminated
    And Ankaios server is started with config "${CONFIGS_DIR}/state_with_restart_policies.yaml"
    And Ankaios agent is started with name "agent_B"
    # Asserts
    Then the workload "kube_restarted_always" shall have the execution state "Pending(Starting)" on agent "agent_A"
    Then the workload "kube_restarted_always" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "restarted_never" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    And the workload "default_restarted_never" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    [Teardown]    Clean up Ankaios
