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

*** Test Cases ***

Test Ankaios workload restart after update with a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    # It is supposed that ankaios server and agent are already up and running
    # Actions
    When user triggers "ank -v apply ${manifest_yaml_file}"
    # Asserts
    Then the workload "${workload_name}" shall not exist
    And Ankaios agent is started with name "${name}"
    And all workloads of agent "{agent_name}" have an initial execution state
    And he input and output files have been generated while using the control interface in "${directory}"
    [Teardown]    Clean up Ankaios

Test Ankaios workload successful start-up with a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${config_path}"
    And Ankaios agent is started with name "${name}"
    And all workloads of agent "{agent_name}" have an initial execution state
    # Actions
    # Asserts
    Then the workload "${workload_name}" shall have the execution state "Running(Ok)" on agent "${agent_name}"
    And he input and output files have been generated while using the control interface in "${directory}"
    [Teardown]    Clean up Ankaios
