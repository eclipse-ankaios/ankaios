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

Resource            ./variables.resource


*** Variables ***
${CURRENT_RESULT}       ${EMPTY}
${SERVER_PROCESS_HANDLE}       ${EMPTY}
${TEST_FOLDER_NAME}    ${EMPTY}
&{ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}
&{ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING}
${ANKAIOS_VERSION}    ${EMPTY}
${CONFIGS_DIR}=    ${CURDIR}/configs


*** Keywords ***
Get MTLS Enabled
    ${result}=    Get Environment Variable    name=MTLS_ENABLED    default=False
    RETURN    ${result}

Retrieve current Ankaios version
    ${run_result}=    Run Process    command=%{ANK_BIN_DIR}${/}ank-server --version    shell=True
    ${ANKAIOS_VERSION}=    Evaluate    '${run_result.stdout}'.split(' ')[1]
    Set Global Variable    ${ANKAIOS_VERSION}

Setup Ankaios without MTLS Setup
    ${run_result}=    Run Process    command=mktemp -d    shell=True
    Set Environment Variable    name=ANKAIOS_TEMP    value=${run_result.stdout}
    ${TEST_FOLDER_NAME}=    Evaluate    '${TEST_NAME}'.lower().replace(' ', '_')
    Set Global Variable    ${TEST_FOLDER_NAME}
    ${run_result}=    Run Process    command=mkdir -p "target/robot_tests_result/${TEST_FOLDER_NAME}"    shell=True
    Set Environment Variable    name=ANKSERVER_INSECURE    value=true
    Set Environment Variable    name=ANKAGENT_INSECURE    value=true
    Set Environment Variable    name=ANK_INSECURE    value=true

Setup Ankaios
    [Arguments]    ${mtls_enabled}=False
    Setup MTLS    enabled=${mtls_enabled}
    Setup Ankaios without MTLS Setup

Clean up Ankaios
    Terminate All Processes    kill=True
    Clean up Podman
    Clean up containerd
    ${ANKAIOS_TEMP}=    Get Environment Variable    ANKAIOS_TEMP
    Run Command    rm -rf ${ANKAIOS_TEMP}    timeout=20
    Run Command    rm -rf ${ANKAIOS_TMP_FOLDER}
    Set Variable    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}    {}
    Set Variable    ${ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING}    {}
    Remove Environment Variable    ANKSERVER_CA_PEM
    Remove Environment Variable    ANKSERVER_CRT_PEM
    Remove Environment Variable    ANKSERVER_KEY_PEM
    Remove Environment Variable    ANKAGENT_CA_PEM
    Remove Environment Variable    ANKAGENT_CRT_PEM
    Remove Environment Variable    ANKAGENT_KEY_PEM
    Remove Environment Variable    ANK_CA_PEM
    Remove Environment Variable    ANK_CRT_PEM
    Remove Environment Variable    ANK_KEY_PEM
    Remove Environment Variable    ANKSERVER_INSECURE
    Remove Environment Variable    ANKAGENT_INSECURE
    Remove Environment Variable    ANK_INSECURE

Clean up Podman
    Log    Clean up Podman
    Podman has deleted all existing containers
    Podman has deleted all existing pods
    Podman has deleted all existing volumes

Clean up containerd
    Log    Clean up containerd
    containerd has deleted all existing containers

