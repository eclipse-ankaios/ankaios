*** Settings ***
Documentation    Tests to verify that ank cli updates workloads correctly
...              by adapting the portmapping from the host port 8081 to host port 8082.
Resource    ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Variables ***
${default_state_yaml_file}
${new_state_yaml_file} 

*** Test Cases ***
# my integration test test
Test Ankaios CLI update workload
    [Setup]        Run Keywords    Setup Ankaios
    ...            AND             Set Global Variable    ${default_state_yaml_file}    ${CONFIGS_DIR}/default.yaml
    ...            AND             Set Global Variable    ${new_state_yaml_file}    %{ANKAIOS_TEMP}/itest_update_workload_new_state.yaml
    # Preconditions
    Given Ankaios server is started with "ank-server --startup-config ${default_state_yaml_file}"
    And Ankaios agent is started with "ank-agent --name agent_A -p /tmp/podman.sock"
    And all workloads of agent "agent_A" have an initial execution state
    And the command "curl localhost:8081" finished with exit code "0"
    # Actions
    When user triggers "ank get state > ${new_state_yaml_file}" 
    And user updates the state "${new_state_yaml_file}" with "currentState.workloads.nginx.runtimeConfig.ports.0.hostPort=8082"
    And user triggers "ank set state -f ${new_state_yaml_file} currentState.workloads.nginx"
    And user triggers "ank get workloads" 
    # Asserts
    Then the workload "nginx" shall have the execution state "Running"
    And the command "curl localhost:8082" shall finish with exit code "0"
    [Teardown]    Clean up Ankaios
