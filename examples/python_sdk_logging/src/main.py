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

"""
This example demonstrates how to request logs from workloads running on the system.
It connects to the Ankaios control interface, retrieves the names of workloads,
and requests logs for those workloads that match a specific name ("screamer" in this case).
It then prints the logs received from the log campaign for a limited time (10 seconds).
"""

from ankaios_sdk import Ankaios, AnkaiosException
import sys, signal
import time

# Create a new Ankaios object.
# The connection to the control interface is automatically done at this step.
# The Ankaios class supports context manager syntax:
with Ankaios() as ankaios:

    def signal_handler(sig, frame):
        sys.exit(0)

    # Add a SIGTERM handler to allow a clean shutdown
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        # Get the workload instance names of the workloads running on the system
        workload_states_list = ankaios.get_workload_states().get_as_list()
        workload_names = [
            workload_state.workload_instance_name
            for workload_state in workload_states_list
            if workload_state.workload_instance_name.workload_name == "screamer"
        ]

        # Request the logs of the workloads
        print("Requesting logs")
        log_campaign = ankaios.request_logs(
            workload_names=workload_names,
            follow=True,
        )

        # Check what workloads were accepted for logging
        print("Accepted workload names for logging: {}"
              .format([
                  instance_name.workload_name
                  for instance_name in log_campaign.accepted_workload_names
                  ]))

        start_time = time.time()
        while time.time() - start_time < 10000:
            # Get the logs from the queue
            log = log_campaign.queue.get()

            # Print the logs
            print(str(log))

    # Catch the AnkaiosException in case something went wrong with apply_workload
    except AnkaiosException as e:
        print("Ankaios Exception occurred: ", e)

    # Stop receiving logs
    print("Stopping log campaign...")
    ankaios.stop_receiving_logs(log_campaign)
    print("Done.")
