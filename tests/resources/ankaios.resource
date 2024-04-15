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
Documentation       This is a resource file, that can contain variables and keywords.
...                 Keywords defined here can be used where this Keywords.resource in loaded.

Library             Process
Library             Collections
Library             OperatingSystem
Library             String
Library             ankaios_library.py


*** Variables ***
${CURRENT_RESULT}       ${EMPTY}
${SERVER_PROCESS_HANDLE}       ${EMPTY}
${TEST_FOLDER_NAME}    ${EMPTY}


*** Keywords ***
Setup Ankaios
    ${run_result}=    Run Process    command=mktemp -d    shell=True
    Set Environment Variable    name=ANKAIOS_TEMP    value=${run_result.stdout}
    ${TEST_FOLDER_NAME}=    Evaluate    '${TEST_NAME}'.lower().replace(' ', '_')
    Set Global Variable    ${TEST_FOLDER_NAME}
    ${run_result}=    Run Process    command=mkdir -p "target/robot_tests_result/${TEST_FOLDER_NAME}"    shell=True

Clean up Ankaios
    Terminate All Processes    kill=True
    Clean up Podman
    Run Command    rm -rf %{ANKAIOS_TEMP}    timeout=20

Clean up Podman
    Log    Clean up Podman
    Podman has deleted all existing containers
    Podman has deleted all existing pods
    Podman has deleted all existing volumes

Run in background
    [Arguments]    ${location}    ${cli_command}
    Log    Run in background: ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    Set List Value    ${cmd}    0    ${location}${/}${cmd}[0]
    Start Process    @{cmd}

Run in foreground
    [Arguments]    ${cli_command}
    Log    Run in foreground: ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    ${run_result}=    Run Process    @{cmd}    shell=True
    RETURN    ${run_result}

the last command finished with exit code "${exit_code}"
    ${exit_code_int}=    Convert To Integer    ${exit_code}
    Should Be Equal    ${CURRENT_RESULT.rc}    ${exit_code_int}

the last command finished with an error
    Should Not Be Equal    ${CURRENT_RESULT.rc}    ${0}

the command "${cli_command}" finished with exit code "${exit_code}" within "${timeout}" seconds
    ${start_time}=    Get Time Secs
    ${run_result}=    Create Dictionary
    WHILE    True
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${run_result}=    Run in foreground    ${cli_command}
        Log    ${run_result.stdout}
        ${exit_code_int}=    Convert To Integer    ${exit_code}
        IF    ${run_result.rc}==${exit_code_int}
            Should Be Equal    ${run_result.rc}    ${exit_code_int}
            BREAK
        END
        IF    ${elapsed_secs} >= ${timeout}
            Should Be Equal
            ...    ${run_result.rc}
            ...    ${exit_code_int}
            ...    msg=Timeout: Execution result is still '${run_result.rc}!=${exit_code_int}' after ${timeout} seconds!
            BREAK
        END
    END

Ankaios server is started with config "${config_path}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --startup-config
    ...    ${config_path}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    RETURN    ${server_process}

Ankaios server is started with an invalid config "${config_path}"
    ${SERVER_PROCESS_HANDLE}=        Ankaios server is started with config "${config_path}"
    Set Global Variable    ${SERVER_PROCESS_HANDLE}

the Ankaios server shall exit with an error code
    ${process_result}=    Wait For Process    ${SERVER_PROCESS_HANDLE}    timeout=3 s    on_timeout=continue
    Process Should Be Stopped    ${SERVER_PROCESS_HANDLE}
    Should Not Be Equal As Integers   ${process_result.rc}    0

Ankaios server is started without config
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug

Ankaios agent is started with name "${name}"
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-agent
    ...    --name
    ...    ${name}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${name}_logs.txt
    ...    env:RUST_LOG=debug

Ankaios is terminated
    Terminate All Processes    kill=True

Podman has deleted all existing containers
    Run Process    podman    rm    --all    --force    --time    2    timeout=20    shell=True

Podman has deleted all existing pods
    Run Process    podman    pod    rm    --all    --force    --time    2    timeout=20    shell=True

Podman has deleted all existing volumes
    Run Process    podman    volume    rm    --all    --force    timeout=20    shell=True

all workloads of agent "${agent_name}" have an initial execution state
    ${list_result}=    Wait For Initial Execution State
    ...    %{ANK_BIN_DIR}${/}ank get workloads
    ...    ${agent_name}
    ...    60
    Should Not Be Empty
    ...    ${list_result}
    ...    msg=Timeout: Workloads on agent '${agent_name}' do not have the expected initial execution states!

