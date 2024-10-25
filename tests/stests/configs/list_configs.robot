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

# [stest->swdd~cli-provides-list-of-configs~1]
Test Ankaios CLI get configs
    [Setup]        Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default_with_config.yaml"
    # Actions
    When user triggers "ank -k get configs"
    # Asserts
    Then the last command shall list the config "config_1"
    And the last command shall list the config "config_2"
    [Teardown]    Clean up Ankaios

# [stest->swdd~cli-provides-delete-configs~1]
Test Ankaios Podman remove confgis
    [Setup]        Setup Ankaios
    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    And Ankaios server is started with config "${CONFIGS_DIR}/default_with_config.yaml"
    # Actions
    When user triggers "ank -k delete configs config_1"
    And user triggers "ank -k get configs"
    # Asserts
    Then the last command shall not list the config "config_1"
    And the last command shall list the config "config_2"
    [Teardown]    Clean up Ankaios

