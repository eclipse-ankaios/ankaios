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
Documentation       Tests to verify that ank cli updates containerd workloads correctly
...                 by adapting the portmapping from the host port 8081 to host port 8082.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${default_state_yaml_file}      ${EMPTY}
${new_state_file}          ${EMPTY}


*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios CLI update workload for containerd runtime
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/containerd_nginx.yaml
    ...    AND    Set Global Variable    ${new_state_file}    %{ANKAIOS_TEMP}/itest_update_workload_new_state_containerd.yaml

    # Preconditions
    # This test assumes that all containers in the containerd have been created with this test -> clean it up first
    Given Containerd has deleted all existing containers
    And Ankaios server is started with config "${default_state_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A" within "5" seconds
    And the command "curl localhost:8081" finished with exit code "0" within "10" seconds
    # Actions
    When user triggers "ank -k get state > ${new_state_file}"
    And user updates the state "${new_state_file}" with "desiredState.workloads.nginx.runtimeConfig.commandOptions=['-p', '8082:80']"
    And user triggers "ank -k set state desiredState.workloads.nginx ${new_state_file}"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the command "curl localhost:8082" shall finish with exit code "0" within "10" seconds
    [Teardown]    Clean up Ankaios

