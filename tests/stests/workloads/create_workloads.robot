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
Documentation       Tests to verify that Ankaios can create Podman workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***
# [stest->swdd~agent-supports-podman~2]
# [stest->swdd~podman-create-workload-runs-workload~1]
# [stest->swdd~podman-delete-workload-stops-and-removes-workload~1]
Test Ankaios Podman create and remove workloads
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with "ank-server --startup-config ${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with "ank-agent --name agent_B"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios agent is started with "ank-agent --name agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get workloads"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Removed" from agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded" on agent "agent_B"
    # Actions
    When user triggers "ank delete workload nginx"
    And user triggers "ank get workloads"
    Then the workload "nginx" shall not exist
    And the workload "hello1" shall have the execution state "Removed" from agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded" on agent "agent_B"
    When user executes system app "podman ps -a --format=json"
    ${dict_array}=    And the result is valid JSON
    Then the JSON array "${dict_array}" shall contain key "Labels" with subkey "agent" with the subvalue "agent_B"
    Then the JSON array "${dict_array}" shall contain key "Labels" with subkey "name" which matches the expression "^hello[2|3].\\w+.agent_B$"
    Then the JSON array "${dict_array}" shall contain array "Names" which matches the expression "^hello[2|3].\\w+.agent_B$"
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-supports-podman~2]
# [stest->swdd~podman-create-workload-runs-workload~1]
# [stest->swdd~podman-create-workload-sets-optionally-container-name~1]

Test Ankaios Podman create a container with custom name
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with "ank-server --startup-config ${CONFIGS_DIR}/create_workload_custom_name.yaml"
    And Ankaios agent is started with "ank-agent --name agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get workloads"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    When user executes system app "podman ps -a --format=json"
    ${dict_array}=    And the result is valid JSON
    Then the JSON array "${dict_array}" shall contain key "Labels" with subkey "agent" with the subvalue "agent_A"
    Then the JSON array "${dict_array}" shall contain key "Labels" with subkey "name" which matches the expression "^nginx.\\w+.agent_A$"
    Then the JSON array "${dict_array}" shall contain array "Names" which contains value "test_workload1"
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-create-workload-apply-manifest~1]
# [stest->swdd~podman-kube-create-workload-creates-config-volume~1]
# [stest->swdd~podman-kube-create-workload-creates-pods-volume~1]
# [stest->swdd~podman-kube-delete-workload-downs-manifest-file~1]
# [stest->swdd~podman-kube-delete-removes-volumes~1]

Test Ankaios Podman create kube workload
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all pods and volume in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing pods
    Given Podman has deleted all existing volumes
    And Ankaios server is started with "ank-server --startup-config ${CONFIGS_DIR}/kube.yaml"
    And Ankaios agent is started with "ank-agent --name agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    When user triggers "ank get workloads"
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    # Check config and pods volumes has been created
    When user executes system app "podman volume ls --format=json"
    ${dict_array}=    And the result is valid JSON
    Then the JSON array "${dict_array}" shall contain key "Name" which matches the expression "^nginx.\\w+.agent_A.(config|pods)$"
    # Check config and pods volumes are deleted when workload is deleted
    When user triggers "ank delete workload nginx"
    And user triggers "ank get workloads"
    Then the workload "nginx" shall not exist
    When user executes system app "podman volume ls --format=json"
    ${dict_array}=    And the result is valid JSON
    Then the JSON array "${dict_array}" shall contain key "Name" which not matches the expression "^nginx.\\w+.agent_A.(config|pods)$"
    [Teardown]    Clean up Ankaios