Setup MTLS
    [Arguments]    ${enabled}
    IF  ${enabled} == True
        ${ca_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /ca.pem
        ${server_crt_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /server.pem
        ${server_key_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /server-key.pem
        ${agent_crt_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /agent.pem
        ${agent_key_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /agent-key.pem
        ${cli_crt_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /cli.pem
        ${cli_key_pem}=    Catenate    SEPARATOR=    ${CERTS_DIR}    /cli-key.pem
        Set Environment Variable    name=ANKSERVER_CA_PEM    value=${ca_pem}
        Set Environment Variable    name=ANKSERVER_CRT_PEM    value=${server_crt_pem}
        Set Environment Variable    name=ANKSERVER_KEY_PEM    value=${server_key_pem}
        Set Environment Variable    name=ANKAGENT_CA_PEM    value=${ca_pem}
        Set Environment Variable    name=ANKAGENT_CRT_PEM    value=${agent_crt_pem}
        Set Environment Variable    name=ANKAGENT_KEY_PEM    value=${agent_key_pem}
        Set Environment Variable    name=ANK_CA_PEM    value=${ca_pem}
        Set Environment Variable    name=ANK_CRT_PEM    value=${cli_crt_pem}
        Set Environment Variable    name=ANK_KEY_PEM    value=${cli_key_pem}
    ELSE
        Set Environment Variable    name=ANKSERVER_INSECURE    value=true
        Set Environment Variable    name=ANKAGENT_INSECURE    value=true
        Set Environment Variable    name=ANK_INSECURE    value=true
    END


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

Ankaios server is started with config "${config_path}" and PEM files: "${ca_pem}" "${crt_pem}" "${key_pem}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --startup-manifest
    ...    ${config_path}
    ...    --ca_pem
    ...    ${ca_pem}
    ...    --crt_pem
    ...    ${crt_pem}
    ...    --key_pem
    ...    ${key_pem}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=ank-server
    RETURN    ${server_process}

Ankaios insecure server is started with config "${config_path}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --startup-manifest
    ...    ${config_path}
    ...    --insecure
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=ank-server
    RETURN    ${server_process}

Ankaios insecure server is started with server config file "${server_config_path}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --server-config
    ...    ${server_config_path}
    ...    --insecure
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=ank-server
    RETURN    ${server_process}

Ankaios insecure server is started with config "${config_path}" and server config file "${server_config_path}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --startup-manifest
    ...    ${config_path}
    ...    --server-config
    ...    ${server_config_path}
    ...    --insecure
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=ank-server
    RETURN    ${server_process}

Ankaios server is started with config "${config_path}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --startup-manifest
    ...    ${config_path}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=ank-server
    RETURN    ${server_process}

Ankaios server is started with config "${config_path}" and server config file "${server_config_path}"
    ${server_process}=    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    --startup-manifest
    ...    ${config_path}
    ...    --server-config
    ...    ${server_config_path}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=ank-server
    RETURN    ${server_process}

Ankaios server is started with an invalid config "${config_path}"
    ${SERVER_PROCESS_HANDLE}=        Ankaios server is started with config "${config_path}"
    Set Global Variable    ${SERVER_PROCESS_HANDLE}

the Ankaios server shall exit with an error code
    ${process_result}=    Wait For Process    ${SERVER_PROCESS_HANDLE}    timeout=3 s    on_timeout=continue
    Process Should Be Stopped    ${SERVER_PROCESS_HANDLE}
    Should Not Be Equal As Integers   ${process_result.rc}    0

Ankaios server is started without config successfully
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-server
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt
    ...    env:RUST_LOG=debug
    Ankaios server is available

Ankaios agent is started with name "${name}" and PEM files: "${ca_pem}" "${crt_pem}" "${key_pem}"
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-agent
    ...    --name
    ...    ${name}
    ...    --ca_pem
    ...    ${ca_pem}
    ...    --crt_pem
    ...    ${crt_pem}
    ...    --key_pem
    ...    ${key_pem}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${name}_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=${name}
Ankaios insecure agent is started with name "${name}"
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-agent
    ...    --insecure
    ...    --name
    ...    ${name}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${name}_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=${name}

Ankaios agent is started with name "${name}"
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-agent
    ...    --name
    ...    ${name}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${name}_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=${name}

Ankaios agent is started with config file "${config_file_path}"
    &{agent_name}=    Run Keyword And Return    extract agent name from config file    ${config_file_path}
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-agent
    ...    --config_path
    ...    ${config_file_path}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${agent_name}_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=${agent_name}

Ankaios agent is started with name "${agent_name}" and config file "${config_file_path}"
    &{agent_name}=    Run Keyword And Return    extract agent name from config file    ${config_file_path}
    Start Process
    ...    %{ANK_BIN_DIR}${/}ank-agent
    ...    --name
    ...    ${agent_name}
    ...    --config_path
    ...    ${config_file_path}
    ...    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${agent_name}_logs.txt
    ...    env:RUST_LOG=debug
    ...    alias=${agent_name}

Ankaios server is terminated
    Terminate Process    ank-server    kill=True
    ${server_running}=    Is Process Running    ank-server
    Should Not Be True    ${server_running}

Ankaios agent with name "${name}" is terminated
    Terminate Process    ${name}    kill=True
    ${agent_running}=    Is Process Running    ${name}
    Should Not Be True    ${agent_running}

Podman has deleted all existing containers
    Run Process    podman    rm    --all    --force    --time    2    timeout=20    shell=True

Podman has deleted all existing pods
    Run Process    podman    pod    rm    --all    --force    --time    2    timeout=20    shell=True

Podman has deleted all existing volumes
    Run Process    podman    volume    rm    --all    --force    timeout=20    shell=True

Podman has terminated all existing containers
    Run Process    podman    stop    --all    timeout=20    shell=True

containerd has deleted all existing containers
    Run Process    nerdctl    stop    --time    2    $(nerdctl    ps    --all    --quiet    --no-trunc)    timeout=20    shell=True
    Run Process    nerdctl    rm    --force    $(nerdctl    ps    --all    --quiet    --no-trunc)    timeout=20    shell=True

containerd has terminated all existing containers
    Run Process    nerdctl    stop    $(nerdctl    ps    --all    --quiet    --no-trunc)    timeout=20    shell=True

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
        ...    ${expected_agent_name}
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

runtime "${runtime_name}" shall have assigned a container id for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds
    ${runtime_cli_name}=    Convert Runtime Name To Cli Process Name    ${runtime_name}

    runtime "${runtime_name}" shall have a container for workload "${workload_name}" with custom name "" on agent "${agent_name}" within "${timeout}" seconds
    ${container_id}    ${ankaios_instance_name}    Get Container Id And Name By Workload Name From Runtime    ${runtime_cli_name}    ${workload_name}
    Should Not Be Empty    ${container_id}    msg=Workload '${workload_name}' has no id assigned in podman!
    Set To Dictionary    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}    ${ankaios_instance_name}=${container_id}
    Set Global Variable    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}
    Log   ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING = '${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}'
    Set To Dictionary    ${ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING}    ${workload_name}=${container_id}
    Set Global Variable    ${ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING}
    Log   ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING = '${ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING}'

podman kube shall have assigned an id for pod "${pod_name}" of workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds
    volumes for "${workload_name}" shall exist on "${agent_name}" within "${timeout}" seconds
    ${pod_id}=    Get Pod Id By Pod Name From Podman    ${pod_name}
    Should Not Be Empty    ${pod_id}    msg=Received no pod id for workload '${pod_name}'!
    ${volume_name}=    Get Volume Name By Workload Name From Podman   ${workload_name}
    Should Not Be Empty    ${volume_name}    msg=Received no volume name for workload '${workload_name}'!
    ${ankaios_instance_name}=    Fetch From Left    ${volume_name}    .pods
    Set To Dictionary    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}    ${ankaios_instance_name}=${pod_id}
    Set Global Variable    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}
    Log   ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING = '${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}'

runtime "${runtime_name}" shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}" within "${timeout}" seconds
    ${runtime_cli_name}=    Convert Runtime Name To Cli Process Name    ${runtime_name}

    ${start_time}=    Get Time Secs
    ${workload_names}=    Create List
    ${used_name}=    Set Variable    ${name}
    ${workload_name_agent_name}=    Set Variable    ${name}
    IF    "${name}" == ""
        ${used_name}=    Set Variable    ${workload_name}
        ${workload_name_agent_name}=    To X Dot Y Format    ${workload_name}    ${agent_name}
    END
    WHILE    True
        ${workload_names}=    Get Workload Names From Runtime    ${runtime_cli_name}
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${found_index}=    Get Index From List    ${workload_names}    ${workload_name_agent_name}
        IF    ${found_index}!=-1    BREAK
        IF    ${elapsed_secs} >= ${timeout}    BREAK
    END
    Should Contain X Times
    ...    ${workload_names}
    ...    ${workload_name_agent_name}
    ...    1
    ...    msg=Workload '${used_name}' on agent '${agent_name}' not found in '${runtime_name}'!
    Log    Workload '${used_name}' on agent '${agent_name}' found in '${runtime_name}', found workloads: '${workload_names}'!

runtime "${runtime_name}" shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds
    ${runtime_cli_name}=    Convert Runtime Name To Cli Process Name    ${runtime_name}

    ${start_time}=    Get Time Secs
    ${workload_names}=    Create List
    ${workload_name_agent_name}=    To X Dot Y Format    ${workload_name}    ${agent_name}
    WHILE    True
        ${workload_names}=    Get Workload Names From Runtime    ${runtime_cli_name}
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        ${found_index}=    Get Index From List    ${workload_names}    ${workload_name_agent_name}
        IF    ${found_index}==-1    BREAK
        IF    ${elapsed_secs} >= ${timeout}    BREAK
    END
    Should Not Contain
    ...    ${workload_names}
    ...    ${workload_name_agent_name}
    ...    msg=Unexpected workload '${workload_name}' on agent '${agent_name}' found in '${runtime_name}'!

list of workloads shall be empty
    @{list_result}=    Create List
    Should Be Empty    item=${list_result}

the configs field inside the state shall be empty
    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank get state -o json    timeout=5
    ${result_dict}=    Json To Dict    ${command_result.stdout}
    ${desiredState}=    Get From Dictionary    ${result_dict}    desiredState
    ${configs}=    Get From Dictionary    ${desiredState}    configs
    Should Be Empty    ${configs}

the container of workload "${workload_name}" shall have a different id but same configuration on the runtime "${runtime_name}"
    ${runtime_cli_name}=    Convert Runtime Name To Cli Process Name    ${runtime_name}

    ${id_changed}=    Set Variable    ${False}
    ${start_time}=    Get Time Secs
    WHILE    True
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        IF    ${elapsed_secs} >= 20    BREAK
        ${current_workload_id}    ${current_ankaios_instance_name}    Get Container Id And Name By Workload Name From Runtime    ${runtime_cli_name}    ${workload_name}
        Sleep    0.2s
        IF    '${current_workload_id}' == '' or '${current_ankaios_instance_name}' == ''
            CONTINUE
        END

        ${workload_id}=    Get From Dictionary    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}    ${current_ankaios_instance_name}

        IF    '${current_workload_id}' != '${workload_id}'
            ${id_changed}=    Set Variable    ${True}
            BREAK
        END
    END

    Should Be True    ${id_changed}    msg=Workload '${workload_name}' has the same id '${workload_id}' and the same configuration on podman!

