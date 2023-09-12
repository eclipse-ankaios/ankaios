import subprocess
import time

def run_command(command, timeout=3):
    try:
        return subprocess.run(command, timeout=timeout, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
    except Exception as e:
        print(f"execption!!! {e}")
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

    return table

def table_to_dict(input_list, key):
    out_dict = {}
    for item in input_list:
        out_dict[item[key]] = item
        del item[key]
    return out_dict

def wait_for_initial_execution_state(command, agent_name, next_try_in_sec=1,timeout=10):
        start_time = time.time()
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        while (time.time() - start_time) < timeout:
            if table and all([len(row["EXECUTION STATE"].strip()) > 0 for row in filter(lambda r: r["AGENT"] == agent_name, table)]):
                return True

            time.sleep(next_try_in_sec)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
        return False


