# System tests

## General overview

System tests are a critical phase of software testing, aimed at evaluating the entire software system as a whole to ensure that it meets its specified requirements and functions correctly in its intended environment. These tests are conducted after unit and integration testing and serve as a comprehensive validation of the software's readiness for deployment.

Here are key aspects of system tests:

1. **End-to-End Evaluation:** System tests assess the software's performance, functionality, and reliability in a real-world scenario, simulating the complete user journey. They cover all aspects of the system, from the user interface to the backend processes.

2. **Functional and Non-Functional Testing:** These tests not only verify that the software's features work as intended (functional testing) but also assess non-functional attributes like performance, scalability, security, and usability.

3. **Scenario-Based Testing:** Test scenarios are designed to replicate various user interactions, use cases, and business workflows. This includes testing different paths, inputs, and error conditions to ensure the system handles them correctly.

4. **Interoperability Testing:** In cases where the software interacts with external systems or components, system tests evaluate its compatibility and ability to communicate effectively with these external entities.

5. **Data Integrity and Security:** Ensuring the protection of sensitive data and the integrity of information is a critical part of system testing. This includes checking for vulnerabilities and ensuring compliance with security standards.

6. **Performance Testing:** Assessing the system's response times, resource utilization, and scalability under various load conditions to ensure it can handle expected levels of usage.

7. **Regression Testing:** System tests often include regression testing to ensure that new features or changes do not introduce new defects or disrupt existing functionality.

## Robot test framework for system tests

