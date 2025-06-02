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
# [stest->swdd~agent-skips-unknown-runtime~2]
Test Ankaios shall not start a workload with an invalid runtime
    [Setup]    Run Keywords    Setup Ankaios
    # Preconditions
    # This test assumes that all pods and volumes in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing pods
    And Podman has deleted all existing volumes
    And Ankaios server is started with config "${CONFIGS_DIR}/simple_with_invalid_runtime.yaml"
    # Actions
    When Ankaios agent is started with name "agent_A"
    # Asserts
    Then the workload "simple" shall have the execution state "Pending(StartingFailed)" on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios
