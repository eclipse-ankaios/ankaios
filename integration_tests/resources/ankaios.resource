*** Settings ***
Documentation     This is a resource file, that can contain variables and keywords.
...               Keywords defined here can be used where this Keywords.resource in loaded.
Library           Process
Library           Collections
Library           OperatingSystem
Library           resources/ankaios_library.py

*** Keywords ***
Clean up Ankaios
    Terminate All Processes    kill=True
    Clean up Podman
    
Clean up Podman
    LOG    Clean up Podman
    RUN COMMAND    podman stop -a    timeout=20
    RUN COMMAND    podman rm -a    timeout=20
Ankaios server is started with "${startup_config}"
    LOG    Ankaios server is started with ${startup_config}
    Start Process    %{ANK_BIN_DIR}${/}ank-server    --startup-config    ${startup_config}
    
Ankaios server is started
    LOG    Ankaios server is started

Ankaios agent is started with "${cli_command}"
    LOG    Ankaios agent is started with ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    LOG    ${cmd}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]  
    Start Process    @{cmd}

all workloads of agent "${agent_name}" have an initial execution state
    LOG    Waiting for initial execution states of agent '${agent_name}'
    wait for initial execution state    %{ANK_BIN_DIR}${/}ank get workloads    ${agent_name}    20

user triggers "${cli_command}"
    LOG    Triggers ${cli_command}
    ${cmd}=    Split Command Line    ${cli_command}
    LOG    ${cmd}
    Set List Value    ${cmd}    0    %{ANK_BIN_DIR}${/}${cmd}[0]  
    ${CURRENT_RESULT} =    Run Process    @{cmd}    timeout=20
    LOG    ${CURRENT_RESULT.stdout}
    SET GLOBAL VARIABLE    ${CURRENT_RESULT}

result should be "${expected}"
    LOG    Checking result ${CURRENT_RESULT.stdout}
    @{DICT_RESULT}=    table to list   ${CURRENT_RESULT.stdout}
    LOG    Dict result: @{DICT_RESULT}
    ${VAL}=     Get From List   ${DICT_RESULT}  0
    LOG    result: ${VAL}
    