user triggers "${cli_command}"
    ${cmd}=    Split Command Line    ${cli_command}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]
    ${CURRENT_RESULT}=    Run Process    @{cmd}    timeout=20    shell=True    stdout=${TEMPDIR}/stdout.txt    stderr=${TEMPDIR}/stderr.txt
    Log    ${CURRENT_RESULT.stdout}
    Log    ${CURRENT_RESULT.stderr}
    Set Global Variable    ${CURRENT_RESULT}

user triggers "${cli_command}" passing "${ankaios_manifest_file}" through stdin
    ${cmd}=    Split Command Line    ${cli_command}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]
    ${CURRENT_RESULT}=    Run Process    @{cmd}    timeout=20    shell=True    stdout=${TEMPDIR}/stdout.txt    stderr=${TEMPDIR}/stderr.txt    stdin=${ankaios_manifest_file}
    Log    ${CURRENT_RESULT.stdout}
    Log    ${CURRENT_RESULT.stderr}
    Set Global Variable    ${CURRENT_RESULT}

the user waits "${seconds_to_wait}" seconds
    Run Process    sleep    ${seconds_to_wait}

user updates the state "${new_state_yaml_file}" with "${field_value}"
    @{key_val}=    Split String    ${field_value}    separator==
    ${yaml_data}=    Get File    ${new_state_yaml_file}
    ${result_dict}=    Yaml To Dict    ${yaml_data}
    ${new_config}=    Replace Config    ${result_dict}    filter_path=${key_val}[0]    new_value=${key_val}[1]
    Write Yaml    new_yaml=${new_config}    path=${new_state_yaml_file}

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}" within "${timeout}" seconds
    @{list_result}=    Create List
    IF    ${timeout} > 0
        @{list_result}=    Wait For Execution State
        ...    %{ANK_BIN_DIR}${/}ank get workloads
        ...    ${workload_name}
        ...    ${expected_execution_state}
        ...    ${timeout}
    END
    Should Not Be Empty
    ...    ${list_result}
    ...    msg=Timeout: Workload '${workload_name}' on agent '${expected_agent_name}' does not have the execution state '${expected_execution_state}'!

the workload "${workload_name}" shall not exist
    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank get workloads
    @{list_result}=    table to list    ${command_result.stdout}
    &{workload_table}=    table to dict    ${list_result}    WORKLOAD NAME
    Dictionary Should Not Contain Key    ${workload_table}    ${workload_name}

the workload "${workload_name}" shall not exist on agent "${expected_agent_name}" within "${timeout}" seconds
    Should Be True    ${timeout} >= 0    msg='timeout' must be greater than or equal to 0!
    @{list_result}=    Create List
    IF    ${timeout} >= 0
        @{list_result}=    Wait For Workload Removal
        ...    %{ANK_BIN_DIR}${/}ank get workloads
        ...    ${workload_name}
        ...    ${expected_agent_name}
        ...    ${timeout}
    END
    Should Be Empty
    ...    ${list_result}
    ...    msg=Timeout: Workload '${workload_name}' found on agent '${expected_agent_name}'!

the workload "${workload_name}" shall not exist within "${timeout}" seconds
    Should Be True    ${timeout} >= 0    msg='timeout' must be greater than or equal to 0!
    @{list_result}=    Create List
    IF    ${timeout} >= 0
        @{list_result}=    Wait For Workload Removal
        ...    %{ANK_BIN_DIR}${/}ank get workloads
        ...    ${workload_name}
        ...    ""
        ...    ${timeout}
    END
    Should Be Empty
    ...    ${list_result}
    ...    msg=Timeout: Workload '${workload_name}' found!

in the last result, the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}"
    @{list_result}=    table to list    ${CURRENT_RESULT.stdout}
    @{list_result}=    workload with execution_state    ${list_result}    ${workload_name}    ${expected_execution_state}
    Log List    ${list_result}
    Should Not Be Empty
    ...    ${list_result}
    ...    msg=Timeout: Workload '${workload_name}' on agent '${expected_agent_name}' does not have the execution state '${expected_execution_state}'!

user executes system app "${cli_command}"
    ${cmd}=    Split Command Line    ${cli_command}    escaping=True
    ${CURRENT_RESULT}=    Run Process    @{cmd}    timeout=20    shell=True    stdout=${TEMPDIR}/stdout.txt    stderr=${TEMPDIR}/stderr.txt
    Log    ${CURRENT_RESULT.stdout}
    Set Global Variable    ${CURRENT_RESULT}

