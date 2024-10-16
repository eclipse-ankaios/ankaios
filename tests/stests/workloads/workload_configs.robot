# Copyright (c) 2023 Elektrobit Automotive GmbH
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
Documentation       Tests to verify that ank cli updates workloads correctly
...                 by adapting the portmapping from the host port 8081 to host port 8082.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${start_up_yaml_file}           ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
Test Ankaios CLI updates a config item a workload is referncing
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${start_up_yaml_file}    ${CONFIGS_DIR}/manifest_with_configs.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}   ${CONFIGS_DIR}/update_state_updated_config_item.yaml
    [Tags]    run_now
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${start_up_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    And the command "curl localhost:8081" finished with exit code "0"
    # Actions
    When user triggers "ank -k set state desiredState.configs ${new_state_yaml_file}"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the command "curl localhost:8082" shall finish with exit code "0" within "10" seconds
    [Teardown]    Clean up Ankaios
