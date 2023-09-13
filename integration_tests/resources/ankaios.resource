*** Settings ***
Documentation     This is a resource file, that can contain variables and keywords.
...               Keywords defined here can be used where this Keywords.resource in loaded.
Library           Process
Library           Collections
Library           OperatingSystem
Library           String
Library           resources/ankaios_library.py

*** Keywords ***
Setup Ankaios
    ${run_result}=    Run Process  command=mktemp -d    shell=True
    Set Environment Variable    name=ANKAIOS_TEMP    value=${run_result.stdout}

Clean up Ankaios
    Terminate All Processes    kill=True
    Clean up Podman
    RUN COMMAND    rm -rf %{ANKAIOS_TEMP}    timeout=20
Clean up Podman
    LOG    Clean up Podman
    RUN COMMAND    podman stop -a    timeout=20
    RUN COMMAND    podman rm -a    timeout=20

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
    ${run_result}=    Run Process    @{cmd}
    RETURN    ${run_result}

the command "${cli_command}" finished with exit code "${exit_code}"
    Log     run ${cli_command}
    ${run_result}=    Run in forground    ${cli_command}
    Log    ${run_result.stdout}
    ${exit_code_int}=    Convert To Integer     ${exit_code}
    Should Be Equal    ${run_result.rc}    ${exit_code_int}


Ankaios server is started with "${ank_server_cli_command}"
    LOG    Ankaios server is started with ${ank_server_cli_command}
    Run in background    %{ANK_BIN_DIR}    ${ank_server_cli_command}
    
Ankaios server is started
    LOG    Ankaios server is started

Ankaios agent is started with "${ank_agent_cli_command}"
    LOG    Ankaios agent is started with ${ank_agent_cli_command}
    Run in background    %{ANK_BIN_DIR}    ${ank_agent_cli_command}

all workloads of agent "${agent_name}" have an initial execution state
    LOG    Waiting for initial execution states of agent '${agent_name}'
    ${result}=    wait for initial execution state    %{ANK_BIN_DIR}${/}ank get workloads    ${agent_name}    20
    Should Be True    ${result}

user triggers "${cli_command}"
    LOG    Triggers ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    LOG    ${cmd}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]  
    ${CURRENT_RESULT} =    Run Process    @{cmd}    timeout=20    shell=True
    LOG    ${CURRENT_RESULT.stdout}
    SET GLOBAL VARIABLE    ${CURRENT_RESULT}

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" from agent "${expected_agent_name}"
    the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}"

the workload "${workload_name}" shall have the execution state "${expected_execution_state}" on agent "${expected_agent_name}" 
    LOG    Assert: ${workload_name} shall have execution state ${expected_execution_state}
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
    
