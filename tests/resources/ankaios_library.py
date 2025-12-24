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
from robot.api import logger
from robot.libraries.BuiltIn import BuiltIn
from tempfile import TemporaryDirectory
from os import path
from typing import Union
import shutil
import tomllib


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
    res = run_command(command_str)
    if res.returncode != 0:
        logger.warning(f"Command '{runtime_cli} ps' failed with return code {res.returncode}. Error: '{res.stderr.strip()}' Retrying operation... ")
        res = run_command(command_str)
    assert res.returncode == 0, f"Command '{runtime_cli} ps' failed with return code {res.returncode}. Error: {res.stderr.strip()}"
    raw = res.stdout.strip()
    raw_wln = raw.split('\n')
    container_ids_and_names = list(map(lambda x: x.split(' '), raw_wln)) # 2-dim [[id,name],[id,name],...]
    logger.trace(container_ids_and_names)
    amount_of_rows = len(container_ids_and_names)
    expected_amount_of_rows = 1
    assert amount_of_rows == expected_amount_of_rows, \
        f"Expected {expected_amount_of_rows} row for workload name {workload_name} but found {amount_of_rows} rows"
    amount_of_columns = len(container_ids_and_names[0])
    expected_amount_of_columns = 2
    if amount_of_columns < expected_amount_of_columns:
        return "", ""

    container_id = container_ids_and_names[0][0]
    container_name = container_ids_and_names[0][1]

    logger.trace(f"Container ID: {container_id}, Container Name: {container_name}")
    assert container_id and container_name, \
        f"Container ID or name is empty for workload name {workload_name}. "

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


def wait_for_initial_execution_state(command: str, agent_name: str, timeout: float=10, next_try_in_sec: float=0.25):
        start_time = get_time_secs()
        logger.trace(run_command("ps aux | grep ank").stdout)
        logger.trace(run_command("podman ps -a").stdout)
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (get_time_secs() - start_time) < timeout:
            if table and all([(len(row["EXECUTION STATE"].strip()) > 0 and row["EXECUTION STATE"].strip() != "Pending(Initial)") for row in filter(lambda r: r["AGENT"] == agent_name, table)]):
                return table

            time.sleep(next_try_in_sec)
            logger.trace(run_command("ps aux | grep ank").stdout)
            logger.trace(run_command("podman ps -a").stdout)
            res = run_command(command)
            logger.trace(res)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return list()


def workload_with_execution_state(table: list, workload_name: str, expected_state: str) -> list:
    logger.trace(table)
    if table and any([row["EXECUTION STATE"].strip() == expected_state for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name, table)]):
        return table
    return list()


def wait_for_execution_state(command: str, workload_name: str, agent_name: str, expected_state: str, timeout: float=10, next_try_in_sec: float=0.25) -> list:
        start_time = get_time_secs()
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (get_time_secs() - start_time) < timeout:
            if table and any([row["EXECUTION STATE"].strip() == expected_state for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name and r["AGENT"] == agent_name, table)]):
                return table

            time.sleep(next_try_in_sec)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
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
    global manifest_files_location
    global next_manifest_number
    global control_interface_allow_rules
    global control_interface_deny_rules
    global logs_requests

    control_interface_workload_config = []
    manifest_files_location = []
    next_manifest_number = 0
    control_interface_allow_rules = []
    control_interface_deny_rules = []
    logs_requests = {}


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


def wait_for_workload_removal(command: str, workload_name: str, expected_agent_name: str, timeout: float=10, next_try_in_sec: float=0.25) -> list:
        start_time = get_time_secs()
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (get_time_secs() - start_time) < timeout:
            if table and any([not expected_agent_name or row["AGENT"].strip() == expected_agent_name for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name, table)]):
                time.sleep(next_try_in_sec)
                res = run_command(command)
                table = table_to_list(res.stdout if res else "")
                logger.trace(table)
            else:
                return list()
        return table


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
def internal_add_get_state_command(field_mask: str):
    global control_interface_workload_config

    field_mask = field_mask.replace(" and ", ", ").split(", ")
    if field_mask == [""]:
        field_mask = []
    control_interface_workload_config.append({
        "command": {
            "type": "GetState",
            "field_mask": field_mask
        }
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
        if test_result["result"]["type"] == "NoCheckNeeded":
            continue
        test_result = test_result["result"]["value"]["type"] == "Ok"
        assert test_result, \
            f"Expected request {test_number + 1} to succeed, but it failed"

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
