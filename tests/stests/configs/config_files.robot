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
Documentation       Tests to verify that Ankaios creates and mounts config files
...                 assigned to workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Test Cases ***

Test Ankaios starts manifest with config files assigned to workloads
    [Documentation]    Create the assigned config files on the agent's host file system and mount it into workloads.
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    When Ankaios server is started with config "${CONFIGS_DIR}/manifest_config_files.yaml"
    And Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "workload_with_mounted_text_file" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "workload_with_mounted_binary_file" shall have the execution state "Succeeded(Ok)" on agent "agent_A"
    And the command "curl -Lf localhost:8087/custom" shall finish with exit code "0"
    [Teardown]    Clean up Ankaios


Test Ankaios rejects unsupported config files for workloads using podman-kube runtime
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all pods and volumes in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing pods
    And Podman has deleted all existing volumes
    And Ankaios server is started with config "${CONFIGS_DIR}/manifest_config_files.yaml"
    # Actions
    When Ankaios agent is started with name "agent_B"
    # Asserts
    Then the workload "kube_workload_with_unsupported_config_files" shall have the execution state "Pending(StartingFailed)" on agent "agent_B" within "20" seconds
    And the mount point for the config files of workload "kube_workload_with_unsupported_config_files" on agent "agent_B" has not been generated
    [Teardown]    Clean up Ankaios