volumes for "${workload_name}" shall exist on "${agent_name}" within "${timeout}" seconds
    ${start_time}=    Get Time Secs
    ${volume_names}=    Create List
    ${volume_agent_config}=    To X Dot Y Dot Z Format    ${workload_name}    ${agent_name}    config
    ${volume_agent_pods}=    To X Dot Y Dot Z Format    ${workload_name}    ${agent_name}    pods
    WHILE    True
        ${volume_names}=    Get Volume Names From Podman
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${found_config_index}=    Get Index From List    ${volume_names}    ${volume_agent_config}
        ${found_pods_index}=    Get Index From List    ${volume_names}    ${volume_agent_pods}
        IF    ${found_config_index}!=-1 and ${found_pods_index}!=-1    BREAK
        IF    ${elapsed_secs} >= ${timeout}    BREAK
    END
    Should Contain X Times
    ...    ${volume_names}
    ...    ${volume_agent_config}
    ...    1
    ...    msg=Volume '${workload_name}.{hash}.${agent_name}.config' not found in podman!
    Should Contain X Times
    ...    ${volume_names}
    ...    ${volume_agent_pods}
    ...    1
    ...    msg=Volume '${workload_name}.{hash}.${agent_name}.pods' not found in podman!

volumes for "${workload_name}" shall not exist on "${agent_name}" within "${timeout}" seconds
    ${start_time}=    Get Time Secs
    ${volume_names}=    Create List
    ${workload_dot_agent}=    To X Dot Y Format    ${workload_name}    ${agent_name}
    WHILE    True
        ${volume_names}=    Get Volume Names From Podman
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${found_index}=    Get Index From List    ${volume_names}    ${workload_dot_agent}
        IF    ${found_index} == -1    BREAK
        IF    ${elapsed_secs} >= ${timeout}    BREAK
    END
    Should Contain X Times
    ...    ${volume_names}
    ...    ${workload_dot_agent}
    ...    0
    ...    msg=Unexpected Volume '${workload_name}.{hash}.${agent_name}.{config|pods}' found in podman!

podman shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}" within "${timeout}" seconds
    ${start_time}=    Get Time Secs
    ${workload_names}=    Create List
    ${used_name}=    Set Variable    ${name}
    ${workload_name_angent_name}=    Set Variable    ${name}
    IF    "${name}" == ""
        ${used_name}=    Set Variable    ${workload_name}
        ${workload_name_angent_name}=    To X Dot Y Format    ${workload_name}    ${agent_name}
    END
    WHILE    True
        ${workload_names}=    Get Workload Names From Podman
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${found_index}=    Get Index From List    ${workload_names}    ${workload_name_angent_name}
        IF    ${found_index}!=-1    BREAK
        IF    ${elapsed_secs} >= ${timeout}    BREAK
    END
    Should Contain X Times
    ...    ${workload_names}
    ...    ${workload_name_angent_name}
    ...    1
    ...    msg=Workload '${used_name}' on agent '${agent_name}' not found in podman!

podman shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds
    ${start_time}=    Get Time Secs
    ${workload_names}=    Create List
    ${workload_name_angent_name}=    To X Dot Y Format    ${workload_name}    ${agent_name}
    WHILE    True
        ${workload_names}=    Get Workload Names From Podman
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${found_index}=    Get Index From List    ${workload_names}    ${workload_name_angent_name}
        IF    ${found_index}==-1    BREAK
        IF    ${elapsed_secs} >= ${timeout}    BREAK
    END
    Should Not Contain
    ...    ${workload_names}
    ...    ${workload_name_angent_name}
    ...    msg=Unexpected workload '${workload_name}' on agent '${agent_name}' found in podman!

list of workloads shall be empty
    @{list_result}=    Create List
    Should Be Empty    item=${list_result}

# ALIASES

the command "${cli_command}" finished with exit code "${exit_code}"
    the command "${cli_command}" finished with exit code "${exit_code}" within "1" seconds

the command "${cli_command}" shall finish with exit code "${exit_code}"
    the command "${cli_command}" finished with exit code "${exit_code}"

the command "${cli_command}" shall finish with exit code "${exit_code}" within "${timeout}" seconds
    the command "${cli_command}" finished with exit code "${exit_code}" within "${timeout}" seconds

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" from agent "${expected_agent_name}"
    the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}"

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" from agent "${expected_agent_name}" within "${timeout}" seconds
    the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}" within "${timeout}" seconds

the workload "${workload_name}" shall have the execution state "${execution_state}" on agent "${expected_agent_name}"
    the workload "${workload_name}" shall have the execution state "${execution_state}" on agent "${expected_agent_name}" within "5" seconds

volumes for "${workload_name}" shall exist on "${agent_name}"
    volumes for "${workload_name}" shall exist on "${agent_name}" within "1" seconds

podman shall have a container for workload "${workload_name}" on agent "${agent_name}"
    podman shall have a container for workload "${workload_name}" with custom name "" on agent "${agent_name}" within "1" seconds

podman shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}"
    podman shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}" within "1" seconds

podman shall not have a container for workload "${workload_name}" on agent "${agent_name}"
    podman shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "1" seconds

the last command shall finish with an error
    the last command finished with an error

the last command shall finish with exit code "${exit_code}"
    the last command finished with exit code "${exit_code}"
