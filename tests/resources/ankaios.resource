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
    Start Process    @{cmd}    stderr=target/robot_tests_result/agent_A_logs.txt

Run in foreground
    [Arguments]    ${cli_command}
    Log    Run in foreground: ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    ${run_result}=    Run Process    @{cmd}    shell=True
    RETURN    ${run_result}

the command "${cli_command}" finished with exit code "${exit_code}"
    Log    run ${cli_command}
    ${run_result}=    Run in foreground    ${cli_command}
    Log    ${run_result.stdout}
    ${exit_code_int}=    Convert To Integer    ${exit_code}
    Should Be Equal    ${run_result.rc}    ${exit_code_int}

the command "${cli_command}" shall finish with exit code "${exit_code}"
    the command "${cli_command}" finished with exit code "${exit_code}"

Ankaios server is started with config "${config_path}"
    Log    Ankaios server is started with config ${config_path}
    Start Process    %{ANK_BIN_DIR}${/}ank-server    --startup-config    ${config_path}    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/server_logs.txt

Ankaios agent is started with name "${name}"
    Log    Ankaios agent is started with name ${name}
    Start Process    %{ANK_BIN_DIR}${/}ank-agent    --name    ${name}    stderr=target/robot_tests_result/${TEST_FOLDER_NAME}/${name}_logs.txt

Podman has deleted all existing containers
    Run Process    podman    rm    -a    -f    timeout=20    shell=True

Podman has deleted all existing pods
    Run Process    podman    pod    stop    -a    &&    podman    pod    rm    -a    -f    timeout=20    shell=True

Podman has deleted all existing volumes
    Run Process    podman    volume    rm    -a    timeout=20    shell=True

all workloads of agent "${agent_name}" have an initial execution state
    Log    Waiting for initial execution states of agent '${agent_name}'
    ${result}=    wait for initial execution state    %{ANK_BIN_DIR}${/}ank get workloads    ${agent_name}    30    5
    Should Be True    ${result}

user triggers "${cli_command}"
    Log    Triggers ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    Log    ${cmd}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]
    ${CURRENT_RESULT}=    Run Process    @{cmd}    timeout=20    shell=True
    Log    ${CURRENT_RESULT.stdout}
    Set Global Variable    ${CURRENT_RESULT}

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" from agent "${expected_agent_name}"
    the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}"

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}"
    Log    Assert: ${workload_name} shall have execution state ${expected_execution_state}
    @{list_result}=    table to list    ${CURRENT_RESULT.stdout}
    &{dict_result}=    table to dict    ${list_result}    WORKLOAD NAME
    &{item}=    Get From Dictionary    ${dict_result}    ${workload_name}
    ${actual_execution_state}=    Get From Dictionary    ${item}    EXECUTION STATE
    ${actual_agent_name}=    Get From Dictionary    ${item}    AGENT
    Should be equal    ${actual_execution_state}    ${expected_execution_state}
    Should be equal    ${actual_agent_name}    ${expected_agent_name}

user updates the state "${new_state_yaml_file}" with "${field_value}"
    @{key_val}=    Split String    ${field_value}    separator==
    ${yaml_data}=    Get File    ${new_state_yaml_file}
    ${result_dict}=    Yaml To Dict    ${yaml_data}
    Log    ${result_dict}
    ${new_config}=    Replace Config    ${result_dict}    filter_path=${key_val}[0]    new_value=${key_val}[1]
    Log    ${new_config}
    Write Yaml    new_yaml=${new_config}    path=${new_state_yaml_file}

# TODO delete this one:
# the workload "${workload_name}" has the execution state "${execution_state}"
#     ${result}=    Wait For Execution State
#     ...    %{ANK_BIN_DIR}${/}ank get workloads
#     ...    ${workload_name}
#     ...    ${execution_state}
#     ...    20
#     ...    5
#     Should Be True    ${result}

the workload "${workload_name}" shall have the execution state "${execution_state}"
    the workload "${workload_name}" shall have the execution state "${execution_state}" on agent ""

the workload "${workload_name}" shall have the execution state "${execution_state}" on agent "${expected_agent_name}"
    the workload "${workload_name}" shall have the execution state "${execution_state}" on agent "${expected_agent_name}" within "0" ms

the workload "${workload_name}" shall have the execution state "${execution_state}" on agent "${expected_agent_name}" within "${timeout}" ms
    # TODO

the workload "${workload_name}" shall not exist
    the workload "${workload_name}" shall not exist within "0" ms

