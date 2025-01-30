*** Comments ***
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
Documentation       Tests to verify that Ankaios can apply workload specifications via Ankaios manifest files.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Variables ***
${simple_yaml_file}      ${EMPTY}
${manifest12_yaml_file}  ${EMPTY}
${manifest1_yaml_file}   ${EMPTY}
${manifest2_yaml_file}   ${EMPTY}
${manifest_no_agent_name_yaml_file}    ${EMPTY}
${manifest_wrong_api_version}    ${EMPTY}
${manifest_wrong_api_version_format}    ${EMPTY}

*** Test Cases ***

Test Ankaios apply workload specifications showing progress via spinner
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest12_yaml_file}    ${CONFIGS_DIR}/manifest12.yaml

    # Preconditions
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply ${manifest12_yaml_file}"
    # Asserts
    Then the last command shall finish with exit code "0"
    And in the last result, the workload "nginx_from_manifest1" shall have the execution state "Pending(Initial)" on agent "agent_A"
    And in the last result, the workload "nginx_from_manifest1" shall have the execution state "Pending(Starting)" on agent "agent_A"
    And in the last result, the workload "nginx_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A"
    And in the last result, the workload "nginx_from_manifest2" shall have the execution state "Pending(Initial)" on agent "agent_A"
    And in the last result, the workload "nginx_from_manifest2" shall have the execution state "Pending(Starting)" on agent "agent_A"
    And in the last result, the workload "nginx_from_manifest2" shall have the execution state "Running(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
# [stest->swdd~cli-apply-generates-state-object-from-ankaios-manifests~1]
# [stest->swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1]
# [stest->swdd~cli-apply-send-update-state~1]
Test Ankaios apply workload specifications via Ankaios Manifest files
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest1_yaml_file}    ${CONFIGS_DIR}/manifest1.yaml
    ...        AND    Set Global Variable    ${manifest2_yaml_file}    ${CONFIGS_DIR}/manifest2.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank -v apply ${manifest1_yaml_file} ${manifest2_yaml_file}"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the workload "nginx_from_manifest2" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1]
Test Ankaios apply workload specifications via Ankaios Manifest content through stdin
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest1_yaml_file}    ${CONFIGS_DIR}/manifest1.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply -" passing "${manifest1_yaml_file}" through stdin
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
Test Ankaios apply workload specification overwriting the agent names
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest1_yaml_file}    ${CONFIGS_DIR}/manifest1.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply --agent agent_B ${manifest1_yaml_file}"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx_from_manifest1" shall have the execution state "Running(Ok)" on agent "agent_B" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1]
Test Ankaios apply workload specification defining the agent names
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest_no_agent}   ${CONFIGS_DIR}/manifest_no_agent_name.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And Ankaios agent is started with name "agent_B"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply --agent agent_B ${manifest_no_agent}"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx_from_manifest_no_agent_name" shall have the execution state "Running(Ok)" on agent "agent_B" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~1]
Test Ankaios apply workload specification without agent name
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest_no_agent_name_yaml_file}    ${CONFIGS_DIR}/manifest_no_agent_name.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply ${manifest_no_agent_name_yaml_file}"
    # Asserts
    Then the last command shall finish with an error
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-send-update-state~1]
Test Ankaios apply workload specifications via Ankaios Manifest files for deletion
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${manifest12_yaml_file}    ${CONFIGS_DIR}/manifest12.yaml
    ...        AND    Set Global Variable    ${manifest1_yaml_file}    ${CONFIGS_DIR}/manifest1.yaml
    ...        AND    Set Global Variable    ${manifest2_yaml_file}    ${CONFIGS_DIR}/manifest2.yaml

    # Preconditions
    Given Ankaios server is started with config "${manifest12_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply -d ${manifest1_yaml_file} ${manifest2_yaml_file}"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx_from_manifest1" shall not exist within "20" seconds
    And the workload "nginx_from_manifest2" shall not exist within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-send-update-state~1]
# [stest->swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1]
Test Ankaios apply workload specifications via Ankaios Manifest content through stdin for deletion
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest1_yaml_file}    ${CONFIGS_DIR}/manifest1.yaml

    # Preconditions
    Given Ankaios server is started with config "${manifest1_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply -d -" passing "${manifest1_yaml_file}" through stdin
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx_from_manifest1" shall not exist within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-send-update-state~1]
Test Ankaios apply workload specifications in Ankaios manifest with templated fields
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in Podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started without config
    And Ankaios agent is started with name "agent_A"
    # Actions
    When user triggers "ank apply ${CONFIGS_DIR}/manifest_with_configs.yaml"
    # Asserts
    Then the last command shall finish with exit code "0"
    And the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A" within "20" seconds
    And the workload "greeting_person" shall have the execution state "Succeeded(Ok)" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-manifest-check-for-api-version-compatibility~1]
Test Ankaios apply workload specification with wrong api version
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest_wrong_api_version}    ${CONFIGS_DIR}/manifest_wrong_api_version.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply ${manifest_wrong_api_version}"
    # Asserts
    Then the last command shall finish with an error
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-apply-manifest-check-for-api-version-compatibility~1]
Test Ankaios apply workload specification with wrong api version format
    [Setup]           Run Keywords    Setup Ankaios
    ...        AND    Set Global Variable    ${simple_yaml_file}    ${CONFIGS_DIR}/simple.yaml
    ...        AND    Set Global Variable    ${manifest_wrong_api_version_format}    ${CONFIGS_DIR}/manifest_wrong_api_version_format.yaml

    # Preconditions
    Given Ankaios server is started with config "${simple_yaml_file}"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank apply ${manifest_wrong_api_version_format}"
    # Asserts
    Then the last command shall finish with an error
    [Teardown]    Clean up Ankaios