the container of workload "${workload_name}" shall have a different id on the podman runtime
    ${id_changed}=    Set Variable    ${False}
    ${start_time}=    Get Time Secs
    WHILE    True
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        IF    ${elapsed_secs} >= 20    BREAK
        ${current_workload_id}    ${current_ankaios_instance_name}    Get Container Id And Name By Workload Name From Runtime    "podman"    ${workload_name}
        Sleep    0.2s
        IF    '${current_workload_id}' == '' or '${current_ankaios_instance_name}' == ''
            CONTINUE
        END

        ${workload_id}=    Get From Dictionary    ${ANKAIOS_WORKLOAD_NAME_TO_PODMAN_ID_MAPPING}    ${workload_name}

        IF    '${current_workload_id}' != '${workload_id}'
            ${id_changed}=    Set Variable    ${True}
            BREAK
        END
    END

    Should Be True    ${id_changed}    msg=Workload '${workload_name}' has the same id '${workload_id}' on podman!

the container of workload "${workload_name}" shall have the same id and same configuration on the runtime "${runtime_name}"
    ${runtime_cli_name}=    Convert Runtime Name To Cli Process Name    ${runtime_name}
    ${id_changed}=    Set Variable    ${True}
    ${start_time}=    Get Time Secs
    WHILE    True
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        IF    ${elapsed_secs} >= 20    BREAK
        ${current_workload_id}    ${current_ankaios_instance_name}    Get Container Id And Name By Workload Name From Runtime    ${runtime_cli_name}    ${workload_name}
        Sleep    0.2s
        IF    '${current_workload_id}' == '' or '${current_ankaios_instance_name}' == ''
            CONTINUE
        END

        ${old_container_id}=    Get From Dictionary    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}    ${current_ankaios_instance_name}

        IF    '${current_workload_id}' == '${old_container_id}'
            ${id_changed}=    Set Variable    ${False}
            BREAK
        END
    END

    Should Not Be True    ${id_changed}    msg=Workload '${workload_name}' has a different '${old_container_id}' on ${runtime_name}!

