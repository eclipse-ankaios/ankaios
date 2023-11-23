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

def wait_for_initial_execution_state(command, agent_name, timeout=10, next_try_in_sec=1):
        start_time = get_time_secs()
        logger.trace(run_command("ps aux | grep ank").stdout)
        logger.trace(run_command("podman ps -a").stdout)
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (get_time_secs() - start_time) < timeout:
            if table and all([len(row["EXECUTION STATE"].strip()) > 0 for row in filter(lambda r: r["AGENT"] == agent_name, table)]):
                return table

            time.sleep(next_try_in_sec)
            logger.trace(run_command("ps aux | grep ank").stdout)
            logger.trace(run_command("podman ps -a").stdout)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return list()

def wait_for_execution_state(command, workload_name, expected_state, timeout=10, next_try_in_sec=1):
        start_time = get_time_secs()
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (get_time_secs() - start_time) < timeout:
            if table and any([row["EXECUTION STATE"].strip() == expected_state for row in filter(lambda r: r["WORKLOAD NAME"] == workload_name, table)]):
                return table

            time.sleep(next_try_in_sec)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return list()

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
