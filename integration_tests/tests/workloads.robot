*** Settings ***
Documentation    A test suite for workloads.
Resource         resources/ankaios.resource
Suite Teardown    Terminate All Processes    kill=True

*** Test Cases ***
Test Ankaios CLI get workloads
    Given Ankaios server is started with "/home/ubuntu/conoa/ankaios/server/resources/startConfig.yaml"
    And Ankaios agent is started
    When user triggers "ank get workloads"
    Then result should be "my result"