the pod "${pod_name}" of workload "${workload_name}" shall have a different id but same configuration on the podman kube runtime
    ${id_changed}=    Set Variable    ${False}
    ${start_time}=    Get Time Secs
    ${workload_states}=   Run Command    %{ANK_BIN_DIR}${/}ank get state workloadStates -o json
    ${ankaios_instance_name}=    Get Instance Name From Ankaios Workload States    ${workload_states.stdout}    ${workload_name}
    Should Not Be Empty    ${ankaios_instance_name}    msg=No instance name found for workload '${workload_name}' in ankaios workload states!
    WHILE    True
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        IF    ${elapsed_secs} >= 30    BREAK
        ${current_pod_id}=    Get Pod Id By Pod Name From Podman    ${pod_name}
        Sleep    0.2s
        IF    '${current_pod_id}' == '' or '${ankaios_instance_name}' == ''
            CONTINUE
        END

        ${pod_id}=    Get From Dictionary    ${ANKAIOS_INSTANCE_NAME_TO_PODMAN_ID_MAPPING}    ${ankaios_instance_name}

        IF    '${current_pod_id}' != '${pod_id}'
            ${id_changed}=    Set Variable    ${True}
            BREAK
        END
    END

    Should Be True    ${id_changed}    msg='Workload ${pod_name}' has the same pod id '${pod_id}' and the same configuration on podman kube runtime!

the last command shall list the connected agent "${agent_name}"
    ${table_as_list}=    Table To List    ${CURRENT_RESULT.stdout}
    ${table_as_dict}=    Table To Dict    ${table_as_list}    NAME
    Dictionary Should Contain Key    ${table_as_dict}    ${agent_name}
    ${agent_dict}=    Get Agent Dict    ${table_as_dict}    ${agent_name}
    Dictionary Should Contain Key    ${agent_dict}    CPU USAGE
    Dictionary Should Contain Key    ${agent_dict}    FREE MEMORY

the last command shall list the config "${config_name}"
    Config Name Shall Exist In List    ${config_name}    ${CURRENT_RESULT.stdout}

the last command shall not list the config "${config_name}"
    Run Keyword And Expect Error    Config name ${config_name} does not exist in the list    the last command shall list the config "${config_name}"

the last command shall contain the workload "${workload_name}"
    ${list_result}=    Json To Dict    ${CURRENT_RESULT.stdout}
    ${desired_state}=    Get From Dictionary    ${list_result}    desiredState
    ${workloads}=    Get From Dictionary    ${desired_state}    workloads
    Should Contain   ${workloads}    ${workload_name}

the last command shall only contain agent and runtime
    ${expected_keys}=    Create List    agent    runtime

    ${list_result}=    Json To Dict    ${CURRENT_RESULT.stdout}
    dictionary ${list_result} should have exact keys    desiredState
    ${desired_state}=    Get From Dictionary    ${list_result}    desiredState
    ${workloads}=    Get From Dictionary    ${desired_state}    workloads
    @{workloads}=    Get Dictionary Values   ${workloads}
    FOR    ${w}    IN   @{workloads}
        # ${keys}=    Get Dictionary Keys    ${w}
        # Should Be Equal    ${keys}   ${expected_keys}
        dictionary ${w} should have exact keys   @{expected_keys}
    END

