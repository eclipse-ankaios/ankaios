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
Documentation    Tests to verify that ank cli lists workloads correctly.
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~server-supports-pem-file-paths-as-cli-arguments~1]
# [stest->swdd~agent-supports-pem-file-paths-as-cli-arguments~1]
# [stest->swdd~cli-supports-pem-file-paths-as-cli-arguments~1]
Test Ankaios MTLS by providing PEM files via environment variables
    [Setup]    Run Keyword    Setup Ankaios    mtls_enabled=True
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with name "agent_B"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get workloads"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Failed(Lost)" from agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-supports-pem-file-paths-as-cli-arguments~]
# [stest->swdd~agent-supports-pem-file-paths-as-cli-arguments~1]
# [stest->swdd~cli-supports-pem-file-paths-as-cli-arguments~1]
Test Ankaios MTLS by providing PEM files via command line arguments
    [Setup]    Run Keyword    Setup Ankaios without MTLS Setup
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default.yaml" and PEM files: "${CERTS_DIR}/ca.pem" "${CERTS_DIR}/server.pem" "${CERTS_DIR}/server-key.pem"
    And Ankaios agent is started with name "agent_B" and PEM files: "${CERTS_DIR}/ca.pem" "${CERTS_DIR}/agent.pem" "${CERTS_DIR}/agent-key.pem"
    And Ankaios agent is started with name "agent_A" and PEM files: "${CERTS_DIR}/ca.pem" "${CERTS_DIR}/agent.pem" "${CERTS_DIR}/agent-key.pem"
    # Actions
    When user triggers "ank --ca_pem ${CERTS_DIR}/ca.pem --crt_pem ${CERTS_DIR}/cli.pem --key_pem ${CERTS_DIR}/cli-key.pem get workloads"
    # Asserts
    Then the last command finished with exit code "0"
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-supports-pem-file-paths-as-cli-arguments~]
# [stest->swdd~agent-supports-pem-file-paths-as-cli-arguments~1]
# [stest->swdd~cli-supports-pem-file-paths-as-cli-arguments~1]
Test Ankaios MTLS by providing wrong PEM config via command line arguments
    [Setup]    Run Keyword    Setup Ankaios without MTLS Setup
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default.yaml" and PEM files: "${CERTS_DIR}/ca.pem" "${CERTS_DIR}/server.pem" "${CERTS_DIR}/server-key.pem"
    And Ankaios agent is started with name "agent_B" and PEM files: "${CERTS_DIR}/ca.pem" "${CERTS_DIR}/agent.pem" "${CERTS_DIR}/agent-key.pem"
    And Ankaios agent is started with name "agent_A" and PEM files: "${CERTS_DIR}/ca.pem" "${CERTS_DIR}/agent.pem" "${CERTS_DIR}/agent-key.pem"
    # Actions
    # note that "--ca_pem ${CERTS_DIR}/ca.pem" is missing
    When user triggers "ank --crt_pem ${CERTS_DIR}/cli.pem --key_pem ${CERTS_DIR}/cli-key.pem get workloads"
    # Asserts
    Then the last command finished with exit code "1"
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-supports-cli-argument-for-insecure-communication~1]
# [stest->swdd~agent-supports-cli-argument-for-insecure-communication~1]
# [stest->swdd~cli-supports-cli-argument-for-insecure-communication~1]
# [stest->swdd~cli-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
Test Ankaios insecure mode by providing --insecure command line arguments
    [Setup]    Run Keyword    Setup Ankaios without MTLS Setup
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios insecure server is started with config "${CONFIGS_DIR}/default.yaml"
    And Ankaios insecure agent is started with name "agent_B"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios insecure agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank --insecure get workloads"
    # # Asserts
    Then the workload "nginx" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Failed(Lost)" from agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded(Ok)" on agent "agent_B"
    [Teardown]    Clean up Ankaios
