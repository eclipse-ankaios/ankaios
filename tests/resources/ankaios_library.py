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
from robot.api import logger
from robot.libraries.BuiltIn import BuiltIn
from tempfile import TemporaryDirectory
from os import path, popen
import shutil

import re
list_pattern = re.compile("^[\"|\']*\[.*\][\"|\']*$")
CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START = "^"
EXPLICIT_DOT_IN_REGEX = "\\\\."

def run_command(command, timeout=3):
    try:
        return subprocess.run(command, timeout=timeout, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
    except Exception as e:
        logger.error(f"{e}")
        return None

def table_to_list(raw):
    raw = raw.strip()
    splitted = raw.split('\n')

    # Skip all lines before the table
    header = ""
    while splitted and ("WORKLOAD NAME" and "NAME") not in header:
        header = splitted.pop(0)

    columns = [(x.group(0).strip(), x.start(), x.end()) for x in re.finditer(r'(([^\s]+\s?)+\s*)', header.replace('\x1b[1G\x1b[1G', ''))]
    logger.trace("columns: {}".format(columns))
    table = []
    for row in splitted:
        table_row = {}
        for c in columns:
            table_row[c[0]] = row[c[1]:c[2]].strip()
        table.append(table_row)

    logger.trace(table)

    return table

def table_to_dict(input_list, key):
    out_dict = {}
    for item in input_list:
        out_dict[item[key]] = item
        del item[key]
    logger.trace(out_dict)
    return out_dict

def get_agent_dict(table_dict, agent_name):
    agent_dict = table_dict.get(agent_name)
    assert agent_dict, f"Agent {agent_name} does not provide availabe resources information"
    logger.trace(agent_dict)
    return agent_dict


def to_x_dot_y_format(x, y):
    return f"{x}.{y}"

def to_x_dot_y_dot_z_format(x, y, z):
    return f"{x}.{y}.{z}"

def remove_hash_from_workload_name(wn_hash_an_string):
    items = wn_hash_an_string.split('.')
    if len(items) == 3:
        return f"{items[0]}.{items[2]}"
    elif len(items) == 4:
        return f"{items[0]}.{items[2]}.{items[3]}"
    return items[0]

def get_time_secs():
    return time.time()

def get_workload_names_from_podman():
    res = run_command('podman ps -a --format "{{.Names}}"')
    raw = res.stdout.strip()
    raw_wln = raw.split('\n')
    workload_names = list(map(remove_hash_from_workload_name, raw_wln))
    logger.trace(workload_names)
    return workload_names

def get_volume_names_from_podman():
    res = run_command('podman volume ls --format "{{.Name}}"')
    raw = res.stdout.strip()
    raw_vols = raw.split('\n')
    vol_names = list(map(remove_hash_from_workload_name, raw_vols))
    logger.trace(vol_names)
    return vol_names

def get_volume_name_by_workload_name_from_podman(workload_name):
    res = run_command('podman volume ls --format "{{{{.Name}}}}" --filter name={}{}{}.*{}pods'.format(CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START, workload_name, EXPLICIT_DOT_IN_REGEX, EXPLICIT_DOT_IN_REGEX))
    volume_name = res.stdout.strip()
    logger.trace(volume_name)
    return volume_name

def get_container_id_and_name_by_workload_name_from_podman(workload_name):
    res = run_command('podman ps -a --no-trunc --format="{{{{.ID}}}} {{{{.Names}}}}" --filter=name={}{}{}.*'.format(CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START, workload_name, EXPLICIT_DOT_IN_REGEX))
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

    return container_id, container_name

def get_pod_id_by_pod_name_from_podman(pod_name):
    res = run_command('podman pod ls --no-trunc --format="{{{{.ID}}}}" --filter=name={}{}'.format(CHAR_TO_ANCHOR_REGEX_PATTERN_TO_START, pod_name))
    raw = res.stdout.strip()
    pod_ids = raw.split('\n')
    logger.trace(pod_ids)
    amount_of_rows = len(pod_ids)
    expected_amount_of_rows = 1
    assert amount_of_rows == expected_amount_of_rows, \
        f"Expected {expected_amount_of_rows} row for pod name {pod_name} but found {amount_of_rows} rows"

    pod_id = pod_ids[0]
    return pod_id

def wait_for_initial_execution_state(command, agent_name, timeout=10, next_try_in_sec=0.25):
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
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return list()

def workload_with_execution_state(table, workload_name, expected_state):
    logger.trace(table)
    if table and any([row["EXECUTION STATE"].strip() == expected_state for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name, table)]):
        return table
    return list()

def wait_for_execution_state(command, workload_name, agent_name, expected_state, timeout=10, next_try_in_sec=0.25):
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

def wait_for_workload_removal(command, workload_name, expected_agent_name, timeout=10, next_try_in_sec=0.25):
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

def replace_key(data, match, func):
    if isinstance(data, dict):
        for k, v in data.items():
            if k == match:
                data[k] = func(v)
            replace_key(data[k], match, func)
    elif isinstance(data, list):
        for item in data:
            replace_key(item, match, func)

def yaml_to_dict(raw):
    y = yaml.safe_load(raw)
    replace_key(y, "runtimeConfig", yaml.safe_load)
    return y

def replace_config(data, filter_path, new_value):
    filter_path = filter_path.split('.')
    filter_iterator = iter(filter_path)
    next_level = data[next(filter_iterator)]
    for level in filter_iterator:
        if filter_path[-1] == level:
            break

        next_level = next_level[level] if isinstance(next_level, dict) else next_level[int(level)]
    next_level[filter_path[-1]] = int(new_value) if new_value.isdigit() else yaml.safe_load(new_value) if list_pattern.match(new_value) else new_value

    return data