dictionary ${dictionary} should have exact keys
    [Arguments]    @{expected_keys}
    ${keys}=    Get Dictionary Keys   ${dictionary}
    Sort List    ${expected_keys}
    Should Be Equal    ${keys}    ${expected_keys}

Setup Ankaios for Control Interface test
    Setup Ankaios
    Prepare Test Control Interface Workload
    Retrieve current Ankaios version

The controller workload wait for ${milliseconds} milliseconds
    internal_controller_wait_milliseconds    ${milliseconds}

The controller workload has no access rights
    The controller workload sends initial hello with correct version

The controller workload is allowed to ${operation} on ${filter_mask}
    internal_state_allow_control_interface    ${operation}     ${filter_mask}
    The controller workload sends initial hello with correct version

The controller workload is forbidden to ${operation} on ${filter_mask}
    internal_state_deny_control_interface    ${operation}     ${filter_mask}
    The controller workload sends initial hello with correct version

The controller workload does not send hello
    internal_state_allow_control_interface    read    *

The controller workload can receive logs from ${workload_names}
    internal_log_allow_control_interface    ${workload_names}
    The controller workload sends initial hello with correct version

The controller workload cannot receive logs from ${workload_names}
    internal_log_deny_control_interface    ${workload_names}
    The controller workload sends initial hello with correct version

The controller workload requests the logs of ${workload_names_with_agents}
    internal_add_logs_request_command    ${workload_names_with_agents}

The controller workload gets the logs of ${workload_names}
    internal_get_logs_command    ${workload_names}

The controller workload cancels the logs request of ${workload_names}
    internal_add_cancel_logs_request_command    ${workload_names}

The controller workload sends initial hello with correct version
    internal_send_initial_hello    ${ANKAIOS_VERSION}

The controller workload updates the state with manifest "${manifest}"
    The controller workload updates the state with manifest "${manifest}" and update mask ${EMPTY}

The controller workload updates the state with manifest "${manifest}" and update mask ${update_mask}
    internal_add_update_state_command    ${manifest}    ${update_mask}

The controller workload gets the state
    The controller workload gets the state of fields ${EMPTY}

The controller workload gets the state of fields ${field_mask}
    internal_add_get_state_command    ${field_mask}

The controller workload subscribes to the state of fields ${field_mask}
    internal_add_get_state_command    ${field_mask}    true

The controller workload gets event for fields ${field_mask}
    internal_add_get_event_command    ${field_mask}

The last result contains exactly the workloads ${workload_names}
    internal_check_control_interface_workloads_in_last_result    ${workload_names}

The last result contains no workloads
    internal_check_control_interface_workloads_in_last_result    ${EMPTY}

In the last result the workload ${workload_name} has exactly the fields ${field_names}
    internal_check_control_interface_workload_fields_in_last_result    ${workload_name}    ${field_names}

The last result has added fields ${field_names}
    internal_check_control_interface_altered_fields_in_last_result    addedFields    ${field_names}

The last result has no added fields
    internal_check_control_interface_altered_fields_in_last_result    addedFields    ${EMPTY}

The last result has updated fields ${field_names}
    internal_check_control_interface_altered_fields_in_last_result    updatedFields    ${field_names}

The last result has no updated fields
    internal_check_control_interface_altered_fields_in_last_result    updatedFields    ${EMPTY}

The last result has removed fields ${field_names}
    internal_check_control_interface_altered_fields_in_last_result    removedFields    ${field_names}

The last result has no removed fields
    internal_check_control_interface_altered_fields_in_last_result    removedFields    ${EMPTY}

The controller workload cancels events for fields ${field_mask}
    internal_add_cancel_events_command    ${field_mask}

The controller workload requests shall all succeed
    ${ankaios_config_folder}=    create_control_interface_config_for_test
    Ankaios server is started with config "${ankaios_config_folder.name}${/}startup_config.yaml"
    Ankaios agent is started with name "${AGENT_NAME}"
    And the workload "controller" shall have the execution state "Succeeded(Ok)" on agent "${AGENT_NAME}" within "20" seconds
    internal_check_all_control_interface_requests_succeeded    ${ankaios_config_folder.name}
    internal_check_all_result_expectations_succeeded    ${ankaios_config_folder.name}

The controller workload requests shall all succeed with second agent
    ${ankaios_config_folder}=    create_control_interface_config_for_test
    Ankaios server is started with config "${ankaios_config_folder.name}${/}startup_config.yaml"
    Ankaios agent is started with name "${AGENT_NAME}"
    BuiltIn.Sleep    1s
    Ankaios agent is started with name "${AGENT_2_NAME}"
    # we do no want the agent to send a second update of its state, which would be send after 1 second
    BuiltIn.Sleep    500ms
    Ankaios agent with name "${AGENT_2_NAME}" is terminated
    And the workload "controller" shall have the execution state "Succeeded(Ok)" on agent "${AGENT_NAME}" within "20" seconds
    internal_check_all_control_interface_requests_succeeded    ${ankaios_config_folder.name}
    internal_check_all_result_expectations_succeeded    ${ankaios_config_folder.name}

