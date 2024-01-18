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
Documentation       Tests to verify that Ankaios rejects initial startup state with a cycle inside the interworkload dependency config

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***
Test Ankaios reject cyclic interworkload dependencies config
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all containers in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing containers
    # Actions
    # The startup config is invalid because it contains a cycle inside the interworkload dependencies config has a cycle
    When Ankaios server is started with an invalid config "${CONFIGS_DIR}/state_with_dependency_cycle.yaml"
    # Asserts
    Then the Ankaios server shall exit with an error code
    [Teardown]    Clean up Ankaios
