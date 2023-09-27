import subprocess
import time
import yaml
from robot.api import logger

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
    return map(lambda r: r[column_name], list)

def get_container_ids_by_workload_names(workload_names):
    res = run_command('sudo podman ps -a --format "{{.Names}} {{.ID}}"')
    raw = res.stdout.strip()
    raw_wln_id = raw.split('\n')
    # ["workload_name.hash.agent_name id", ...] -> [(workload_name,id), ...]
    wln_ids = map(lambda t: (t[0].split('.')[0], t[1]), map(lambda s: tuple(s.split(' ')), raw_wln_id))
    wln_id_tuple_list = wln_ids if not workload_names else filter(lambda wln_id_tuple: wln_id_tuple[0] in workload_names, wln_ids)
    logger.trace(wln_id_tuple_list)
    return wln_id_tuple_list

def wait_for_initial_execution_state(command, agent_name, timeout=10, next_try_in_sec=1):
        start_time = time.time()
        logger.trace(run_command("sudo ps aux | grep ank").stdout)
        logger.trace(run_command("sudo podman ps -a").stdout)
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (time.time() - start_time) < timeout:
            if table and all([len(row["EXECUTION STATE"].strip()) > 0 for row in filter(lambda r: r["AGENT"] == agent_name, table)]):
                return True

            time.sleep(next_try_in_sec)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
            logger.trace(table)
        return False

def wait_for_execution_state(command, workload_name, expected_state, timeout=10, next_try_in_sec=1):
        start_time = time.time()
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        logger.trace(table)
        while (time.time() - start_time) < timeout:
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
    next_level[filter_path[-1]] = int(new_value) if new_value.isdigit() else new_value
    return data

def write_yaml(new_yaml: dict, path):
    with open(path,"w+") as file:
        replace_key(new_yaml, "runtimeConfig", yaml.dump)
        yaml.dump(new_yaml, file)
