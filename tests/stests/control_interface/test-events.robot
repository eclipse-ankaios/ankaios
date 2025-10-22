*** Comments ***
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


Test events
    Given the controller workload is allowed to read and write on *

    When the controller workload subscribes to the state of fields desiredState.workloads.*.agent
    And the controller workload updates the state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask desiredState.workloads.simple
    And the controller workload get events for fields desiredState.workloads.*.agent
    Then the last result contains exactly the workloads simple
    And in the last result the workload simple has exactly the fields agent

    When the controller workload cancels events for fields desiredState.workloads.*.agent
    Then the controller workload requests shall all succeed
