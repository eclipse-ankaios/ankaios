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

import subprocess
import time
import yaml
import json
import re
import uuid
import functools
import signal
from fnmatch import fnmatch
from robot.api import logger
from robot.libraries.BuiltIn import BuiltIn
from tempfile import TemporaryDirectory
from os import path, environ
from typing import Union
import shutil
import tomllib
import multiprocessing as mp


###############################################################################
## Global vars
###############################################################################


LIST_PATTERN: re.Pattern = re.compile("^[\"|\']*\\[.*\\][\"|\']*$")
CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START: str = "^"
EXPLICIT_DOT_IN_REGEX: str = "\\\\."
EXECUTABLE: str = '/bin/bash'
MANIFEST_TEMPLATE: str = "control_interface_workload.yaml.template"
STARTUP_MANIFEST: str = "startup_config.yaml"
DEFAULT_AGENT_NAME: str = "agent_A"
FORCE_TRACE: bool = False
EVENT_BUFFER: list = []
EVENTS_RECEIVED = mp.Event()
EVENT_PROCESS = None


if FORCE_TRACE:
    logger.trace = logger.info

###############################################################################
## General utils
###############################################################################


def run_command(command: str, timeout: float=3):
    try:
        return subprocess.run(command, timeout=timeout, shell=True, executable=EXECUTABLE, check=True, capture_output=True, text=True)
    except subprocess.CalledProcessError as e:
        logger.error(f"Command '{command}' failed with return code {e.returncode}. Error: {e.stderr.strip()}")
        return e
    except Exception as e:
        logger.error(f"{e}")
        return None

def table_to_list(raw: str) -> list:
    raw = raw.strip()
    splitted = raw.split('\n')

    # Skip all lines before the table
    header = ""
    while splitted and ("WORKLOAD NAME" and "NAME") not in header:
        header = splitted.pop(0)

    # Regex for extracting column names and positions
    column_regex = r'(([^\s]+\s?)+\s*)'
    # Replacement pattern for cleaning header
    clean_header_pattern = '\x1b[1G\x1b[1G'

    columns = [(x.group(0).strip(), x.start(), x.end()) for x in re.finditer(column_regex, header.replace(clean_header_pattern, ''))]
    logger.trace("columns: {}".format(columns))
    table = []
    for row in splitted:
        table_row = {}
        for c in columns:
            table_row[c[0]] = row[c[1]:c[2]].strip()
        table.append(table_row)

    logger.trace(table)

    return table


def table_to_dict(input_list: list, key):
    out_dict = {}
    for item in input_list:
        out_dict[item[key]] = item
        del item[key]
    logger.trace(out_dict)
    return out_dict


def to_x_dot_y_format(x: str, y: str) -> str:
    return f"{x}.{y}"


def to_x_dot_y_dot_z_format(x: str, y: str, z: str) -> str:
    return f"{x}.{y}.{z}"


def get_time_secs() -> float:
    return time.time()


def generate_request_id() -> str:
    return str(uuid.uuid4())


def replace_key(data: Union[dict, list], match: str, func: callable):
    if isinstance(data, dict):
        for k, v in data.items():
            if k == match:
                data[k] = func(v)
            replace_key(data[k], match, func)
    elif isinstance(data, list):
        for item in data:
            replace_key(item, match, func)


def parse_yaml(raw: str) -> dict:
    raw = raw.strip()
    if not raw:
        return {}
    try:
        return yaml.safe_load(raw)
    except yaml.YAMLError as e:
        logger.error(f"Error parsing YAML: {e}")
        raise ValueError(f"Invalid YAML format: {raw}")


def read_yaml(file_path: str) -> dict:
    with open(file_path) as file:
        content = file.read()
        return parse_yaml(content)


def write_yaml(new_yaml, path: str):
    with open(path,"w+") as file:
        replace_key(new_yaml, "runtimeConfig", yaml.dump)
        yaml.dump(new_yaml, file)


def yaml_to_dict(raw: str):
    y = parse_yaml(raw)
    replace_key(y, "runtimeConfig", parse_yaml)
    return y


def json_to_dict(raw: str) -> dict:
    json_data = json.loads(raw)
    return json_data

def convert_runtime_name_to_cli_process_name(container_engine_name: str) -> str:
    """Converts the containerd container engine name to the internally used nerdctl name.
    """
    container_engine_name = container_engine_name.lower()
    if container_engine_name == "containerd":
        return "nerdctl"
    else:
        return container_engine_name

###############################################################################
## Ankaios utils
###############################################################################


def get_agent_dict(table_dict: dict, agent_name: str) -> dict:
    agent_dict = table_dict.get(agent_name)
    assert agent_dict, f"Agent {agent_name} does not provide available resources information"
    logger.trace(agent_dict)
    return agent_dict


def remove_hash_from_workload_name(wn_hash_an_string: str) -> str:
    items = wn_hash_an_string.split('.')
    if len(items) == 3:
        return f"{items[0]}.{items[2]}"
    elif len(items) == 4:
        return f"{items[0]}.{items[2]}.{items[3]}"
    return items[0]


