*** Settings ***
Documentation    System Tests Suite Setup
Library          Process
Library          OperatingSystem
Suite Setup      Run Setup Script

*** Keywords ***
Run Setup Script
    ${setup_dir}=    Set Variable    ${EXECDIR}/target/robot_tests_result
    Create Directory    ${setup_dir}
    ${result}=    Run Process
    ...    command=${EXECDIR}/tools/setup_robot_tests.sh
    ...    shell=True
    ...    stdout=${setup_dir}/setup.log
    ...    stderr=STDOUT
    IF    ${result.rc} != 0
        Fail    Setup script failed with return code ${result.rc}. Check logs at ${setup_dir}/setup.log
    END
    
