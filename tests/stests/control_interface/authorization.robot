*** Comments ***
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
Documentation       Tests the authorization of the Control Interface access from workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Variables ***
${simple_yaml_file}      ${EMPTY}
${manifest12_yaml_file}  ${EMPTY}
${manifest1_yaml_file}   ${EMPTY}
${manifest2_yaml_file}   ${EMPTY}
${manifest_no_agent_name_yaml_file}    ${EMPTY}

*** Test Cases ***

Allow set rule with empty string
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to write on ${EMPTY}
    Control Interface Command should update state with manifest "${CONFIGS_DIR}/empty_desired_state.yaml" and update mask: desiredState.workloads.workload1
    Control Interface Command should fail getting state
    Control Interface Command should fail getting state of fields: desiredState.workloads.workload1
    Execute Control Interface test

    [Teardown]    Clean up Ankaios
