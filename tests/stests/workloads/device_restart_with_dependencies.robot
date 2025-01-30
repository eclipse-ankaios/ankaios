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

*** Test Cases ***
# [stest->swdd~agent-existing-workloads-replace-updated~3]
# [stest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
# [stest->swdd~agent-enqueues-unfulfilled-create~1]
# [stest->swdd~agent-existing-workloads-reuse-unmodified~1]
# [stest->swdd~podman-create-workload-starts-existing-workload~1]
Test Ankaios restarts exited workloads on device restart with considering inter-workload dependencies
    [Documentation]    Restart not running workloads after a device restart with considering inter-workload dependencies
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/device_restart_with_dependencies.yaml"
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    And podman has assigned a container id for workload "filesystem_init" on agent "agent_A"
    And podman has assigned a container id for workload "web_service_init" on agent "agent_A"
    And podman has assigned a container id for workload "web_service" on agent "agent_B"
    # Simulate full device restart
    And Ankaios server is terminated
    And Ankaios agent with name "agent_A" is terminated
    And Ankaios agent with name "agent_B" is terminated
    And Podman has terminated all existing containers
    # Restart of Ankaios on full device restart
    And Ankaios server is started with config "${CONFIGS_DIR}/device_restart_with_dependencies_modified_web_service_init.yaml"
    And Ankaios agent is started with name "agent_B"
    And Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "web_service" shall have the execution state "Pending(WaitingToStart)" on agent "agent_B"
    And the workload "web_service" shall have the execution state "Running(Ok)" on agent "agent_B"
    And the container of workload "filesystem_init" shall have the same id and same configuration on the podman runtime
    And the container of workload "web_service_init" shall have a different id on the podman runtime
    And the container of workload "web_service" shall have the same id and same configuration on the podman runtime
    [Teardown]    Clean up Ankaios
