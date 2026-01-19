# Copyright (c) 2024 Elektrobit Automotive GmbH
#
# This program and the accompanying materials are made available under the
# terms of the Apache License, Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
#
# SPDX-License-Identifier: Apache-2.0


*** Settings ***
Documentation       Tests to verify that Ankaios workloads are being restarted after having added a Control Interface.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource

*** Variables ***
${agent_name}            agent_A

*** Test Cases ***

# [stest->swdd~agent-control-interface-created-for-eligible-workloads~1]
Test Ankaios workload successful start-up without a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    # Actions
    # Asserts
    Then the mount point for the control interface has not been generated for ${agent_name}
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-control-interface-created-for-eligible-workloads~1]
Test Ankaios workload restart after update without a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple_with_control.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    # Actions
    When user triggers "ank -k apply ${CONFIGS_DIR}/simple.yaml"
    # Asserts
    Then the mount point for the control interface has not been generated for ${agent_name}
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-control-interface-created-for-eligible-workloads~1]
Test Ankaios workload restart after update with a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    And the mount point for the control interface has not been generated for ${agent_name}
    # Actions
    When user triggers "ank apply ${CONFIGS_DIR}/simple_with_control.yaml"
    # Asserts
    Then the mount point for the control interface has been generated for ${agent_name}
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-control-interface-created-for-eligible-workloads~1]
Test Ankaios containerd workload restart after update with a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios

    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple_containerd.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "{agent_name}" have an initial execution state
    And the mount point for the control interface has not been generated for ${agent_name}
    # Actions
    When user triggers "ank apply ${CONFIGS_DIR}/simple_with_control_containerd.yaml"
    # Asserts
    Then the mount point for the control interface has been generated for ${agent_name}
    [Teardown]    Clean up Ankaios

# [stest->swdd~agent-closes-control-interface-on-missing-initial-hello~1]
Test Control Interface closes connection when initial hello missing
    [Setup]           Run Keywords    Setup Ankaios for Control Interface test
    Given the controller workload does not send hello
    And the controller workload gets the state
    Then The controller workload shall get a closed connection
    [Teardown]    Clean up Ankaios

# [stest->swdd~server-desired-state-field-conventions~1]
# [stest->swdd~api-access-rules-filter-mask-convention~1]
Test workload with empty Control Interface access field mask rejected
    [Setup]           Run Keywords    Setup Ankaios
    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    # Actions
    When user triggers "ank apply ${CONFIGS_DIR}/faulty_with_control_as_empty.yaml"
    # Asserts
    Then the last command finished with exit code "1"
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-mounts-control-interface~1]
Test server started with empty Control Interface access field mask fails
    [Setup]           Run Keywords    Setup Ankaios
    # Actions
    Given Ankaios server is started with config "${CONFIGS_DIR}/faulty_with_control_as_empty.yaml"
    # Asserts
    Then the last command finished with exit code "1"
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-mounts-control-interface~1]
Test Ankaios podman-kube workload restart after update without a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios
    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple_kube_with_control.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    And podman kube has assigned an id for pod "simple-pod" of workload "simple-kube" on agent "${agent_name}"
    # Actions
    When user triggers "ank apply ${CONFIGS_DIR}/simple_kube.yaml"
    # Asserts
    Then the mount point for the control interface has not been generated for ${agent_name}
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-mounts-control-interface~1]
Test Ankaios podman-kube workload restart after update with a Control Interface access
    [Setup]           Run Keywords    Setup Ankaios
    # Preconditions
    Given Ankaios server is started with config "${CONFIGS_DIR}/simple_kube.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    And the mount point for the control interface has not been generated for ${agent_name}
    # Actions
    When user triggers "ank apply ${CONFIGS_DIR}/simple_kube_with_control.yaml"
    # Asserts
    Then the mount point for the control interface has been generated for ${agent_name}
    And the pod "simple-pod" of workload "simple-kube" shall have a different id but same configuration on the podman kube runtime
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-mounts-control-interface~1]
Test target path from control interface access is limited to the designated pod and container
    [Setup]           Run Keywords    Setup Ankaios
    # Preconditions
    And Ankaios server is started with config "${CONFIGS_DIR}/multi_container_podman_kube.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    And the workload "simple" shall have the execution state "Running(Ok)" on agent "${agent_name}"
    And the mount point for the control interface has been generated for ${agent_name}
    # Asserts
    Then verify multi container control interface access    simple
    [Teardown]    Clean up Ankaios

# [stest->swdd~podman-kube-mounts-control-interface~1]
Test target path from control interface access is limited to the designated deployment pod and container
    [Setup]           Run Keywords    Setup Ankaios
    # Preconditions
    And Ankaios server is started with config "${CONFIGS_DIR}/multi_container_podman_kube_deployment.yaml"
    And Ankaios agent is started with name "${agent_name}"
    And all workloads of agent "${agent_name}" have an initial execution state
    And the workload "simple" shall have the execution state "Running(Ok)" on agent "${agent_name}"
    And the mount point for the control interface has been generated for ${agent_name}
    # Asserts
    Then verify multi container control interface access    simple    container_A    pod_A-pod
    [Teardown]    Clean up Ankaios
