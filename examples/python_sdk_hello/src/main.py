# Copyright (c) 2025 Elektrobit Automotive GmbH
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

from ankaios_sdk import Workload, Ankaios, WorkloadStateEnum, AnkaiosException, AnkaiosLogLevel, ControlInterfaceException
import sys, signal

# Create a new Ankaios object.
# The connection to the control interface is automatically done at this step.
# The Ankaios class supports context manager syntax:
with Ankaios(AnkaiosLogLevel.INFO) as ankaios:
    looping = True
    def signal_handler(sig, frame):
        global ankaios, looping
        looping = False
        del ankaios
        sys.exit(0)

    # Add a SIGTERM handler to allow a clean shutdown
    signal.signal(signal.SIGTERM, signal_handler)

    # Create a new workload
    workload = (
        Workload.builder()
        .workload_name("dynamic_nginx")
        .agent_name("agent_A")
        .runtime("podman")
        .restart_policy("NEVER")
        .runtime_config(
            'image: docker.io/library/nginx\ncommandOptions: ["-p", "8080:80"]'
        )
        .build()
    )

    try:
        # Run the workload
        update_response = ankaios.apply_workload(workload)

        # Get the WorkloadInstanceName to check later if the workload is running
        workload_instance_name = update_response.added_workloads[0]

        # Request the execution state based on the workload instance name
        ret = ankaios.get_execution_state_for_instance_name(
            workload_instance_name
        )
        if ret is not None:
            print(
                f"State: {ret.state}, substate: {ret.substate}, info: {ret.additional_info}"
            )

        # Wait until the workload reaches the running state
        try:
            ankaios.wait_for_workload_to_reach_state(
                workload_instance_name,
                state=WorkloadStateEnum.RUNNING,
                timeout=5,
            )
        except TimeoutError:
            print("Workload didn't reach the required state in time.")
        else:
            print("Workload reached the RUNNING state.")

    # Catch the AnkaiosException in case something went wrong with apply_workload
    except AnkaiosException as e:
        print("Ankaios Exception occurred: ", e)

    # Delete the workload
    update_response = ankaios.delete_workload(
        workload_instance_name.workload_name
    )
