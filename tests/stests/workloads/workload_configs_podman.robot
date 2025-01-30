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
Documentation       Test of different cases related to workloads and their rendered configuration.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${start_up_yaml_file}           ${EMPTY}
${new_state_yaml_file}          ${EMPTY}


*** Test Cases ***
# [stest->swdd~server-state-compares-rendered-workloads~1]
Test Ankaios start up with templated Ankaios manifest and update state with updated config item
    [Setup]    Run Keywords    Setup Ankaios
    ...    AND    Set Global Variable    ${start_up_yaml_file}    ${CONFIGS_DIR}/manifest_with_configs.yaml
    ...    AND    Set Global Variable    ${new_state_yaml_file}   ${CONFIGS_DIR}/manifest_with_configs_updated_config_item.yaml

    # Preconditions
    # This test assumes that all Podman containers have been created with this test -> clean it up first
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

# [stest->swdd~common-config-item-key-naming-convention~1]
# [stest->swdd~server-naming-convention~1]
Test Ankaios update configs with invalid config item key
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all Podman containers have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    Then the configs field inside the state shall be empty
    When user triggers "ank -k set state desiredState.configs ${CONFIGS_DIR}/update_state_invalid_config_item_key.yaml"
    Then the configs field inside the state shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
# [stest->swdd~server-naming-convention~1]
Test Ankaios update workload with invalid config alias
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all Podman containers have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    Then the configs field inside the state shall be empty
    When user triggers "ank -k set state desiredState ${CONFIGS_DIR}/update_state_invalid_workload_config_alias.yaml"
    Then the configs field inside the state shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
# [stest->swdd~server-naming-convention~1]
Test Ankaios update workload with invalid config reference key
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all Podman containers have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    Then the configs field inside the state shall be empty
    When user triggers "ank -k set state desiredState ${CONFIGS_DIR}/update_state_invalid_workload_config_reference_key.yaml"
    Then the configs field inside the state shall be empty

    [Teardown]    Clean up Ankaios

# [stest->swdd~server-fails-on-invalid-startup-state~1]
Test Ankaios start up fails with invalid templated Ankaios manifest
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all Podman containers have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    # Manifest contains invalid template string syntax
    And Ankaios server is started with an invalid config "${CONFIGS_DIR}/invalid_templated_manifest.yaml"
    # Asserts
    Then the Ankaios server shall exit with an error code
    [Teardown]    Clean up Ankaios
