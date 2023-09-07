import subprocess
import time

def run_command(command: list, timeout=3, wait=False, max_retires=10):
    if not wait:
        return subprocess.run(command, timeout=timeout, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)

    for _ in range(max_retires):
        try:
            result = subprocess.run(command, timeout=timeout, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
            return result
        except Exception:
            time.sleep(0.5)

    return None

def run_until_match(command: list, search_string):
    start_time = time.time()
    
    while (time.time() - start_time) < 10:
        try:
            result = run_command(command)
            if search_string in result.stdout:
                return result
        except Exception:
            pass
        time.sleep(1)

    return None

class AnkCommand:

    def __init__(self, ank_bin_dir):
        self.ANK_BIN_DIR = ank_bin_dir

    def run(self, command_part, format_func=None):
        command = f"{self.ANK_BIN_DIR}ank {command_part}"
        process = subprocess.Popen(command,
                        stdout=subprocess.PIPE, 
                        stderr=subprocess.PIPE, shell=True, executable='/bin/bash')
        stdout, stderr = process.communicate()
        stdout = stdout.decode("utf-8")
        stderr = stderr.decode("utf-8")

        if stderr:
            return False, stderr

        if format_func:
            try:
                return True, format_func(stdout)
            except Exception as e:
                print(e)
                return False, stdout

        return True, stdout
