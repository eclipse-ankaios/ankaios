*** Settings ***
Documentation     This is a resource file, that can contain variables and keywords.
...               Keywords defined here can be used where this Keywords.resource in loaded.
Library           Process
Library           Collections
Library           OperatingSystem
Library           String
Library           resources/ankaios_library.py

*** Variables ***
${CURRENT_RESULT}

*** Keywords ***
Setup Ankaios
    ${run_result}=    Run Process  command=mktemp -d    shell=True
    Set Environment Variable    name=ANKAIOS_TEMP    value=${run_result.stdout}

Clean up Ankaios
    Terminate All Processes    kill=True
    Clean up Podman
    Run Command    rm -rf %{ANKAIOS_TEMP}    timeout=20
Clean up Podman
    Log    Clean up Podman
    @{list_result}=    Table To List   ${CURRENT_RESULT.stdout}
    @{workload_names}=    Get Column Values    ${list_result}    WORKLOAD NAME
    @{workload_container_ids}=    Get Container Ids By Workload Names    ${workload_names}
    Log    ${workload_container_ids}
    @{stop_cmd_list}=    Create List    podman stop
    @{rm_cmd_list}=    Create List    podman rm
    FOR  ${wln_id_tuple}  IN  @{workload_container_ids}
        @{wln_id_list}=    Convert To List    ${wln_id_tuple}
        Append To List    ${stop_cmd_list}    ${wln_id_list}[1]
        Append To List    ${rm_cmd_list}    ${wln_id_list}[1]
    END
    ${stop_cmd}=    Catenate    @{stop_cmd_list}
    ${rm_cmd}=    Catenate    @{rm_cmd_list}
    Run Command    ${stop_cmd}    timeout=20
    Run Command    ${rm_cmd}    timeout=20

Run in background
    [Arguments]    ${location}    ${cli_command}
    Log    Run in background: ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    Set List Value    ${cmd}    0    ${location}${/}${cmd}[0]  
    Start Process    @{cmd}

Run in forground
    [Arguments]    ${cli_command}
    Log    Run in background: ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    ${run_result}=    Run Process    @{cmd}    shell=True
    RETURN    ${run_result}

the command "${cli_command}" finished with exit code "${exit_code}"
    Log     run ${cli_command}
    ${run_result}=    Run in forground    ${cli_command}
    Log    ${run_result.stdout}
    ${exit_code_int}=    Convert To Integer     ${exit_code}
    Should Be Equal    ${run_result.rc}    ${exit_code_int}

the command "${cli_command}" shall finish with exit code "${exit_code}"
    the command "${cli_command}" finished with exit code "${exit_code}"

Ankaios server is started with "${ank_server_cli_command}"
    Log    Ankaios server is started with ${ank_server_cli_command}
    Run in background    %{ANK_BIN_DIR}    ${ank_server_cli_command}
    
Ankaios server is started
    Log    Ankaios server is started

Ankaios agent is started with "${ank_agent_cli_command}"
    Log    Ankaios agent is started with ${ank_agent_cli_command}
    Run in background    %{ANK_BIN_DIR}    ${ank_agent_cli_command}

all workloads of agent "${agent_name}" have an initial execution state
    Log    Waiting for initial execution states of agent '${agent_name}'
    ${result}=    wait for initial execution state    %{ANK_BIN_DIR}${/}ank get workloads    ${agent_name}    20
    Should Be True    ${result}

user triggers "${cli_command}"
    Log    Triggers ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    Log    ${cmd}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]  
    ${CURRENT_RESULT} =    Run Process    @{cmd}    timeout=20    shell=True
    Log    ${CURRENT_RESULT.stdout}
    Set Global Variable    ${CURRENT_RESULT}

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" from agent "${expected_agent_name}"
    the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}"

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}" 
    Log    Assert: ${workload_name} shall have execution state ${expected_execution_state}
    @{list_result}=    table to list   ${CURRENT_RESULT.stdout}
    &{dict_result}=    table to dict    ${list_result}    WORKLOAD NAME
    &{item}=    Get From Dictionary    ${dict_result}    ${workload_name}
    ${actual_execution_state}=    Get From Dictionary    ${item}    EXECUTION STATE
    ${actual_agent_name}=    Get From Dictionary    ${item}    AGENT
    Should be equal    ${actual_execution_state}    ${expected_execution_state}
    Should be equal    ${actual_agent_name}    ${expected_agent_name}

user updates the state "${new_state_yaml_file}" with "${field_value}"
    @{key_val}=    Split String    ${field_value}  separator==
    ${yaml_data}=    Get File    ${new_state_yaml_file}
    ${result_dict}=    Yaml To Dict    ${yaml_data}
    Log    ${result_dict}
    ${new_config}=    Replace Config    ${result_dict}    filter_path=${key_val}[0]  new_value=${key_val}[1]
    Log    ${new_config}
    Write Yaml    new_yaml=${new_config}  path=${new_state_yaml_file}

the workload "${workload_name}" has the execution state "${execution_state}"
    ${result}=    Wait For Execution State   %{ANK_BIN_DIR}${/}ank get workloads    ${workload_name}    ${execution_state}    20
    Should Be True    ${result}

the workload "${workload_name}" shall have the execution state "${execution_state}"
    the workload "${workload_name}" has the execution state "${execution_state}"
    
