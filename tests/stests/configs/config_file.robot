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
Documentation    Tests to verify that ank cli lists configs correctly.
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~server-loads-config-file~1]
Test server config file successful start-up
    [Setup]        Setup Ankaios
    # Preconditions
    Ankaios server is started with config "${CONFIGS_DIR}/default.yaml" and server config file "${CONFIGS_DIR}/ank-server.conf"
    # Actions
    # Asserts
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-loads-config-file~1]
Test overwrite config files by cli arguments
    [Setup]        Setup Ankaios
    # Preconditions
    Ankaios insecure server is started with config "${CONFIGS_DIR}/manifest2.yaml" and server config file "${CONFIGS_DIR}/ank-server.conf"
    And Ankaios insecure agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank -k get workloads"
    # Asserts
    Then the workload "nginx_from_manifest2" shall have the execution state "Running(Ok)" on agent "agent_A"
    And the workload "nginx_from_manifest1" shall not exist on agent "agent_A" within "5" seconds
    [Teardown]    Clean up Ankaios