The controller workload last request shall fail
    ${ankaios_config_folder}=    create_control_interface_config_for_test
    Ankaios server is started with config "${ankaios_config_folder.name}${/}startup_config.yaml"
    Ankaios agent is started with name "${AGENT_NAME}"
    And the workload "controller" shall have the execution state "Succeeded(Ok)" on agent "${AGENT_NAME}" within "20" seconds
    internal_check_last_control_interface_request_failed    ${ankaios_config_folder.name}

The controller workload requests shall all fail
    ${ankaios_config_folder}=    create_control_interface_config_for_test
    Ankaios server is started with config "${ankaios_config_folder.name}${/}startup_config.yaml"
    Ankaios agent is started with name "${AGENT_NAME}"
    And the workload "controller" shall have the execution state "Succeeded(Ok)" on agent "${AGENT_NAME}" within "20" seconds
    internal_check_all_control_interface_requests_failed    ${ankaios_config_folder.name}

The controller workload shall get a closed connection
    ${ankaios_config_folder}=    create_control_interface_config_for_test
    Ankaios server is started with config "${ankaios_config_folder.name}${/}startup_config.yaml"
    Ankaios agent is started with name "${AGENT_NAME}"
    And the workload "controller" shall have the execution state "Succeeded(Ok)" on agent "${AGENT_NAME}" within "20" seconds
    internal_check_control_interface_closed    ${ankaios_config_folder.name}

The controller workload has no access to Control Interface
    ${ankaios_config_folder}=    create_control_interface_config_for_test
    Ankaios server is started with config "${ankaios_config_folder.name}${/}startup_config.yaml"
    Ankaios agent is started with name "${AGENT_NAME}"
    And the workload "controller" shall have the execution state "Succeeded(Ok)" on agent "${AGENT_NAME}" within "20" seconds
    internal_check_no_access_to_control_interface    ${ankaios_config_folder.name}

the mount point for the control interface has not been generated for ${agent_name}
    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank get state -o json
    internal_check_if_mount_point_has_not_been_generated_for    ${agent_name}    ${command_result}

the mount point for the control interface has been generated for ${agent_name}
    Run Keyword And Expect Error    the mount point has been generated    the mount point for the control interface has not been generated for ${agent_name}

the mount point for the workload files of workload "${workload_name}" on agent "${agent_name}" has not been generated
    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank get state -o json
    Run Keyword And Expect Error    the workload file for ${workload_name} does not exist    Internal Check Workload Files Exists    ${command_result.stdout}    ${workload_name}    ${agent_name}
Ankaios server is available
    the command "%{ANK_BIN_DIR}${/}ank get workloads" finished with exit code "0" within "2" seconds

the cli log collection shall output "${total_log_lines}" log lines in total in the expected format for all workloads
    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank get workloads
    @{list_result}=    table to list    ${command_result.stdout}
    ${joined_workload_names}=    Set Variable    ${EMPTY}
    FOR    ${workload}    IN    @{list_result}
        ${workload_name}=    Get From Dictionary    ${workload}    WORKLOAD NAME
        ${joined_workload_names}=    Catenate    ${joined_workload_names}    ${workload_name}
    END

    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank logs ${joined_workload_names}

    ${log_lines}=    Split String    ${command_result.stdout}    separator=\n
    # Ankaios CLI outputs log lines with a trailing newline, so we need to remove the last line
    ${last_log_line}=    Get From List    ${log_lines}    -1
    IF    '${last_log_line}' == ''
        Remove From List    ${log_lines}    -1
    END
    ${len_log_lines}=    Get Length    ${log_lines}
    Should Be Equal As Integers    ${len_log_lines}    ${total_log_lines}    msg=Expected ${total_log_lines} log lines, but got ${len_log_lines}

    # check the format of each log line
    ${dict_workload_names}=    Table To Dict    ${list_result}    WORKLOAD NAME
    FOR    ${log_line}    IN    @{log_lines}
        ${splitted_log_line}=    Split String    ${log_line}    separator=${SPACE}
        Dictionary Should Contain Key    ${dict_workload_names}    ${splitted_log_line}[0]    msg=Log line does not start with the workload name: ${log_line}
        ${len_splitted_log_line}=    Get Length    ${splitted_log_line}
        Should Be True    ${len_splitted_log_line} >= 2    msg=Log line does not have the expected format: ${log_line}
    END

Get Running Container Names
    [Documentation]    Returns a list of all running and stopped container names from podman
    ${result}=    Run Process    podman    ps    -a    --format    {{.Names}}    shell=True
    Log    Raw container names output: "${result.stdout}"

    # Filter out empty lines and get container names
    ${all_lines}=    Split String    ${result.stdout}    \n
    ${container_names}=    Create List
    FOR    ${line}    IN    @{all_lines}
        ${trimmed_line}=    Strip String    ${line}
        IF    "${trimmed_line}" != ""
            Append To List    ${container_names}    ${trimmed_line}
        END
    END

    RETURN    ${container_names}