The [Robot test framework](https://robotframework.org/), often referred to as just "Robot Framework," is a popular open-source test automation framework used for automating test cases in various software applications. It is designed to be easy to use, highly readable, and adaptable for both beginners and experienced testers. It employs a keyword-driven approach, which means that test cases are written using a combination of keywords that represent actions, objects, and verifications. These keywords can be custom-defined by using Python programming language or come from libraries specific to the application under test. One of the standout features of Robot Framework is its human-readable syntax. Test cases are written in plain text composed with defined keywords, making it accessible to non-programmers and allowing stakeholders to understand and contribute to test case creation. Because of the ability to create custom keywords, a pool of domain specific and generic keywords could be defined to form an Ankaios project specific language for writing test cases.This makes it possible to directly use the test specifications written in natural language or the same wording of it to write automated test cases. This is the main reason why we use this test framework for system tests in Ankaios.

## System tests structure

```text
ankaios                              # Ankaios root
  |--tests                           # Location for system tests and their resources
  |  |--resources                    # Location for test resources
  |  |  |--configs                   # Location for test case specific start-up configuration files
  |  |  |  |--default.yaml           # A start-up configuration file
  |  |  |  |--... <----------------  # Add more configuration files here!
  |  |  |
  |  |  |--ankaios_library.py        # Ankaios keywords implementations
  |  |  |--ankaios.resource          # Ankaios keywords
  |  |  |--variables.resource        # Ankaios variables
  |  |  |--... <-------------------  # Add more keywords and keywords implementation resources here!
  |  |
  |  |--stests                       # Location for system tests
  |  |  |--workloads                 # Location for tests with specific test subject focus e.g. "workloads" for tests related "workloads"
  |  |  |  |--list_workloads.robot   # A test suite testing "list workloads"
  |  |  |  |--... <----------------  # Add more tests related to "workloads" here!
  |  |  |... <---------------------  # Add test subject focus here!
```

## System test creation

### A generic Ankaios system test structure

The most common approach to create a robot test is using the space separated format where pieces of the data, such as keywords and their arguments, are separated from each others with two or more spaces.
A basic Ankaios system test consists of the following sections:

```robot
# ./tests/stests/workloads/my_workload_stest.robot

*** Settings ***
Documentation    Add test suit documentation here.      # Test suite documentation
Resource     ../../resources/ankaios.resource           # Ankaios specific keywords that forms the Ankaios domain language
Resource    ../../resources/variables.resource          # Ankaios variables e.g. CONFIGS_DIR

*** Test Cases ***
[Setup]        Setup Ankaios
# ADD YOUR SYSTEM TEST HERE!
[Teardown]    Clean up Ankaios
```

For more best practices about writing tests with Robot framework see [here](https://github.com/robotframework/HowToWriteGoodTestCases/blob/master/HowToWriteGoodTestCases.rst).

### Behavior-driven system test

Behavior-driven tests (BDT) use natural language specifications to describe expected system behavior, fostering collaboration between teams and facilitating both manual and automated testing. It's particularly valuable for user-centric and acceptance testing, ensuring that software aligns with user expectations. The Robot test framework supports BDT, and this approach shall be preferred for writing system tests in Ankaios the project.

Generic structure of BDT:

```robot
*** Test Cases ***
[Setup]        Setup Ankaios
Given  <preconditions>
When   <actions>
Then   <asserts>
[Teardown]    Clean up Ankaios
```

Example: System test testing listing of workloads.

```robot
*** Settings ***
Documentation    Tests to verify that ank cli lists workloads correctly.
Resource     ../../resources/ankaios.resource
Resource    ../../resources/variables.resource

*** Test Cases ***
Test Ankaios CLI get workloads
    [Setup]        Setup Ankaios
    # Preconditions
    Given Ankaios server is started with "ank-server --startup-config ${CONFIGS_DIR}/default.yaml"
    And Ankaios agent is started with "ank-agent --name agent_B"
    And all workloads of agent "agent_B" have an initial execution state
    And Ankaios agent is started with "ank-agent --name agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank -k get workloads"
    # Asserts
    Then the workload "nginx" shall have the execution state "Running" on agent "agent_A"
    And the workload "hello1" shall have the execution state "Removed" from agent "agent_B"
    And the workload "hello2" shall have the execution state "Succeeded" on agent "agent_B"
    And the workload "hello3" shall have the execution state "Succeeded" on agent "agent_B"
    [Teardown]    Clean up Ankaios
```

!!! note

    For Ankaios manifests that are used for system tests, only images from ghcr.io should be used.
    A lot of other registries (docker.io, quay.io) apply rate limits which might cause failures when executing the system tests.

### Run long-runtime system tests upon merge into main

To keep the pull request status check runtime short, system tests with a longer runtime (> 30-40 seconds) shall be excluded from the pull request CI/CD verification by assigning the tag "non_execution_during_pull_request_verification" directly to the test case. When the pull request is merged into the main branch, the system test is executed. A contributor shall check the test results of those system tests afterwards.

Example system test that runs only on merge into main:

```robot hl_lines="7"
...

*** Test Cases ***
...

Test Ankaios Podman stops retries after reaching the retry attempt limit
    [Tags]    non_execution_during_pull_request_verification
    [Setup]    Run Keywords    Setup Ankaios

...
```

## System test execution

!!! warning
    The system tests will delete all Podman containers, pods and volume.
    We recomment to only execute the system tests in the dev container.

A shell script is provided for the easy execution of the system tests. The script does the following:

1. It checks if the required Ankaios executables (`ank`, `ank-server` and `ank-agent`) are available at specified path.
1. It prints out the version number executables.
1. It starts all the tests under specified folder or a specific robot test file.
1. It stores the test result in the folder `{Ankaios root folder}/target/robot_tests_result`.

### Run in dev container

Generic syntax:

```bash
/workspaces/ankaios$ [ANK_BIN_DIR=path_to_ankaios_executables] tools/run_robot_tests <options> <directory or robot file>
```

If *ANK_BIN_DIR* is not provided the script looks in the path `{Ankaios root folder}/target/x86_64-unknown-linux-musl/debug` for the Ankaios executables.
The supported options are the same as of `robot` cli, so for more detailed description about it see [here](https://robotframework.org/robotframework/latest/RobotFrameworkUserGuide.html#using-command-line-options).

*Note: In order to be able to start `podman` runtime in the dev container properly, the dev container needs to be run in `privilege` mode.*

#### Example: Run all tests under the folder tests

```bash
/workspaces/ankaios$ tools/run_robot_tests.sh tests
```

Example output:

```text
Use default executable directory: /workspaces/ankaios/tools/../target/x86_64-unknown-linux-musl/debug
Found ank 0.1.0
Found ank-server 0.1.0
Found ank-agent 0.1.0
==============================================================================
Tests
==============================================================================
Tests.Stests
==============================================================================
Tests.Stests.Workloads
==============================================================================
Tests.Stests.Workloads.List Workloads :: List workloads test cases.
==============================================================================
Test Ankaios CLI get workloads                                        | PASS |
------------------------------------------------------------------------------
Tests.Stests.Workloads.List Workloads :: List workloads test cases.   | PASS |
1 test, 1 passed, 0 failed
==============================================================================
Tests.Stests.Workloads.Update Workload :: Update workload test cases.
==============================================================================
Test Ankaios CLI update workload                                      | PASS |
------------------------------------------------------------------------------
Tests.Stests.Workloads.Update Workload :: Update workload test cases. | PASS |
1 test, 1 passed, 0 failed
==============================================================================
Tests.Stests.Workloads                                                | PASS |
2 tests, 2 passed, 0 failed
==============================================================================
Tests.Stests                                                          | PASS |
2 tests, 2 passed, 0 failed
==============================================================================
Tests                                                                 | PASS |
2 tests, 2 passed, 0 failed
==============================================================================
Output:  /workspaces/ankaios/target/robot_tests_result/output.xml
Log:     /workspaces/ankaios/target/robot_tests_result/log.html
Report:  /workspaces/ankaios/target/robot_tests_result/report.html
```

#### Example: Run a single test file

```bash
/workspaces/ankaios$ tools/run_robot_tests.sh tests/stests/workloads/list_workloads.robot
```

Example output:

```text
Use default executable directory: /workspaces/ankaios/tools/../target/x86_64-unknown-linux-musl/debug
Found ank 0.1.0
Found ank-server 0.1.0
Found ank-agent 0.1.0
==============================================================================
List Workloads :: List workloads test cases.
==============================================================================
Test Ankaios CLI get workloads                                        | PASS |
------------------------------------------------------------------------------
List Workloads :: List workloads test cases.                          | PASS |
1 test, 1 passed, 0 failed
==============================================================================
Output:  /workspaces/ankaios/target/robot_tests_result/output.xml
Log:     /workspaces/ankaios/target/robot_tests_result/log.html
Report:  /workspaces/ankaios/target/robot_tests_result/report.html
```

## Integration in GitHub workflows

The execution of the system tests is integrated in the GitHub workflow build step and will be triggered on each commit on a pull request.
