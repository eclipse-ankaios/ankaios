*** Settings ***
Documentation     This is a resource file, that can contain variables and keywords.
...               Keywords defined here can be used where this Keywords.resource in loaded.
Library           Process

*** Keywords ***
Ankaios server is started with "${startup_config}"
    LOG    Ankaios server is started with ${startup_config}
    # Start Server   ${startup_config}
    Start Process    %{ANK_BIN_DIR}${/}ank-server    --startup-config    ${startup_config}

Ankaios server is started
    LOG    Ankaios server is started

Ankaios agent is started
    LOG    Ankaios agent is started

user triggers "${cli_command}"
    LOG    Triggers ${cli_command}
    ${CURRENT_RESULT} =    Run Process    %{ANK_BIN_DIR}${/}ank    get    workloads
    LOG    ${CURRENT_RESULT.stdout}
    SET GLOBAL VARIABLE    ${CURRENT_RESULT}

result should be "${expected}"
    LOG    Checking result ${CURRENT_RESULT.stdout}
    