Log Container Debugging Information
    [Documentation]    Logs debugging information about workloads, pods, and containers
    # Debug: Check workload status
    ${command_result}=    Run Command    %{ANK_BIN_DIR}${/}ank get workloads
    Log    Ankaios workloads: ${command_result.stdout}

    # Debug: Check podman pods
    ${result}=    Run Process    podman    pod    ls    shell=True
    Log    Podman pods: ${result.stdout}

    # Debug: Check all containers (running and stopped)
    ${result}=    Run Process    podman    ps    -a    shell=True
    Log    All containers: ${result.stdout}

Identify Target Container For Control Interface
    [Arguments]    ${container_names}    ${target_container_name}=container_A    ${pod_name}=pod_A
    [Documentation]    Identifies the target container and non-target containers based on naming patterns
    ${target_container}=    Set Variable    ${EMPTY}
    ${non_target_containers}=    Create List

    FOR    ${container_name}    IN    @{container_names}
        # Check various possible naming patterns for the target container
        IF    "${pod_name}" in "${container_name}" and "${target_container_name}" in "${container_name}" and "sidecar" not in "${container_name}"
            ${target_container}=    Set Variable    ${container_name}
            Log    Identified target container: ${target_container}
        ELSE
            Append To List    ${non_target_containers}    ${container_name}
        END
    END

    Log    Target container: ${target_container}
    Log    Non-target containers: ${non_target_containers}

    RETURN    ${target_container}    ${non_target_containers}

Verify Control Interface Access In Containers
    [Arguments]    ${target_container}    ${non_target_containers}
    [Documentation]    Verifies that only the target container has access to the control interface

    IF    "${target_container}" == ""
        Log    Could not identify target container from naming patterns, but control interface was configured
        Fail    Could not identify target container for control interface access verification
    END

    # Test target container access to control interface
    ${result}=    Run Process    podman    exec    ${target_container}    test    -d    /run/ankaios/control_interface    shell=True
    IF    ${result.rc} == 0
        Log    Target container has access to control interface path

        # Verify it is a named pipe
        ${result}=    Run Process    podman    exec    ${target_container}    test    -p    /run/ankaios/control_interface/input    shell=True
        Should Be Equal As Integers    ${result.rc}    0    msg=Control interface should be a named pipe in target container
        ${result}=    Run Process    podman    exec    ${target_container}    test    -p    /run/ankaios/control_interface/output    shell=True
        Should Be Equal As Integers    ${result.rc}    0    msg=Control interface should be a named pipe in target container

        # Test non-target containers should NOT have access
        FOR    ${non_target_container}    IN    @{non_target_containers}
            ${result}=    Run Process    podman    exec    ${non_target_container}    test    -e    /run/ankaios/control_interface    shell=True
            Should Not Be Equal As Integers    ${result.rc}    0    msg=Non-target container ${non_target_container} should NOT have control interface access
        END

        Log    Successfully verified control interface access is limited to target container
    ELSE
        Log    Target container exists but control interface not accessible (container may have exited)
        Fail    Target container ${target_container} does not have access to control interface path
    END

Verify Multi Container Control Interface Access
    [Arguments]    ${workload_name}=simple    ${container_name}=container_A    ${pod_name}=pod_A
    [Documentation]    Verifies that control interface access is properly limited in multi-container environments

    ${container_names}=    Get Running Container Names
    ${container_count}=    Get Length    ${container_names}

    ${target_container}    ${non_target_containers}=    Identify Target Container For Control Interface    ${container_names}    ${container_name}    ${pod_name}
    Verify Control Interface Access In Containers    ${target_container}    ${non_target_containers}

the workload state of workload "${workload_name}" shall contain an additional info signaling retries within "${timeout}" seconds
    ${start_time}=    Get Time Secs
    WHILE    True
        ${current_secs}=    Get Time Secs
        ${elapsed_secs}=    Evaluate    ${current_secs} - ${start_time}
        IF    ${elapsed_secs} >= ${timeout}    BREAK
        ${res}=    Run Process   %{ANK_BIN_DIR}${/}ank get workloads    timeout=10    shell=True
        IF     ${res.rc} != 0
            Sleep    0.2s
            CONTINUE
        END
        ${result_list}=    Table To List    ${res.stdout}
        ${result_dict}=    Table To Dict    ${result_list}    WORKLOAD NAME
        Log    ${result_dict}
        ${workload_state}=    Get From Dictionary    ${result_dict}    ${workload_name}
        ${additional_info}=    Get From Dictionary    ${workload_state}    ADDITIONAL INFO
        IF    "Retry " in $additional_info
            BREAK
        END

        Sleep    0.2s
    END
    # ${workload_state}=    Get From Dictionary    ${result_dict}    ${workload_name}
    # Should Contain Key    ${workload_state}    additionalInfo
    # ${additional_info}=    Get From Dictionary    ${workload_state}    additionalInfo
    # Should Contain    ${additional_info}    retries

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
    runtime "podman" shall have a container for workload "${workload_name}" with custom name "" on agent "${agent_name}" within "1" seconds

