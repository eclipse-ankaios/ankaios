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
Documentation       Tests to verify that Ankaios deletes workloads of disconnected agents properly

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Test Cases ***

Test Ankaios deletes pending initial podman workloads properly
    [Documentation]    Delete workload properly that is Pending(Initial) and its agent has never connected to the server
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/delete_pending_initial_podman_workload.yaml"
    And Ankaios agent is started with name "agent_A"
    And the workload "hello1" shall have the execution state "Succeeded(Ok)" on agent "agent_A" within "5" seconds
    And the workload "pending_initial_workload" shall have the execution state "Pending(Initial)" from agent "never_connected_agent"
    And user triggers "ank --insecure --no-wait set state desiredState ${CONFIGS_DIR}/emptyState.yaml"
    # Asserts
    Then the workload "pending_initial_workload" shall not exist
    [Teardown]    Clean up Ankaios
