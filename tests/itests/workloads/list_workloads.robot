*** Settings ***
Documentation    List workloads test cases.
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***
# [itest->swdd~cli-standalone-application~1]
# [itest->swdd~server-handle-cli-communication~1]
Test Ankaios CLI get workloads
    [Setup]        Setup Ankaios
    # Precondition
    Given Ankaios server is started with "ank-server --startup-config ${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with "ank-agent --name agent_B -p /tmp/podman.sock"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios agent is started with "ank-agent --name agent_A -p /tmp/podman.sock"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get workloads"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Removed" from agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded" on agent "agent_B"
    [Teardown]    Clean up Ankaios
