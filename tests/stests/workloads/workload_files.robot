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
Documentation       Tests to verify that Ankaios creates and mounts workload files
...                 assigned to workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~podman-create-mounts-workload-files~1]
# [stest->swdd~containerd-create-mounts-workload-files~1]
Test Ankaios starts manifest with workload files assigned to workloads
    [Documentation]    Create the assigned workload files on the agent's host file system and mount it into workloads.
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Containerd has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/manifest_with_workload_files.yaml"
    And Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "podman_workload_with_mounted_text_file" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "podman_workload_with_mounted_binary_file" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    And the workload "containerd_workload_with_mounted_text_file" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "containerd_workload_with_mounted_binary_file" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    And the command "curl -Lf localhost:8087/custom" shall finish with exit code "0"
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-create-mounts-workload-files~1]
Test Ankaios updates a workload upon update of its workload file content
    [Documentation]    Re-create the new workload file on the host file system and
    ...                mount it in the new updated version of the workload.
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/manifest_with_workload_files.yaml"
    And Ankaios agent is started with name "agent_A"
    And the workload "workload_with_mounted_text_file" shall have the execution state "Running(Ok)" on agent "agent_A"
    And user triggers "ank -k --no-wait set state desiredState.workloads.workload_with_mounted_text_file desiredState.configs.web_server_config ${CONFIGS_DIR}/update_state_workload_files.yaml"
    # Asserts
    Then the workload "workload_with_mounted_text_file" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the command "curl -Lf localhost:8087/update" shall finish with exit code "0"
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-create-mounts-workload-files~1]
Test Ankaios updates a workload upon adding additional workload files
    [Documentation]    Re-create all the workload files including the new one on the host file system,
    ...                mount it in the new updated version of the workload and execute it.
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/manifest_with_workload_files.yaml"
    And Ankaios agent is started with name "agent_A"
    And the workload "workload_with_mounted_binary_file" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    # First update the files only by setting the update mask
    And user triggers "ank -k --no-wait set state desiredState.workloads.workload_with_mounted_binary_file.files ${CONFIGS_DIR}/update_state_workload_files.yaml"
    And the workload "workload_with_mounted_binary_file" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    # Now update the runtimeConfig calling the newly added workload file
    And user triggers "ank -k --no-wait set state desiredState.workloads.workload_with_mounted_binary_file.runtimeConfig ${CONFIGS_DIR}/update_state_workload_files.yaml"
    # Asserts
    Then the workload "workload_with_mounted_binary_file" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-rejects-workload-files~1]
Test Ankaios rejects unsupported workload files for workloads using podman-kube runtime
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all pods and volumes in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing pods
    And Podman has deleted all existing volumes
    And Ankaios server is started with config "${CONFIGS_DIR}/manifest_with_workload_files.yaml"
    # Actions
    When Ankaios agent is started with name "agent_B"
    # Asserts
    Then the workload "kube_workload_with_unsupported_files" shall have the execution state "Pending(StartingFailed)" on agent "agent_B" within "20" seconds
    And the mount point for the workload files of workload "kube_workload_with_unsupported_files" on agent "agent_B" has not been generated
    [Teardown]    Clean up Ankaios
