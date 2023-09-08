import yaml
import json
from utils.common import replace_runtime_config

def yaml_to_dict(raw):
    y = yaml.safe_load(raw)
    replace_runtime_config(y, "runtimeConfig", yaml.safe_load)
    return y

def json_to_dict(raw):
    return json.loads(raw)

def table_to_dict(raw):
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