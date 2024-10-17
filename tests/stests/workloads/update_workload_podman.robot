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
${default_state_yaml_file}      ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios CLI update workload
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/default.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_update_workload_new_state.yaml
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${default_state_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    And the command "curl localhost:8081" finished with exit code "0"
    # Actions
    When user triggers "ank -k get state > ${new_state_yaml_file}"
    And user updates the state "${new_state_yaml_file}" with "desiredState.workloads.nginx.runtimeConfig.commandOptions=['-p', '8082:80']"
    And user triggers "ank -k set state desiredState.workloads.nginx ${new_state_yaml_file}"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the command "curl localhost:8082" shall finish with exit code "0" within "10" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-starts-without-startup-config~1]
Test Ankaios Podman update workload from empty state
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank -k get workloads"
    Then list of workloads shall be empty
    When user triggers "ank -k set state desiredState.workloads ${CONFIGS_DIR}/update_state_create_one_workload.yaml"
    Then the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~update-desired-state-with-invalid-version~1]
Test Ankaios Podman Update workload with invalid api version
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank -k get workloads"
    Then list of workloads shall be empty
    When user triggers "ank -k set state desiredState ${CONFIGS_DIR}/update_state_invalid_version.yaml"
    And user triggers "ank -k get workloads"
    Then list of workloads shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~common-workload-naming-convention~1]
# [stest->swdd~server-naming-convention~1]
Test Ankaios Podman Update workload with invalid workload name
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank -k get workloads"
    Then list of workloads shall be empty
    When user triggers "ank -k set state desiredState ${CONFIGS_DIR}/update_state_invalid_names.yaml"
    And user triggers "ank -k get workloads"
    Then list of workloads shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~common-workload-naming-convention~1]
# [stest->swdd~server-naming-convention~1]
Test Ankaios Podman Update workload with lengthy workload name
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank -k get workloads"
    Then list of workloads shall be empty
    When user triggers "ank -k set state desiredState ${CONFIGS_DIR}/update_state_long_names.yaml"
    And user triggers "ank -k get workloads"
    Then list of workloads shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~common-agent-naming-convention~1]
# [stest->swdd~server-naming-convention~1]
# [stest->swdd~agent-naming-convention~1]
Test Ankaios Podman Update workload with invalid agent name
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank -k get workloads"
    Then list of workloads shall be empty
    When user triggers "ank -k set state desiredState.workloads.nginx ${CONFIGS_DIR}/update_state_invalid_names.yaml"
    And user triggers "ank -k get workloads"
    Then list of workloads shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~update-desired-state-with-missing-version~1]
Test Ankaios Podman Update workload with missing api version
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank -k get workloads"
    Then list of workloads shall be empty
    When user triggers "ank -k set state ${CONFIGS_DIR}/update_state_missing_version.yaml desiredState"
    And user triggers "ank -k get workloads"
    Then list of workloads shall be empty

    [Teardown]    Clean up Ankaios