def get_workload_names_from_runtime(runtime_cli: str) -> list:
    res = run_command(f'{runtime_cli} ps --all --format "{{{{.Names}}}}"')
    raw = res.stdout.strip()
    raw_wln = raw.split('\n')
    workload_names = list(map(remove_hash_from_workload_name, raw_wln))
    logger.trace(workload_names)
    return workload_names


def get_volume_names_from_podman() -> list:
    res = run_command('podman volume ls --format "{{.Name}}"')
    raw = res.stdout.strip()
    raw_vols = raw.split('\n')
    vol_names = list(map(remove_hash_from_workload_name, raw_vols))
    logger.trace(vol_names)
    return vol_names


def get_volume_name_by_workload_name_from_podman(workload_name: str) -> str:
    res = run_command('podman volume ls --format "{{{{.Name}}}}" --filter name={}{}{}.*{}pods'\
                      .format(CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START, workload_name, EXPLICIT_DOT_IN_REGEX, EXPLICIT_DOT_IN_REGEX))
    volume_name = res.stdout.strip()
    logger.trace(volume_name)
    return volume_name


def get_container_id_and_name_by_workload_name_from_runtime(runtime_cli: str, workload_name: str) -> tuple[str, str]:
    command_str = '{} ps -a --no-trunc --format="{{{{.ID}}}} {{{{.Names}}}}" --filter=name={}{}{}.*'\
                  .format(runtime_cli, CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START, workload_name, EXPLICIT_DOT_IN_REGEX)

    # During fast restarts (especially with containerd/nerdctl) the runtime can temporarily
    # return non-zero exit codes like "container ... not found" due to stale tasks/metadata.
    # Treat those as transient and let callers (usually polling loops) retry.
    start_time = time.time()
    last_err = ""
    while True:
        res = run_command(command_str)
        if res is None:
            return "", ""

        if getattr(res, "returncode", 1) == 0:
            break

        last_err = (getattr(res, "stderr", "") or "").strip()
        elapsed = time.time() - start_time
        if elapsed >= 5:
            logger.warning(
                f"Command '{command_str}' did not succeed within 5s. Last error: '{last_err}'"
            )
            return "", ""

        logger.warning(
            f"Command '{command_str}' failed (rc={res.returncode}). Error: '{last_err}'. Retrying..."
        )
        time.sleep(0.2)

    raw = (res.stdout or "").strip()
    if not raw:
        return "", ""

    raw_wln = raw.split('\n')
    # 2-dim [[id,name],[id,name],...]
    container_ids_and_names = [line.split(maxsplit=1) for line in raw_wln if line.strip()]
    logger.trace(container_ids_and_names)

    expected_amount_of_rows = 1
    if len(container_ids_and_names) != expected_amount_of_rows:
        logger.warning(
            f"Expected {expected_amount_of_rows} row for workload name {workload_name} but found {len(container_ids_and_names)} rows"
        )
        return "", ""

    if len(container_ids_and_names[0]) < 2:
        return "", ""

    container_id, container_name = container_ids_and_names[0][0], container_ids_and_names[0][1]
    logger.trace(f"Container ID: {container_id}, Container Name: {container_name}")
    if not container_id or not container_name:
        return "", ""

    return container_id, container_name


def get_workload_instance_name_by_workload_name_from_podman(runtime_cli: str, workload_name: str) -> str:
    return get_container_id_and_name_by_workload_name_from_runtime(runtime_cli, workload_name)[1]


def get_pod_id_by_pod_name_from_podman(pod_name: str) -> str:
    res = run_command('podman pod ls --no-trunc --format="{{{{.ID}}}}" --filter=name={}{}'\
                      .format(CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START, pod_name))
    raw = res.stdout.strip()
    pod_ids = raw.split('\n')
    logger.trace(pod_ids)
    amount_of_rows = len(pod_ids)
    expected_amount_of_rows = 1
    assert amount_of_rows == expected_amount_of_rows, \
        f"Expected {expected_amount_of_rows} row for pod name {pod_name} but found {amount_of_rows} rows"

    pod_id = pod_ids[0]
    return pod_id

def workload_with_execution_state(table: list, workload_name: str, expected_state: str) -> list:
    logger.trace(table)
    if table and any([row["EXECUTION STATE"].strip() == expected_state for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name, table)]):
        return table
    return list()

def replace_config(data: Union[dict, list], filter_path: str, new_value: Union[str, int, dict]) -> Union[dict, list]:
    filter_path = filter_path.split('.')
    filter_iterator = iter(filter_path)
    next_level = data[next(filter_iterator)]
    for level in filter_iterator:
        if filter_path[-1] == level:
            break

        next_level = next_level[level] if isinstance(next_level, dict) else next_level[int(level)]
    next_level[filter_path[-1]] = int(new_value) if new_value.isdigit() else parse_yaml(new_value) if LIST_PATTERN.match(new_value) else new_value

    return data


###############################################################################
## Operations utils
###############################################################################


def find_control_interface_test_tag():
    global control_interface_tester_tag
    control_interface_tester_tag = subprocess.check_output('./tools/control_interface_workload_hash.sh').decode().strip()