podman shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}"
    runtime "podman" shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}" within "1" seconds

podman shall not have a container for workload "${workload_name}" on agent "${agent_name}"
    runtime "podman" shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "1" seconds

podman has assigned a container id for workload "${workload_name}" on agent "${agent_name}"
    runtime "podman" shall have assigned a container id for workload "${workload_name}" on agent "${agent_name}" within "10" seconds

podman shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds
    runtime "podman" shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds

the container of workload "${workload_name}" shall have the same id and same configuration on the podman runtime
    the container of workload "${workload_name}" shall have the same id and same configuration on the runtime "podman"

the container of workload "${workload_name}" shall have a different id but same configuration on the podman runtime
    the container of workload "${workload_name}" shall have a different id but same configuration on the runtime "podman"

podman kube has assigned an id for pod "${pod_name}" of workload "${workload_name}" on agent "${agent_name}"
    podman kube shall have assigned an id for pod "${pod_name}" of workload "${workload_name}" on agent "${agent_name}" within "30" seconds

the container of workload "${workload_name}" shall have the same id and same configuration on the containerd runtime
    the container of workload "${workload_name}" shall have the same id and same configuration on the runtime "containerd"

containerd shall have a container for workload "${workload_name}" on agent "${agent_name}"
    runtime "containerd" shall have a container for workload "${workload_name}" with custom name "" on agent "${agent_name}" within "1" seconds

containerd shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}"
    runtime "containerd" shall have a container for workload "${workload_name}" with custom name "${name}" on agent "${agent_name}" within "1" seconds

containerd shall not have a container for workload "${workload_name}" on agent "${agent_name}"
    runtime "containerd" shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "1" seconds

containerd shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds
    runtime "containerd" shall not have a container for workload "${workload_name}" on agent "${agent_name}" within "${timeout}" seconds

the container of workload "${workload_name}" shall have a different id but same configuration on the containerd runtime
    the container of workload "${workload_name}" shall have a different id but same configuration on the runtime "containerd"

containerd has assigned a container id for workload "${workload_name}" on agent "${agent_name}"
    runtime "containerd" shall have assigned a container id for workload "${workload_name}" on agent "${agent_name}" within "10" seconds

the last command shall finish with an error
    the last command finished with an error

the last command shall finish with exit code "${exit_code}"
    the last command finished with exit code "${exit_code}"

user starts event listener with format "${format}" and field mask "${field_mask}" in background
    ${field_mask_arg}=    Set Variable If    "${field_mask}" == ""    ${EMPTY}    ${field_mask}
    ${cmd}=    Set Variable If    "${field_mask}" == ""    %{ANK_BIN_DIR}${/}ank -k get events -o ${format}    %{ANK_BIN_DIR}${/}ank -k get events ${field_mask_arg} -o ${format}
    Start Process    ${cmd}    shell=True    stdout=target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/events_error.txt    alias=event-listener

the event output shall contain workload "${workload_name}"
    Event Output Shall Contain Workload    ${workload_name}    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall be valid yaml format
    Event Output Shall Be Valid Yaml Format    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall be valid json format
    Event Output Shall Be Valid Json Format    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain timestamp
    Event Output Shall Contain Timestamp    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain timestamp in RFC3339 format
    Event Output Shall Contain Timestamp In RFC3339 Format    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain only desiredState workloads
    Event Output Shall Contain Only DesiredState Workloads    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain at least "${count}" events
    Event Output Shall Contain At Least N Events    ${count}    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

all events shall contain timestamp
    Each Event In Output Shall Contain Timestamp    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain workloadStates
    Event Output Shall Contain WorkloadStates    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain desiredState
    Event Output Shall Contain DesiredState    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

the event output shall contain altered fields with removed workloads
    Event Output Shall Contain Altered Fields With Removed Workloads    target/robot_tests_result/${TEST_FOLDER_NAME}/events_output.txt

all workloads of agent "${agent_name}" shall start via events
    User Waits For All Workloads To Start Via Events    ${agent_name}    60

all workloads of agent "${agent_name}" shall start via events within "${timeout}" seconds
    User Waits For All Workloads To Start Via Events    ${agent_name}    ${timeout}

the workload "${workload_name}" shall reach state "${expected_state}" on agent "${agent_name}" via events
    User Waits For Workload To Reach State Via Events    ${workload_name}    ${agent_name}    ${expected_state}    60

the workload "${workload_name}" shall reach state "${expected_state}" on agent "${agent_name}" via events within "${timeout}" seconds
    User Waits For Workload To Reach State Via Events    ${workload_name}    ${agent_name}    ${expected_state}    ${timeout}

the workload "${workload_name}" shall not exist on agent "${agent_name}" via events
    User Waits For Workload Removal Via Events    ${workload_name}    ${agent_name}    60

the workload "${workload_name}" shall not exist on agent "${agent_name}" via events within "${timeout}" seconds
    User Waits For Workload Removal Via Events    ${workload_name}    ${agent_name}    ${timeout}
