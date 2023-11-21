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

import re
list_pattern = re.compile("^[\"|\']*\[.*\][\"|\']*$")

def run_command(command, timeout=3):
    try:
        return subprocess.run(command, timeout=timeout, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
    except Exception as e:
        logger.error(f"{e}")
        return None

def table_to_list(raw):
    raw = raw.strip()
    splitted = raw.split('\n')
    header = splitted.pop(0)
    columns = []
    next_start_index = 0
    index = header.find("  ", next_start_index)
    while index > -1:
        while index < len(header) and header[index] == ' ':
            index += 1
        
        columns.append(index)
        next_start_index = index + 1
        index = header.find("  ", next_start_index)

    columns.append(len(header))

    table = []
    for row in splitted:
        last_column_index = 0
        table_row = {}
        for column_index in columns:
            table_row[header[last_column_index: column_index].strip()] = row[last_column_index: column_index].strip()
            last_column_index = column_index
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

def get_column_values(list, column_name):
    if column_name in list:
        return map(lambda r: r[column_name], list) 
    else:
        return []

def get_container_ids_by_workload_names(workload_names):
    res = run_command('podman ps -a --format "{{.Names}} {{.ID}}"')
    raw = res.stdout.strip()
    raw_wln_id = raw.split('\n')
    # ["workload_name.hash.agent_name id", ...] -> [(workload_name,id), ...]
    wln_ids = map(lambda t: (t[0].split('.')[0], t[1]), map(lambda s: tuple(s.split(' ')), raw_wln_id))
    wln_id_tuple_list = wln_ids if not workload_names else filter(lambda wln_id_tuple: wln_id_tuple[0] in workload_names, wln_ids)
    logger.trace(wln_id_tuple_list)
    return wln_id_tuple_list

def to_wn_an(workload_name, agent_name):
    return f"{workload_name}.{agent_name}"

def to_vol_an_type(workload_name, agent_name, type):
    return f"{workload_name}.{agent_name}.{type}"

def from_wn_hash_an_to_wn_an(wn_hash_an_string):
    items = wn_hash_an_string.split('.')
    if len(items) == 3:
        return f"{items[0]}.{items[2]}"
    elif len(items) == 4:
        return f"{items[0]}.{items[2]}.{items[3]}"
    return items[0]

def get_workload_names_from_podman():
    res = run_command('podman ps -a --format "{{.Names}}"')
    raw = res.stdout.strip()
    raw_wln = raw.split('\n')
    # ["workload_name.hash.agent_name id", ...] -> [(workload_name,id), ...]
    workload_names = list(map(from_wn_hash_an_to_wn_an, raw_wln))
    logger.trace(workload_names)
    return workload_names

def get_volume_names_from_podman():
    res = run_command('podman volume ls --format "{{.Name}}"')
    raw = res.stdout.strip()
    raw_vols = raw.split('\n')
    # ["workload_name.hash.agent_name id", ...] -> [(workload_name,id), ...]
    vol_names = list(map(from_wn_hash_an_to_wn_an, raw_vols))
    logger.trace(vol_names)
    return vol_names

def wait_for_initial_execution_state(command, agent_name, timeout_ms=10000, next_try_in_sec=1):
        start_time = time.time()
        logger.trace(run_command("ps aux | grep ank").stdout)
        logger.trace(run_command("podman ps -a").stdout)
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        timeout_secs = timeout_ms / 1000
        while (time.time() - start_time) < timeout_secs:
            if table and all([len(row["EXECUTION STATE"].strip()) > 0 for row in filter(lambda r: r["AGENT"] == agent_name, table)]):
                return True

            time.sleep(next_try_in_sec)
            logger.trace(run_command("ps aux | grep ank").stdout)
            logger.trace(run_command("podman ps -a").stdout)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return False

def wait_for_execution_state(command, workload_name, expected_state, timeout_ms=10000, next_try_in_sec=1):
        start_time = time.time()
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        timeout_secs = timeout_ms / 1000
        while (time.time() - start_time) < timeout_secs:
            if table and any([row["EXECUTION STATE"].strip() == expected_state for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name, table)]):
                return True

            time.sleep(next_try_in_sec)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return False

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

def write_yaml(new_yaml: dict, path):
    with open(path,"w+") as file:
        replace_key(new_yaml, "runtimeConfig", yaml.dump)
        yaml.dump(new_yaml, file)

def json_to_dict(raw):
    json_data = json.loads(raw)
    return json_data

def check_podman_kube_volumes_gone(workload_name, agent_name, timeout=500, next_try_in_sec=100):
    start_time = time.time()
    command = "podman volume ls --format=json"
    regex_str = f"^{workload_name}.\\w+.{agent_name}.(config|pods)$"
    reg_exp = re.compile(regex_str)
    while True:
        res = run_command(command)
        dict = json_to_dict(res.stdout if res else "")
        logger.trace(dict)
        still_matching = False
        for value in dict:
            if "Name" in value:
                logger.trace("checking: " + value["Name"])
                if reg_exp.match(value["Name"]):
                    logger.trace("still matching")
                    still_matching = True
                    break

        if not still_matching:
            return True
        time.sleep(next_try_in_sec / 1000)

        # emulate a do while to support 0 ms timeouts
        if (time.time() - start_time) < timeout:
            break
    return False