def write_yaml(new_yaml, path):
    with open(path,"w+") as file:
        replace_key(new_yaml, "runtimeConfig", yaml.dump)
        yaml.dump(new_yaml, file)

def read_yaml(file_path):
    with open(file_path) as file:
        content = file.read()
        return yaml.safe_load(content)

def json_to_dict(raw):
    json_data = json.loads(raw)
    return json_data

def find_control_interface_test_tag():
    global control_interface_tester_tag
    control_interface_tester_tag = "manual-build-3"

def prepare_test_control_interface_workload():
    global control_interface_workload_config
    global manifest_files_location
    global next_manifest_number
    global control_interface_allow_rules
    global control_interface_deny_rules

    control_interface_workload_config = []
    manifest_files_location = []
    next_manifest_number = 0
    control_interface_allow_rules = []
    control_interface_deny_rules = []

def internal_allow_control_interface(operation, filter_mask):
    filter_mask = filter_mask.replace(" and ", ", ").split(", ")
    control_interface_allow_rules.append({
        "type": "StateRule",
        "operation": internal_control_interface_convert_operation(operation),
        "filterMask": filter_mask
    })

def internal_deny_control_interface(operation, filter_mask):
    filter_mask = filter_mask.replace(" and ", ", ").split(", ")
    control_interface_deny_rules.append({
        "type": "StateRule",
        "operation": internal_control_interface_convert_operation(operation),
        "filterMask": filter_mask
    })

def internal_control_interface_convert_operation(operation):
    operation_lower = operation.lower()
    res = ""
    if "read" in operation_lower:
        res = "Read"
    if "write" in operation_lower:
        res += "Write"

    assert res != "", f"The operation(s) '{operation}' is/are unknown"
    return res

def internal_add_update_state_command(manifest, update_mask):
    global control_interface_workload_config

    update_mask = update_mask.replace(" and ", ", ").split(", ")
    internal_manifest_name = internal_add_to_manifest_list(manifest)
    control_interface_workload_config.append({
        "command": {
            "type": "UpdateState",
            "manifest_file": path.join("/data", internal_manifest_name),
            "update_mask": update_mask
        }
    })

def internal_send_initial_hello(version):
    global control_interface_workload_config
    control_interface_workload_config.append({
        "command": {
            "type": "SendHello",
            "version": version
        }
    })

def internal_add_to_manifest_list(manifest_file):
    global next_manifest_number
    global manifest_files_location

    internal_name = "manifest_{}.yaml".format(next_manifest_number)
    manifest_files_location.append({"file_path": manifest_file, "internal_name": internal_name})
    next_manifest_number += 1

    return internal_name

def internal_add_get_state_command(field_mask):
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


def create_control_interface_config_for_test():
    tmp = TemporaryDirectory()
    assert path.isdir(tmp.name) and path.exists(tmp.name), f"The temporary directory at {tmp.name} has not been created"
    write_yaml(new_yaml=control_interface_workload_config, path=path.join(tmp.name, "commands.yaml"))

    for manifest in manifest_files_location:
        shutil.copy(manifest["file_path"], path.join(tmp.name, manifest["internal_name"]))

    configs_dir = BuiltIn().get_variable_value("${CONFIGS_DIR}")

    with open(path.join(configs_dir, "control_interface_workload.yaml.template")) as startup_config_template, open(path.join(tmp.name, "startup_config.yaml"), "w") as startup_config:
        template_content = startup_config_template.read()
        content = template_content.format(temp_data_dir=tmp.name,
                                          allow_rules=json.dumps(control_interface_allow_rules),
                                          deny_rules=json.dumps(control_interface_deny_rules),
                                          control_interface_tester_tag=control_interface_tester_tag)
        startup_config.write(content)
    return tmp

def internal_check_all_control_interface_requests_succeeded(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for test_number,test_result in enumerate(output):
        test_result = test_result["result"]["value"]["type"] == "Ok"
        assert test_result, \
            f"Expected request {test_number + 1} to succeed, but it failed"

def internal_check_all_control_interface_requests_failed(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for test_number,test_result in enumerate(output):
        if test_result["result"]["type"] != "SendHelloResult":
            test_result = test_result["result"]["value"]["type"] != "Ok"
            assert test_result, \
                f"Expected request {test_number + 1} to fail, but it succeeded"

def internal_check_no_access_to_control_interface(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for test_result in output:
        assert test_result["result"]["type"] == "NoApi", "Expect type is different to NoApi"

def internal_check_control_interface_closed(tmp_folder):
    output = read_yaml(path.join(tmp_folder, "output.yaml"))
    for test_result in output:
        assert test_result["result"]["type"] == "ConnectionClosed", "Expect type is different to ConnectionClosed"

def empty_keyword():
    pass

def check_if_mount_point_has_not_been_generated_for(agent_name, command_result):
    AGENT_NAME = agent_name
    TMP_DIRECTORY = path.join(path.sep, f"tmp/ankaios/{AGENT_NAME}_io")
    WORKLOAD_STATES_LEVEL = "workloadStates"
    SHA_ENCODING_LEVEL = 0

    json_result = json.loads(command_result.stdout)

    workloads_list = list(json_result[WORKLOAD_STATES_LEVEL][AGENT_NAME].keys())
    for idx, _ in enumerate(workloads_list):
        state_sha_encoding = list(json_result[WORKLOAD_STATES_LEVEL][AGENT_NAME][workloads_list[idx]].keys())[SHA_ENCODING_LEVEL]

        control_interface_name = f"{workloads_list[idx]}.{state_sha_encoding}"
        control_interface_path = path.join(TMP_DIRECTORY, control_interface_name)

        assert not path.exists(control_interface_path), "the mount point has been generated"
