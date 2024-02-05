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
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios observes the inter-workload dependencies when creating workloads
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    # Asserts
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A" within "30" seconds
    And the command "curl localhost:8082" shall finish with exit code "0" within "10" seconds
    [Teardown]    Clean up Ankaios