def prepare_test_control_interface_workload():
    global control_interface_workload_config
    global control_interface_result_expectations
    global manifest_files_location
    global next_manifest_number
    global control_interface_allow_rules
    global control_interface_deny_rules
    global logs_requests
    global events_requests

    control_interface_workload_config = []
    control_interface_result_expectations = []
    manifest_files_location = []
    next_manifest_number = 0
    control_interface_allow_rules = []
    control_interface_deny_rules = []
    logs_requests = {}
    events_requests = {}


def create_control_interface_config_for_test() -> TemporaryDirectory:
    tmp = TemporaryDirectory()
    assert path.isdir(tmp.name) and path.exists(tmp.name), f"The temporary directory at {tmp.name} has not been created"
    logger.trace(f"commands.yaml content:\n{control_interface_workload_config}")
    write_yaml(new_yaml=control_interface_workload_config, path=path.join(tmp.name, "commands.yaml"))

    for manifest in manifest_files_location:
        shutil.copy(manifest["file_path"], path.join(tmp.name, manifest["internal_name"]))

    configs_dir = BuiltIn().get_variable_value("${CONFIGS_DIR}")

    with open(path.join(configs_dir, MANIFEST_TEMPLATE)) as startup_config_template, open(path.join(tmp.name, STARTUP_MANIFEST), "w") as startup_config:
        template_content = startup_config_template.read()
        content = template_content.format(temp_data_dir=tmp.name,
                                          allow_rules=json.dumps(control_interface_allow_rules),
                                          deny_rules=json.dumps(control_interface_deny_rules),
                                          control_interface_tester_tag=control_interface_tester_tag)
        logger.trace(f"Startup config content:\n{content}")
        startup_config.write(content)
    return tmp


def err_logging_decorator(func: callable) -> callable:
    # Decorator to log errors and the method that raised them
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        try:
            ret = func(*args, **kwargs)
        except Exception as e:
            logger.info(f"Python error in function \"{func.__name__}\":", also_console=False)
            raise e
        return ret
    return wrapper


def state_control_interface_convert_operation(operation: str) -> str:
    operation_lower = operation.lower()
    res = ""
    if "read" in operation_lower:
        res = "Read"
    if "write" in operation_lower:
        res += "Write"

    assert res != "", f"The operation(s) '{operation}' is/are unknown"
    return res


def add_to_manifest_list(manifest_file: str) -> str:
    global next_manifest_number
    global manifest_files_location

    internal_name = "manifest_{}.yaml".format(next_manifest_number)
    manifest_files_location.append({"file_path": manifest_file, "internal_name": internal_name})
    next_manifest_number += 1

    return internal_name


def extract_agent_name_from_config_file(config_file: str) -> str:
    with open(config_file, "rb") as f:
        parsed_config_file = tomllib.load(f)
        agent_name = parsed_config_file["name"]

        return agent_name

# MANDATORY FOR STABLE SYSTEM TESTS
@err_logging_decorator
def config_name_shall_exist_in_list(config_name: str, current_result: str):
    config_names_list = list(current_result.split("\n"))[1:]  # skip the header
    found = False

    for config in config_names_list:
        if config_name in config:
            found = True
            break

    assert found, f"Config name {config_name} does not exist in the list"


###############################################################################
## Internal operations - control interface rules
###############################################################################


@err_logging_decorator
def internal_state_allow_control_interface(operation: str, filter_mask: str):
    global control_interface_allow_rules

    filter_mask = filter_mask.replace(" and ", ", ").split(", ")
    control_interface_allow_rules.append({
        "type": "StateRule",
        "operation": state_control_interface_convert_operation(operation),
        "filterMasks": filter_mask
    })


@err_logging_decorator
def internal_state_deny_control_interface(operation: str, filter_mask: str):
    global control_interface_deny_rules

    filter_mask = filter_mask.replace(" and ", ", ").split(", ")
    control_interface_deny_rules.append({
        "type": "StateRule",
        "operation": state_control_interface_convert_operation(operation),
        "filterMasks": filter_mask
    })


@err_logging_decorator
def internal_log_allow_control_interface(workload_names: str):
    global control_interface_allow_rules

    workload_names = workload_names.replace(" and ", ", ").split(", ")
    control_interface_allow_rules.append({
        "type": "LogRule",
        "workloadNames": workload_names
    })

    # The controller should be able to get the workload states to get the
    # workload instance name in the controller
    internal_state_allow_control_interface("read", "workloadStates")


@err_logging_decorator
def internal_log_deny_control_interface(workload_names: str):
    global control_interface_deny_rules

    workload_names = workload_names.replace(" and ", ", ").split(", ")
    control_interface_deny_rules.append({
        "type": "LogRule",
        "workloadNames": workload_names
    })

    # The controller should be able to get the workload states to get the
    # workload instance name in the controller
    internal_state_allow_control_interface("read", "workloadStates")


###############################################################################
## Internal operations - control interface commands
###############################################################################

@err_logging_decorator
def internal_controller_wait_milliseconds(milliseconds: str):
    global control_interface_workload_config

    control_interface_workload_config.append({
        "command": {
            "type": "Timeout",
            "duration_ms": int(milliseconds)
        }
    })

