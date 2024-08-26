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
Documentation       Tests to verify that Ankaios workloads are being restarted after having added a Control Interface.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Variables ***
${simple_yaml_file}      ${EMPTY}
${config_path}           ${EMPTY}
${agent_name}            "agent_A"
${workload_name}         "nginx"
${directory}             ${EMPTY}
${manifest_yaml_file}    ${CONFIGS_DIR}/default.yaml

*** Test Cases ***

Test Ankaios workload restart after update without a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/startConfig.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "{agent_name}" have an initial execution state
    # Actions
    When user triggers "ank -k apply ${manifest_yaml_file}"
    # Asserts
    Then the input and output files have not been generated for "${agent_name}"
    [Teardown]    Clean up Ankaios

Test Ankaios workload successful start-up without a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "{agent_name}" have an initial execution state
    # Actions
    # Asserts
    Then the input and output files have not been generated for "${agent_name}"
    [Teardown]    Clean up Ankaios
