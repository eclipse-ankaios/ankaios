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

print("Connecting to Ankaios control interface and subscribing to workload state changes...")
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
