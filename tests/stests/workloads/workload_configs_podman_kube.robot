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
Documentation       Test of different cases related to workloads and their rendered configuration.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Variables ***
${start_up_yaml_file}           ${EMPTY}


*** Test Cases ***
# [stest->swdd~config-renderer-supports-rendering-with-keeping-line-indent~1]
Test Ankaios start up with templated Ankaios manifest keeping indentation level of rendered multi-line config
    [Setup]    Run Keywords    Setup Ankaios
    [Tags]    multi_line_config

    # Preconditions
    # This test assumes that all pods and volumes in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing pods
    And Podman has deleted all existing volumes
    And Ankaios server is started with config "${CONFIGS_DIR}/manifest_with_multi_line_config.yaml"
    # Actions
    When Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "nginx_with_custom_config" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the command "curl -fL localhost:8086/custom" shall finish with exit code "0" within "2" seconds
    [Teardown]    Clean up Ankaios
