# # Copyright (c) 2024 Elektrobit Automotive GmbH
# #
# # This program and the accompanying materials are made available under the
# # terms of the Apache License, Version 2.0 which is available at
# # https://www.apache.org/licenses/LICENSE-2.0.
# #
# # Unless required by applicable law or agreed to in writing, software
# # distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# # WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# # License for the specific language governing permissions and limitations
# # under the License.
# #
# # SPDX-License-Identifier: Apache-2.0
# *** Settings ***
# Documentation    Tests to verify that ank cli lists configs correctly.
# Resource     ../../resources/ankaios.resource
# Resource    ../../resources/variables.resource

# *** Test Cases ***

# Test server config file
#     [Setup]        Setup Ankaios
#     # Preconditions
#     Server is started with config file present
#     # This test assumes that all containers in the podman have been created with this test -> clean it up first
#     Given Podman has deleted all existing containers
#     # Actions
#     When user triggers "ank -k get configs`"
#     # Asserts
#     The last command shall list the config "server_config"
#     [Teardown]    Clean up Ankaios