@err_logging_decorator
def internal_add_update_state_command(manifest: str, update_mask: str):
    global control_interface_workload_config

    update_mask = update_mask.replace(" and ", ", ").split(", ")
    internal_manifest_name = add_to_manifest_list(manifest)
    control_interface_workload_config.append({
        "command": {
            "type": "UpdateState",
            "manifest_file": path.join("/data", internal_manifest_name),
            "update_mask": update_mask
        }
    })


@err_logging_decorator
def internal_send_initial_hello(version: str):
    global control_interface_workload_config

    if control_interface_workload_config and \
        control_interface_workload_config[-1]["command"]["type"] == "SendHello":
        logger.trace("Another SendHello received in a row, skipping it.")
        return

    control_interface_workload_config.append({
        "command": {
            "type": "SendHello",
            "version": version
        }
    })


@err_logging_decorator
def internal_add_get_state_command(field_mask: str, subscribe_for_events: bool=False):
    global control_interface_workload_config
    global events_requests

    request_id = generate_request_id()
    if subscribe_for_events:
        events_requests[field_mask] = request_id


    field_mask = field_mask.replace(" and ", ", ").split(", ")
    if field_mask == [""]:
        field_mask = []
    control_interface_workload_config.append({
        "command": {
            "type": "GetState",
            "field_mask": field_mask,
            "subscribe_for_events": subscribe_for_events,
            "request_id": request_id
        }
    })

@err_logging_decorator
def internal_add_cancel_events_command(field_mask: str):
    global control_interface_workload_config

    assert field_mask in events_requests, f"Workload names {field_mask} are not in previous events requests"
    request_id = events_requests.get(field_mask)
    control_interface_workload_config.append({
        "command": {
            "type": "CancelEvents",
            "request_id": request_id
        }
    })

@err_logging_decorator
def internal_add_get_event_command(field_mask: str):
    global control_interface_workload_config

    assert field_mask in events_requests, f"Workload names {field_mask} are not in previous event subscription"
    request_id = events_requests.get(field_mask)
    control_interface_workload_config.append({
        "command": {
            "type": "GetEvent",
            "request_id": request_id,
        }
    })

@err_logging_decorator
def internal_check_control_interface_workloads_in_last_result(workload_names: str):
    global control_interface_result_expectations

    workload_names = [m for m in workload_names.replace(" and ", ", ").split(", ") if m]
    control_interface_result_expectations.append({
        "response_number": len(control_interface_workload_config)-1,
        "type": "exact_workloads",
        "workload_names": workload_names}
    )

@err_logging_decorator
def internal_check_control_interface_workload_fields_in_last_result(workload_name: str, field_names: str):
    global control_interface_result_expectations

    field_names = field_names.replace(" and ", ", ").split(", ")
    control_interface_result_expectations.append({
        "response_number": len(control_interface_workload_config)-1,
        "type": "exact_workload_fields",
        "workload_name": workload_name,
        "field_names": field_names
    })

@err_logging_decorator
def internal_check_control_interface_altered_fields_in_last_result(alteration_type: str, masks: str):
    global control_interface_result_expectations

    masks = [m for m in masks.replace(" and ", ", ").split(", ") if m]
    control_interface_result_expectations.append({
        "response_number": len(control_interface_workload_config)-1,
        "type": "exact_altered_fields",
        "alteration_type": alteration_type,
        "masks": masks
    })


@err_logging_decorator
def internal_add_logs_request_command(workload_names_with_agents: str):
    """
    If no agent is specified, the default agent name will be used.

    Example of workload_names_with_agents:
    "workload1 and workload2, workload3"
    "workload1 and workload2 on agent1 and workload3 on agent2"
    """
    global control_interface_workload_config
    global logs_requests

    workload_names_with_agents = workload_names_with_agents.replace(" and ", ", ").split(", ")
    workload_names = []
    agent_names = []
    for item in workload_names_with_agents:
        if " on " in item:
            workload_name, agent_name = item.split(" on ")
            workload_names.append(workload_name.strip())
            agent_names.append(agent_name.strip())
        else:
            workload_names.append(item.strip())
            agent_names.append(DEFAULT_AGENT_NAME)

    request_id = generate_request_id()
    logs_requests[", ".join(workload_names)] = request_id

    control_interface_workload_config.append({
        "command": {
            "type": "RequestLogs",
            "workload_names": workload_names,
            "agent_names": agent_names,
            "request_id": request_id
        }
    })


@err_logging_decorator
def internal_get_logs_command(workload_names: str):
    global control_interface_workload_config
    global logs_requests

    workload_names = workload_names.replace(" and ", ", ")
    assert workload_names in logs_requests, f"Workload names {workload_names} are not in previous logs requests"
    request_id = logs_requests.get(workload_names)
    control_interface_workload_config.append({
        "command": {
            "type": "GetLogs",
            "request_id": request_id
        }
    })




@err_logging_decorator
def internal_add_cancel_logs_request_command(workload_names: str):
    global control_interface_workload_config

    workload_names = workload_names.replace(" and ", ", ")
    assert workload_names in logs_requests, f"Workload names {workload_names} are not in previous logs requests"
    request_id = logs_requests.get(workload_names)
    control_interface_workload_config.append({
        "command": {
            "type": "CancelLogs",
            "request_id": request_id
        }
    })


