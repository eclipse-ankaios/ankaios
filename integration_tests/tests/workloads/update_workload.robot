*** Settings ***
Documentation    Update workload test cases.
Resource    resources/ankaios.resource
Resource    resources/variables.resource

*** Test Cases ***
# my integration test test
Test Ankaios CLI update workload
    # Preconditions
    Given Ankaios server is started with "ank-server --startup-config ${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with "ank-agent --name agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # And the workload "nginx" is reachable
    # Actions
    When user triggers "ank get state > ${TEMPDIR}/itest_update_workload_new_state.yaml" 
    &{result_dict}=    Yaml To Dict    ${TEMPDIR}/itest_update_workload_new_state.yaml
    Log    ${result_dict}
    &{new_config}=    Replace Config    ${result_dict}    filter_path=currentState.workloads.nginx.runtimeConfig.ports.0.hostPort  new_value=8082
    Log    ${new_config}
    # Asserts
    # Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    [Teardown]    Clean up Ankaios
