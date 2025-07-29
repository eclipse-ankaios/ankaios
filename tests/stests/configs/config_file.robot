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
Documentation    Tests to verify that the configs file are read and respect the priority architecture: cli arguments, env variables and then the config files
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***

# [stest->swdd~server-loads-config-file~1]
Test server config file successful start-up
    [Setup]        Setup Ankaios
    # Preconditions
    Ankaios insecure server is started with server config file "${CONFIGS_DIR}/ank-server-default.conf"
    And Ankaios server is available
    # Actions
    # Asserts
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-loads-config-file~1]
Test agent config file successful start-up
    [Setup]        Setup Ankaios
    # Preconditions
    Ankaios server is started without config successfully
    And Ankaios server is available
    And Ankaios agent is started with config file "${CONFIGS_DIR}/ank-agent-default.conf"
    # Actions
    When user triggers "ank -k get agents"
    ${result_config}=  Run Keyword And Return Status    the last command shall list the connected agent "agent_1"
    # Asserts
    Pass Execution If    ${result_config} == False    The agent name "Invalid@gent.name" is not allowed
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-loads-config-file~1]
Test server config overwrite manifest with cli arguments
    [Setup]        Setup Ankaios
    [Tags]    run_only
    # Preconditions
    Ankaios insecure server is started with config "${CONFIGS_DIR}/default.yaml" and server config file "${CONFIGS_DIR}/ank-server.conf"
    And Ankaios server is available
    And Ankaios insecure agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Asserts
    Then the workload "simple" shall not exist on agent "agent_A" within "1" seconds
    [Teardown]    Clean up Ankaios
