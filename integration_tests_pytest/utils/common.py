import subprocess
import yaml

def replace_runtime_config(data, match, func):
    if isinstance(data, dict):
        for k, v in data.items():
            if k == match:
                data[k] = func(v)
            replace_runtime_config(data[k], match, func)
    elif isinstance(data, list):
        for item in data:
            replace_runtime_config(item, match, func)

def write_yaml(new_yaml, path):
    with open(path,"w+") as file:
        replace_runtime_config(new_yaml, "runtimeConfig", yaml.dump)
        yaml.dump(new_yaml, file)