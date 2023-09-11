*** Settings ***
Documentation    A test suite for workloads.
Resource         resources/ankaios.resource
Suite Teardown    Terminate All Processes    kill=True

*** Test Cases ***
# my integration test test
Test Ankaios CLI get workloads
    # Preconditions
    Given Ankaios server is started with "/home/ubuntu/conoa/ankaios/server/resources/startConfig.yaml"
    And Ankaios agent is started with "ank-agent --name agent_B"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios agent is started with "ank-agent --name agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank get workloads"
    # Assert
    Then result should be "my result" 
    [Teardown]    Clean up Ankaios
