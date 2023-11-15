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
Documentation    Tests to verify that ank cli updates workloads correctly
...              by adapting the portmapping from the host port 8081 to host port 8082.
Resource    ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Variables ***
${default_state_yaml_file}
${new_state_yaml_file} 

*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios CLI update workload
    [Setup]        Run Keywords    Setup Ankaios
    ...            AND             Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/default.yaml
    ...            AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_update_workload_new_state.yaml
    # Preconditions
    Given Ankaios server is started with config "${default_state_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    And the command "curl localhost:8081" finished with exit code "0"
    # Actions
    When user triggers "ank get state > ${new_state_yaml_file}" 
    And user updates the state "${new_state_yaml_file}" with "currentState.workloads.nginx.runtimeConfig.commandOptions=['-p', '8082:80']"
    And user triggers "ank set state -f ${new_state_yaml_file} currentState.workloads.nginx"
    And user triggers "ank get workloads" 
    # Asserts
    Then the workload "nginx" shall have the execution state "Running"
    And the command "curl localhost:8082" shall finish with exit code "0"
    [Teardown]    Clean up Ankaios
