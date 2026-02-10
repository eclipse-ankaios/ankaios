# Tutorial: Registering for events

## Introduction

In this tutorial, you will learn how a workload can register for events.
We will create a simple console dashboard, showing the state of all workloads.
This tutorial assumes that the reader is familiar with the basics of Ankaios showcased in the tutorial [Sending and Receiving Vehicle Signals](tutorial-vehicle-signals.md).

To complete this tutorial, you will need a Linux platform, which can be a WSL2, RaspberryPi, a Linux PC or a virtual machine.
It's also assumed that the Ankaios setup has been performed using the default [installation](installation.md) script.

## Workload

This workload uses the [Python SDK for Eclipse Ankaios](https://eclipse-ankaios.github.io/ank-sdk-python).
It subscribes to the field mask `workloadStates.*.*.*.state` which corresponds to the execution state of all workloads in the system.
It will receive the initial state for this field mask upon registration, and then will receive updates the state of a workload changes (including adding and removing workloads).

### Source code

Let's take a look at the implementation:

```python title="main.py"
from ankaios_sdk import (
    Ankaios,
    AnkaiosException,
    WorkloadInstanceName,
)
import sys, signal
import re

SUBSCRIPTION_FIELD_MASKS = "workloadStates.*.*.*.state"
WORKLOAD_INSTANCE_NAME_FROM_FIELD_RE = r"workloadStates\.([^.]+)\.([^.]+)\.([^.]+)\.state"

# Handle SIGTERM as container workloads run with PID 1
def signal_handler(sig, frame):
    global ankaios
    del ankaios
    sys.exit(0)

signal.signal(signal.SIGTERM, signal_handler)

def workload_from_field(field: str):
    match = re.match(WORKLOAD_INSTANCE_NAME_FROM_FIELD_RE, field)
    if not match:
        return None
    return (
        match.group(1),
        match.group(2),
        match.group(3),
    )

def clear_screen():
    print("\033[2J\033[H", end="")

def print_workloads(workloads: dict[WorkloadInstanceName, str]):
    clear_screen()
    for workload_instance_name, state in workloads.items():
        print(f"{workload_instance_name[1]} on {workload_instance_name[0]}: {state}")
    sys.stdout.flush()

with Ankaios() as ankaios:
    try:
        event_queue = ankaios.register_event(
            field_masks=[SUBSCRIPTION_FIELD_MASKS],
        )
    except AnkaiosException as e:
        print("Ankaios Exception occurred during event registration: ", e)
        sys.exit(1)

    # Initialize the workloads database with the initial state
    workloads_db = {}
    initial_state = event_queue.get()
    for entry in initial_state.complete_state.get_workload_states().get_as_list():
        ws = entry.workload_instance_name
        workloads_db[(ws.agent_name, ws.workload_name, ws.workload_id)] = str(entry.execution_state.state)
    print_workloads(workloads_db)

    # Wait for events and update the workloads database accordingly
    while True:
        event_entry = event_queue.get()
        added_or_updated_fields = []
        added_or_updated_fields.extend(event_entry.added_fields)
        added_or_updated_fields.extend(event_entry.updated_fields)
        for field in added_or_updated_fields:
            workload  = workload_from_field(str(field))
            if workload is not None:
                state_entry = event_entry.complete_state.get_workload_states().get_for_instance_name(WorkloadInstanceName(*workload))
                workloads_db[workload] = str(state_entry.execution_state.state)
        for field in event_entry.removed_fields:
            workload  = workload_from_field(str(field))
            if workload is not None:
                del workloads_db[workload]
        print_workloads(workloads_db)
```

After connecting to the Ankaios control interface the workload subscribes to changes of workload states.

```python
with Ankaios() as ankaios:
    try:
        event_queue = ankaios.register_event(
            field_masks=[SUBSCRIPTION_FIELD_MASKS],
        )
```

The workload then receives the initial state using the `get` command on the `event_queue`
(the object is the same as for a `get_state` request with the same `field_masks`).
It uses this initial state to populate the internal database of workload states.

```python
    workloads_db = {}
    initial_state = event_queue.get()
    for entry in initial_state.complete_state.get_workload_states().get_as_list():
        ws = entry.workload_instance_name
        workloads_db[(ws.agent_name, ws.workload_name, ws.workload_id)] = str(entry.execution_state.state)
```

The workload then enters an event loop and waits for events from the `event_queue`.
It uses the data from the events to keep the internal database of workloads updated.
The state workloads from the `added_fields` and `updated_fields` are set in the database,
using the corresponding data from the `complete_state`.
Workloads from the `removed_fields` are deleted from the database.

```python
    while True:
        event_entry = event_queue.get()
        added_or_updated_fields = []
        added_or_updated_fields.extend(event_entry.added_fields)
        added_or_updated_fields.extend(event_entry.updated_fields)
        for field in added_or_updated_fields:
            workload  = workload_from_field(str(field))
            if workload is not None:
                state_entry = event_entry.complete_state.get_workload_states().get_for_instance_name(WorkloadInstanceName(*workload))
                workloads_db[workload] = str(state_entry.execution_state.state)
        for field in event_entry.removed_fields:
            workload  = workload_from_field(str(field))
            if workload is not None:
                del workloads_db[workload]
```

### Building the workload

You can build the workload using the following Dockerfile:

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bookworm

WORKDIR /usr/src/app

RUN pip install --no-cache-dir ankaios-sdk

COPY main.py ./

CMD [ "python", "./main.py" ]
```

And then call:

```shell
sudo podman build -t ank_simple_dashboard:latest .
```

## Deployment

If you have not yet installed Ankaios, please follow the instructions [here](installation.md).
The following examples assume that the installation script was used with the default options.

To start the dashboard when Ankaios is started, add the dashboard to the startup configuration.

```yaml title="/etc/ankaios/state.yaml"
apiVersion: v1
workloads:
  dashboard:
    runtime: podman
    agent: agent_A
    controlInterfaceAccess:
      allowRules:
        - type: StateRule
          operation: Read
          filterMasks:
            - "workloadStates.*.*.*.state"
    restartPolicy: NEVER
    runtimeConfig: |
      image: ank_simple_dashboard:latest
```

To see the currently running workloads, use the Ankaios CLI: `ank logs -f dashboard`.
You can start a new workload to see the output from the command above updated:

```sh
   ank run workload --runtime podman --agent agent_A \
     --config $'image: alpine\ncommandArgs: ["sh", "-c", "while true;do sleep 60;done"]' \
     sample
```

You can stop the sample workload with `ank delete workload sample`.
This will take some time, as the simple shell script of the sample workload ignores the SIGTERM signal.
