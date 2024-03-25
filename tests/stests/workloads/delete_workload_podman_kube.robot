*** Comments ***
# Copyright (c) 2023 Elektrobit Automotive GmbH
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
Documentation       Tests to verify that Ankaios can create Podman workloads.

Resource            ../../resources/ankaios.resource
Resource            ../../resources/variables.resource


*** Test Cases ***

# [stest->swdd~agent-supports-podman-kube-runtime~1]
# [stest->swdd~podman-kube-delete-workload-downs-manifest-file~1]
# [stest->swdd~podman-kube-delete-removes-volumes~1]
Test Ankaios Podman delete kube workload
    [Setup]    Run Keywords    Setup Ankaios

    # Preconditions
    # This test assumes that all pods and volume in the podman have been created with this test -> clean it up first
    Given Podman has deleted all existing pods
    And Podman has deleted all existing volumes
    And Ankaios server is started with config "${CONFIGS_DIR}/kube.yaml"
    And Ankaios agent is started with name "agent_A"
    And all workloads of agent "agent_A" have an initial execution state
    # Actions
    When user triggers "ank delete workload hello-k8s"
    # Asserts
    Then podman shall not have a container for workload "hello-pod-hello-container" on agent "agent_A"
    And volumes for "hello-k8s" shall not exist on "agent_A" within "20" seconds
    And podman shall not have a container for workload "hello-k8s" on agent "agent_A" within "20" seconds
    And the workload "hello-k8s" shall not exist on agent "agent_A" within "20" seconds
    [Teardown]    Clean up Ankaios
