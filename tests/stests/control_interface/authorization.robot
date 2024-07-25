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

No rules
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload

    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should fail getting state
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing.tags
    Control Interface Command should fail getting state of fields: desiredState.workloads.controller

    Execute Control Interface test

    [Teardown]    Clean up Ankaios

Allow write rule with empty string
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to write on ${EMPTY}

    Control Interface Command should update state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should update state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should fail getting state
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing.tags

    Execute Control Interface test

    [Teardown]    Clean up Ankaios

Allow read rule with empty string
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to read on ${EMPTY}

    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should get state
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing.tags

    Execute Control Interface test

    [Teardown]    Clean up Ankaios

Allow read write rule with empty string
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to read and write on ${EMPTY}

    Control Interface Command should update state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should update state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should get state
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing.tags

    Execute Control Interface test

    [Teardown]    Clean up Ankaios

Allow write rule for only tags
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to write on desiredState.workloads.*.tags

    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should update state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should fail getting state
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing.tags

    Execute Control Interface test

    [Teardown]    Clean up Ankaios

Allow read rule for only tags
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to read on desiredState.workloads.*.tags

    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should fail getting state
    Control Interface Command should fail getting state of fields: desiredState.workloads.simple_existing
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing.tags

    Execute Control Interface test

    [Teardown]    Clean up Ankaios

Allow read write rule for workloads except write to simple_existing
    [Setup]    Setup Ankaios

    Prepare Test Control Interface Workload
    Control Interface allows to read and write on desiredState.workloads
    Control Interface denies to write on desiredState.workloads.simple_existing

    Control Interface Command should update state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple
    Control Interface Command should fail updating state with manifest "${CONFIGS_DIR}/simple_state.yaml" and update mask: desiredState.workloads.simple_existing.tags
    Control Interface Command should fail getting state
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing
    Control Interface Command should get state of fields: desiredState.workloads.simple_existing.tags

    Execute Control Interface test

    [Teardown]    Clean up Ankaios