the workload "${workload_name}" shall not exist within "${timeout}" ms
    # TODO add timeout handling with calling the command
    Log    Assert: ${workload_name} shall not exist
    @{list_result}=    table to list    ${CURRENT_RESULT.stdout}
    &{dict_result}=    table to dict    ${list_result}    WORKLOAD NAME
    Dictionary Should Not Contain Key    ${dict_result}    ${workload_name}

user executes system app "${cli_command}"
    Log    Executes ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}    escaping=True
    Log    ${cmd}
    ${CURRENT_RESULT}=    Run Process    @{cmd}    timeout=20    shell=True
    Log    ${CURRENT_RESULT.stdout}
    Set Global Variable    ${CURRENT_RESULT}

the result is valid JSON
    Log    ${CURRENT_RESULT.stdout}
    ${json_dict}=    Json To Dict    raw=${CURRENT_RESULT.stdout}
    Log    ${json_dict}
    RETURN    ${json_dict}

# TODO: clean up. Most of these are probabaly not needed anymore
the JSON array "${dict_array}" shall contain key "${main_key}" with subkey "${sub_key}" with the subvalue "${sub_value}"
    Log    ${dict_array}
    FOR    ${dict_array_item}    IN    @{dict_array}
        Log    ${dict_array_item}
        Dictionary Should Contain Key    dictionary=${dict_array_item}    key=${main_key}
        Dictionary Should Contain Item
        ...    dictionary=${dict_array_item["${main_key}"]}
        ...    key=${sub_key}
        ...    value=${sub_value}
    END

the JSON array "${dict_array}" shall contain key "${main_key}" with subkey "${sub_key}" which matches the expression "${reg_exp}"
    Log    ${dict_array}
    FOR    ${dict_array_item}    IN    @{dict_array}
        Log    ${dict_array_item}
        Dictionary Should Contain Key    dictionary=${dict_array_item}    key=${main_key}
        Should Match Regexp    string=${dict_array_item["${main_key}"]["${sub_key}"]}    pattern=${reg_exp}
    END

the JSON array "${dict_array}" shall contain key "${key}" which matches the expression "${reg_exp}"
    Log    ${dict_array}
    FOR    ${dict_array_item}    IN    @{dict_array}
        Log    ${dict_array_item}
        Should Match Regexp    string=${dict_array_item["${key}"]}    pattern=${reg_exp}
    END

the JSON array "${dict_array}" shall contain key "${key}" which not matches the expression "${reg_exp}"
    Log    ${dict_array}
    FOR    ${dict_array_item}    IN    @{dict_array}
        Log    ${dict_array_item}
        Should Not Match Regexp    string=${dict_array_item["${key}"]}    pattern=${reg_exp}
    END

the JSON array "${dict_array}" shall contain array "${key}" which matches the expression "${reg_exp}"
    Log    ${dict_array}
    FOR    ${dict_array_item}    IN    @{dict_array}
        Log    ${dict_array_item}
        Dictionary Should Contain Key    dictionary=${dict_array_item}    key=${key}
        FOR    ${key_array_item}    IN    @{dict_array_item["${key}"]}
            Log    ${key_array_item}
            Should Match Regexp    string=${key_array_item}    pattern=${reg_exp}
        END
    END

the JSON array "${dict_array}" shall contain array "${key}" which contains value "${value}"
    Log    ${dict_array}
    FOR    ${dict_array_item}    IN    @{dict_array}
        Log    ${dict_array_item}
        Dictionary Should Contain Key    dictionary=${dict_array_item}    key=${key}
        List Should Contain Value    list_=${dict_array_item["${key}"]}    value=${value}
    END

volumes for "${workload_name}" shall exists on "${agent_name}" within "${timeout}" ms
    # TODO: check that the volume is there

volumes for "${workload_name}" shall not exists on "${agent_name}" within "${timeout}" ms
    ${result}=    check podman kube volumes gone    ${workload_name}    ${agent_name}    ${timeout}
    Should Be True     ${result}

podman shall have a container for workload ${workload_name} on agent "${agent_name}"
    podman shall have a container for workload ${workload_name} on agent "${agent_name}" within "0" ms

podman shall have a container for workload ${workload_name} on agent "${agent_name}" within "${timeout}" ms
    podman shall have a container for workload ${workload_name} with custom name "${workload_name}" on agent "${agent_name}" within "${timeout}" ms

podman shall have a container for workload ${workload_name} with custom name "${name}" on agent "${agent_name}" within "${timeout}" ms
    # TODO

podman shall not have a container for workload ${workload_name} on agent "${agent_name}"
    podman shall not have a container for workload ${workload_name} on agent "${agent_name}" within "0" ms

podman shall not have a container for workload ${workload_name} on agent "${agent_name}" within "${timeout}" ms
    # TODO
