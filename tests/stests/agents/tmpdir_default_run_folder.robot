# Copyright (c) 2026 Elektrobit Automotive GmbH
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
Documentation    Verifies that ank-agent respects TMPDIR when run_folder is not set.
Resource         ../../resources/ankaios.resource

*** Test Cases ***
# [stest->swdd~agent-prepares-dedicated-run-folder~2]
Test agent uses TMPDIR for default run folder
    [Setup]    Setup Ankaios

    # For simplicity, we currently assume TMPDIR is not set by the environment.
    # If we start supporting systems where TMPDIR is set by default, this test
    # can be adapted to save and restore the original value instead.

    # Preconditions
    Environment Variable Should Not Be Set    TMPDIR
    Given Ankaios server is started with manifest "${CONFIGS_DIR}/simple_kube_with_control.yaml"
    And the CLI listens for workload states

    ${run_result}=    Run Process
    ...    command=echo "/tmp/$(tr -dc 'a-z0-9' </dev/urandom | head -c 10)"
    ...    shell=True
    ${tmpdir}=    Strip String    ${run_result.stdout}

    Directory Should Not Exist    ${tmpdir}

    # ank-agent expects TMPDIR to exist when used as the base for the default run folder.
    Run Process    command=mkdir -p "${tmpdir}"    shell=True

    Set Environment Variable    name=TMPDIR    value=${tmpdir}
    ${expected_tmp_ankaios_dir}=    Catenate    SEPARATOR=${/}    ${tmpdir}    ankaios

    # Actions
    And Ankaios agent is started with name "agent_A"
    And the workload "simple-kube" shall have the execution state "Running(Ok)" on agent "agent_A" within "60" seconds
    # Asserts
    Then Directory Should Exist    ${expected_tmp_ankaios_dir}

    [Teardown]    Run Keywords
    ...    Clean up Ankaios
    ...    AND    Run Process    command=rm -rf "${tmpdir}"    shell=True
    ...    AND    Remove Environment Variable    TMPDIR
