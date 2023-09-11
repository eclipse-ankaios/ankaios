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
    index = 0
    while index < (len(header) - 1):
        while index < len(header) - 1 and not (header[index] == ' ' and header[index + 1] == ' '):
            index += 1
        while index < len(header) and header[index] == ' ':
            index += 1
        
        columns.append(index)
        index += 1

    table = []
    for row in splitted:
        last_column_index = 0
        table_row = {}
        for column_index in columns:
            table_row[header[last_column_index: column_index].strip()] = row[last_column_index: column_index].strip()
            last_column_index = column_index
        table.append(table_row)

    return table

def wait_for_initial_execution_state(command, agent_name, timeout=10):
        start_time = time.time()
        print("exec run")
        res = run_command(command)
        table = table_to_list(res.stdout if res else "")
        while (time.time() - start_time) < timeout:
            if table and all([len(row["EXECUTION STATE"].strip()) > 0 for row in filter(lambda r: r["AGENT"] == agent_name, table)]):
                return True

            print("exec run")
            time.sleep(1)
            res = run_command(command)
            table = table_to_list(res.stdout if res else "")
        print("timeout!!!")
        return False