###############################################################################
## Internal operations - control interface checks
###############################################################################

@err_logging_decorator
def internal_check_all_control_interface_requests_succeeded(tmp_folder):
    output_file_path = path.join(tmp_folder, "output.yaml")
    assert path.exists(output_file_path), f"Output file {output_file_path} does not exist"
    output = read_yaml(output_file_path)
    logger.trace(output)
    for test_number, test_result in enumerate(output):
        if test_result["result"]["type"] == "OK":
            continue
        test_result = test_result["result"]["value"]["type"] == "Ok"
        assert test_result, \
            f"Expected request {test_number + 1} to succeed, but it failed"

@err_logging_decorator
def internal_check_all_result_expectations_succeeded(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for expectation in control_interface_result_expectations:
        response_number = expectation["response_number"]
        test_result = output[response_number]
        if expectation["type"] == "exact_workloads":
            expected_workload_names = expectation["workload_names"]
            try:
                actual_workload_names = list(test_result["result"]["value"]["value"][0]["desiredState"]["workloads"].keys())
            except:
                actual_workload_names = []
            assert set(expected_workload_names) == set(actual_workload_names), f"Expected workloads {expected_workload_names} but found {actual_workload_names}"
        elif expectation["type"] == "exact_workload_fields":
            workload_name = expectation["workload_name"]
            expected_field_names = expectation["field_names"]
            try:
                actual_field_names = [k for (k,v) in test_result["result"]["value"]["value"][0]["desiredState"]["workloads"][workload_name].items() if v is not None]
            except:
                actual_field_names = []
            assert set(expected_field_names) == set(actual_field_names), f"Expected fields {expected_field_names} but found {actual_field_names}"
        elif expectation["type"] == "exact_altered_fields":
            alteration_type = expectation["alteration_type"]
            expected_masks = expectation["masks"]
            try:
                actual_masks = test_result["result"]["value"]["value"][1][alteration_type]
            except KeyError:
                actual_masks = []
            actual_masks_clone = actual_masks.copy()
            for em in expected_masks:
                failed = True
                for i in range(len(actual_masks)):
                    am = actual_masks[i]
                    if fnmatch(am, em):
                        failed = False
                        del actual_masks[i]
                        break
                if failed:
                    assert False, f"Expected {alteration_type} to match {expected_masks} but found {actual_masks_clone}"
            if len(actual_masks) != 0:
                assert False, f"Expected {alteration_type} to match {expected_masks} but found {actual_masks_clone}"

@err_logging_decorator
def internal_check_last_control_interface_request_failed(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    logger.trace(output)
    last_test_result = output[-1]
    test_result = last_test_result["result"]["value"]["type"] == "Err"
    assert test_result, "Expected the last request to fail, but it succeeded"

@err_logging_decorator
def internal_check_all_control_interface_requests_failed(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    logger.trace(output)
    for test_number,test_result in enumerate(output):
        if test_result["result"]["type"] != "SendHelloResult":
            test_result = test_result["result"]["value"]["type"] != "Ok"
            assert test_result, \
                f"Expected request {test_number + 1} to fail, but it succeeded"

@err_logging_decorator
def internal_check_no_access_to_control_interface(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for test_result in output:
        assert test_result["result"]["type"] == "NoApi", "Expect type is different to NoApi"

@err_logging_decorator
def internal_check_control_interface_closed(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for test_result in output:
        assert test_result["result"]["type"] == "ConnectionClosed", "Expect type is different to ConnectionClosed"

@err_logging_decorator
def internal_check_if_mount_point_has_not_been_generated_for(agent_name, command_result):
    AGENT_NAME = agent_name
    TMP_DIRECTORY = path.join(path.sep, f"tmp/ankaios/{AGENT_NAME}_io")
    WORKLOAD_STATES_LEVEL = "workloadStates"
    CONTROL_INTERFACE_SUBFOLDER = "control_interface"
    SHA_ENCODING_LEVEL = 0

    json_result = json_to_dict(command_result.stdout)

    workloads_list = list(json_result[WORKLOAD_STATES_LEVEL][AGENT_NAME].keys())
    for idx, _ in enumerate(workloads_list):
        state_sha_encoding = list(json_result[WORKLOAD_STATES_LEVEL][AGENT_NAME][workloads_list[idx]].keys())[SHA_ENCODING_LEVEL]

        workload_folder_name = f"{workloads_list[idx]}.{state_sha_encoding}"
        control_interface_path = path.join(TMP_DIRECTORY, workload_folder_name, CONTROL_INTERFACE_SUBFOLDER)

        assert not path.exists(control_interface_path), "the mount point has been generated"

@err_logging_decorator
def internal_check_workload_files_exists(complete_state_json, workload_name, agent_name):
    DESIRED_STATE_LEVEL = "desiredState"
    WORKLOADS_LEVEL = "workloads"
    WORKLOAD_STATES_LEVEL = "workloadStates"
    SHA_ENCODING_LEVEL = 0
    FILES_KEY = "files"
    MOUNT_POINT = "mountPoint"
    ROOT_PATH = "/"
    tmp_directory = path.join(path.sep, f"tmp/ankaios/{agent_name}_io")
    complete_state = json_to_dict(complete_state_json)
    workload_files = complete_state[DESIRED_STATE_LEVEL][WORKLOADS_LEVEL][workload_name][FILES_KEY]
    workload_id = list(complete_state[WORKLOAD_STATES_LEVEL][agent_name][workload_name].keys())[SHA_ENCODING_LEVEL]
    workload_folder_name = f"{workload_name}.{workload_id}"

    assert len(workload_files) > 0, f"empty field 'files' for {workload_name}"

    for file in workload_files:
        relative_mount_point = path.relpath(file[MOUNT_POINT], ROOT_PATH)
        workload_file_host_path = path.join(tmp_directory, workload_folder_name, "files", relative_mount_point)
        assert path.exists(workload_file_host_path), f"the workload file for {workload_name} does not exist"


@err_logging_decorator
def agent_shall_have_tag(state_output: str, agent_name: str, tag_key: str, tag_value: str):
    state = yaml.safe_load(state_output)
    logger.trace(f"State: {state}")

    assert "agents" in state, f"No 'agents' section found in state"
    assert agent_name in state["agents"], f"Agent '{agent_name}' not found in state"

    agent = state["agents"][agent_name]
    assert "tags" in agent, f"Agent '{agent_name}' has no 'tags' field"

    tags = agent["tags"]
    assert tag_key in tags, f"Tag '{tag_key}' not found in agent '{agent_name}' tags. Available tags: {list(tags.keys())}"

    actual_value = tags[tag_key]
    assert actual_value == tag_value, f"Tag '{tag_key}' has value '{actual_value}', expected '{tag_value}'"


@err_logging_decorator
def agent_shall_not_have_tag(state_output: str, agent_name: str, tag_key: str):
    state = yaml.safe_load(state_output)
    logger.trace(f"State: {state}")

    assert "agents" in state, f"No 'agents' section found in state"
    assert agent_name in state["agents"], f"Agent '{agent_name}' not found in state"

    agent = state["agents"][agent_name]

    if "tags" in agent:
        tags = agent["tags"]
        assert tag_key not in tags, f"Tag '{tag_key}' should not exist but was found in agent '{agent_name}' with value '{tags[tag_key]}'"


@err_logging_decorator
def agent_shall_not_exist(state_output: str, agent_name: str):
    state = yaml.safe_load(state_output)
    logger.trace(f"State: {state}")

    if "agents" in state:
        assert agent_name not in state["agents"], f"Agent '{agent_name}' should not exist but was found in state"


def get_instance_name_from_ankaios_workload_states(workload_states: str, workload_name: str) -> str:
    workload_states_dict = json_to_dict(workload_states)

    curr_workload_states = workload_states_dict.get("workloadStates")
    if not curr_workload_states:
        logger.error("'workloadStates' key not found in the provided JSON")
        return ""

    for agent, workloads in curr_workload_states.items():
        if not workloads:
            logger.error(f"No workloads found for agent '{agent}'")
            return ""

        for wl_name, state in workloads.items():
            if wl_name == workload_name:
                hash_key = next(iter(state.keys()), None)
                if not hash_key:
                    logger.error(f"No hash key found for workload '{workload_name}' on agent '{agent}'")
                    return ""
                return f"{workload_name}.{hash_key}.{agent}"

    logger.warning(f"Workload '{workload_name}' not found in workload states")
    return ""

@err_logging_decorator
def event_output_shall_be_valid_yaml_format(output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"
    with open(output_file, 'r') as f:
        content = f.read()

    documents = [doc.strip() for doc in content.strip().split('---') if doc.strip()]

    assert len(documents) > 0, "No YAML documents found in event output"

    for doc in documents:
        try:
            yaml.safe_load(doc)
        except yaml.YAMLError as e:
            raise AssertionError(f"Invalid YAML format in event output: {e}")

    logger.trace(f"Event output contains {len(documents)} valid YAML documents")


@err_logging_decorator
def event_output_shall_be_valid_json_format(output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"
    with open(output_file, 'r') as f:
        content = f.read()

    lines = [line.strip() for line in content.strip().split('\n') if line.strip()]

    assert len(lines) > 0, "No JSON objects found in event output"

    for i, line in enumerate(lines):
        try:
            json.loads(line)
        except json.JSONDecodeError as e:
            raise AssertionError(f"Invalid JSON format in event {i+1}: {e}\nLine: {line[:200]}...")

    logger.trace(f"Event output contains {len(lines)} valid JSONL events")


@err_logging_decorator
def event_output_shall_contain_timestamp(output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"
    with open(output_file, 'r') as f:
        content = f.read()

    assert "timestamp" in content, "No timestamp field found in event output"
    logger.trace("Event output contains timestamp")


@err_logging_decorator
def event_output_shall_contain_timestamp_in_rfc3339_format(output_file: str):
    import re

    assert path.exists(output_file), f"Event output file {output_file} does not exist"
    with open(output_file, 'r') as f:
        content = f.read()

    rfc3339_pattern = r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})'

    assert re.search(rfc3339_pattern, content), \
        "No RFC3339 formatted timestamp found in event output"

    logger.trace("Event output contains RFC3339 formatted timestamp")


@err_logging_decorator
def event_output_shall_contain_only_desiredstate_workloads(output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"
    with open(output_file, 'r') as f:
        content = f.read()

    documents = [doc.strip() for doc in content.strip().split('---') if doc.strip()]

    for doc in documents:
        data = yaml.safe_load(doc)
        if data and ('complete_state' in data or 'completeState' in data):
            complete_state = data.get('complete_state') or data.get('completeState')
            if complete_state:
                desired_state = complete_state.get('desiredState') or complete_state.get('desired_state')
                assert desired_state is not None, "desiredState not found in event output"
                assert 'workloads' in desired_state, "workloads field not found in desiredState"
                logger.trace("Event output contains only desiredState workloads")
                return

    raise AssertionError("Could not verify desiredState.workloads in event output")


@err_logging_decorator
def event_output_shall_contain_at_least_n_events(count: str, output_file: str):
    expected_count = int(count)
    assert path.exists(output_file), f"Event output file {output_file} does not exist"

    with open(output_file, 'r') as f:
        content = f.read()

    content_stripped = content.strip()
    if content_stripped.startswith('---'):
        yaml_docs = [doc.strip() for doc in content_stripped.split('---') if doc.strip()]
        assert len(yaml_docs) >= expected_count, \
            f"Expected at least {expected_count} events, but found {len(yaml_docs)}"
        logger.trace(f"Found at least {expected_count} events (YAML format)")
    else:
        json_lines = [line.strip() for line in content_stripped.split('\n') if line.strip()]
        assert len(json_lines) >= expected_count, \
            f"Expected at least {expected_count} events, but found {len(json_lines)}"
        logger.trace(f"Found at least {expected_count} events (JSONL format)")


@err_logging_decorator
def each_event_in_output_shall_contain_timestamp(output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"

    with open(output_file, 'r') as f:
        content = f.read()

    content_stripped = content.strip()
    if content_stripped.startswith('---'):
        yaml_docs = [doc.strip() for doc in content_stripped.split('---') if doc.strip()]

        for doc in yaml_docs:
            data = yaml.safe_load(doc)
            assert 'timestamp' in data, f"Event missing timestamp field: {doc[:100]}"

        logger.trace("Each event contains timestamp (YAML format)")
    else:
        lines = [line.strip() for line in content_stripped.split('\n') if line.strip()]

        for line in lines:
            data = json.loads(line)
            assert 'timestamp' in data, f"Event missing timestamp field: {line[:100]}"

        logger.trace("Each event contains timestamp (JSONL format)")

@err_logging_decorator
def event_output_shall_contain_workload(workload_name: str, output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"
    with open(output_file, 'r') as f:
        content = f.read()

    assert workload_name in content, f"Workload '{workload_name}' not found in event output"
    logger.trace(f"Found workload '{workload_name}' in event output")

@err_logging_decorator
def event_output_shall_contain_field_name(output_file: str, field_name: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"

    with open(output_file, 'r') as f:
        content = f.read()

    assert field_name in content, \
        f"{field_name} field not found in event output"

    logger.trace(f"Event output contains {field_name}")

@err_logging_decorator
def event_output_shall_contain_altered_fields_with_removed_workloads(output_file: str):
    assert path.exists(output_file), f"Event output file {output_file} does not exist"

    with open(output_file, 'r') as f:
        content = f.read()

    found_removal = False

    content_stripped = content.strip()
    if content_stripped.startswith('---'):
        yaml_docs = [doc.strip() for doc in content_stripped.split('---') if doc.strip()]

        for doc in yaml_docs:
            data = yaml.safe_load(doc)
            if data:
                removed = data.get('removedFields') or data.get('removed_fields', [])
                if removed and len(removed) > 0:
                    found_removal = True
                    break
                if 'altered_fields' in data or 'alteredFields' in data:
                    altered = data.get('altered_fields') or data.get('alteredFields')
                    if altered:
                        removed = altered.get('removed_fields') or altered.get('removedFields', [])
                        if removed and len(removed) > 0:
                            found_removal = True
                            break
    else:
        lines = [line.strip() for line in content_stripped.split('\n') if line.strip()]

        for line in lines:
            data = json.loads(line)
            removed = data.get('removedFields') or data.get('removed_fields', [])
            if removed and len(removed) > 0:
                found_removal = True
                break
            if 'alteredFields' in data or 'altered_fields' in data:
                altered = data.get('alteredFields') or data.get('altered_fields')
                if altered:
                    removed = altered.get('removedFields') or altered.get('removed_fields', [])
                    if removed and len(removed) > 0:
                        found_removal = True
                        break

    assert found_removal, "No removed workloads found in altered_fields"
    logger.trace("Event output contains altered fields with removed workloads")


@err_logging_decorator
def listen_for_events_with_timeout(field_mask: str, log_output_file: str, ank_bin_dir: str=None, timeout: str="10", insecure: bool=False):
    timeout_float = float(timeout)
    def listen_with_timeout(field_mask: str, event_buffer: list, log_output_file: str, timeout: float, ank_bin_dir: str=None, insecure: bool=False):
        # separate log file for the event process otherwise corrupted robot framework output.xml may occur because of concurrent writes
        log_file_handle = open(log_output_file, 'w+', buffering=1)  # Line buffering
        log_file_handle.write(f"Listening for events with timeout of {timeout_float} seconds\n")

        if ank_bin_dir is None:
            ank_bin_dir = environ.get('ANK_BIN_DIR', '.')

        ank_path = path.join(ank_bin_dir, 'ank')


        if insecure:
            cmd = [ank_path, '--insecure', 'get', 'events', '-o', 'json']
        else:
            cmd = [ank_path, 'get', 'events', '-o', 'json']
        if field_mask:
            cmd.append(field_mask)

        process = None

        try:
            log_file_handle.write(f"Starting event listener: {' '.join(cmd)}\n")
            process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1
            )

            start_time = get_time_secs()
            first_event_received = False

            while (get_time_secs() - start_time) < timeout:

                EVENTS_RECEIVED.clear()
                new_line = process.stdout.readline()  # blocks until a line or EOF

                if new_line:
                    try:
                        event = json.loads(new_line)
                        first_event_received = True
                        log_file_handle.write(f"Received event: {event}\n")
                        event_buffer.append(event)
                        EVENTS_RECEIVED.set()
                    except json.JSONDecodeError as e:
                        log_file_handle.write(f"Failed to parse event line: '{new_line}' with error: {e}\n")
                        pass
                else:
                    stderr = process.stderr.read()
                    if "could not connect to ankaios server" in stderr.lower():
                        # when the server is not yet available, restart the process until the timeout is reached
                        process.terminate()
                        process.wait(timeout=2)

                        process = subprocess.Popen(
                            cmd,
                            stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE,
                            text=True,
                            bufsize=1
                        )
                        log_file_handle.write("Restarted event listener process. This might happen and does not indicate a test failure.\n")
                    else:
                        log_file_handle.write(f"Event listener process terminated: {stderr}\n")
                        break


            log_file_handle.write(f"Event waiting timed out after {timeout}s, first_event_received={first_event_received}\n")
            if not first_event_received:
                log_file_handle.write(f"No events received from 'ank get events' after {timeout}s - possible connection or subscription issue\n")
            if process:
                process.send_signal(signal.SIGTERM)
                process.wait(timeout=2)

        except Exception as e:
            log_file_handle.write(f"Error while listening for events: {e}\n")
        finally:
            if log_file_handle:
                log_file_handle.close()
            if process:
                try:
                    process.terminate()
                    process.wait(timeout=2)
                except:
                    pass

    global EVENT_PROCESS
    global EVENT_BUFFER
    manager = mp.Manager()
    EVENT_BUFFER = manager.list()
    EVENT_PROCESS = mp.Process(target=listen_with_timeout, args=(field_mask, EVENT_BUFFER, log_output_file, timeout_float, ank_bin_dir, insecure))
    EVENT_PROCESS.start()

@err_logging_decorator
def unsubscribe_from_events():
    logger.trace(f"Unsubscribing from events.")
    global EVENT_PROCESS
    global EVENT_BUFFER
    if EVENT_PROCESS:
        EVENT_PROCESS.kill()
        EVENT_PROCESS.join()
    logger.trace(f"Event listener process terminated.")
    del EVENT_BUFFER[:]

def workload_has_execution_state(workload_name: str, agent_name: str, expected_state: str, timeout: str="10"):
    """
    Condition fulfilled if:
    - specified agent is connected
    - specified workload of the agent has reached the expected execution state
    """
    global EVENTS_RECEIVED
    global EVENT_BUFFER
    timeout_float = float(timeout)
    start_time = get_time_secs()

    while (get_time_secs() - start_time) < float(timeout_float):
        for event in EVENT_BUFFER:
            complete_state: dict = event.get('completeState', {})
            workload_states: dict = complete_state.get('workloadStates', {})
            agent_workloads = workload_states.get(agent_name, {})
            workload = agent_workloads.get(workload_name, {})
            logger.trace(f"Complete state: {complete_state}")
            logger.trace(f"Agent workloads: {agent_workloads}")

            # special handling for Removed state (no entry in state of event means removed)
            if expected_state == "Removed" and not workload:
                removed_fields = event.get('removedFields', [])
                for field in removed_fields:
                    if field == f"workloadStates.{agent_name}.{workload_name}" or field == f"workloadStates.{agent_name}":
                        logger.trace(f"Workload '{workload_name}' has been removed from agent '{agent_name}' indicated by removed field: '{field}'.")
                        return True

            for _, instance_state in workload.items():
                state = instance_state.get('state', '')
                sub_state = instance_state.get('subState', '')
                current_state = f"{state}({sub_state})" if sub_state else state

                logger.trace(f"Current state of workload '{workload_name}': {current_state}")

                if current_state == expected_state:
                    return True

        remaining_time = timeout_float - (get_time_secs() - start_time)
        if remaining_time > 0:
            EVENTS_RECEIVED.wait(timeout=remaining_time)
        else: break

    return False
