import pytest
import subprocess
import json
from pathlib import Path
import os, sys
from utils.commands import AnkCommand
from xprocess import ProcessStarter
from utils.precondition import Precondition

def ank_bin_dir():
    ank_bin_path = os.environ["ANK_BIN_DIR"]
    if ank_bin_path.endswith("/"):
        return ank_bin_path
    
    return ank_bin_path + "/"

@pytest.fixture()
def precondition(request):
    return Precondition(AnkCommand(ank_bin_dir()))


@pytest.fixture()
def ank_cli(request):
    return AnkCommand(ank_bin_dir())

@pytest.fixture
def ank_server(xprocess, request):
    startup_state_path = Path(Path(request.node.fspath).parent) / request.param

    class Starter(ProcessStarter):
        # startup pattern for loglines to listen, match all
        pattern = "Waiting for agent"

        # startup timeout 
        timeout = 3
        
        # clean up for upon interruptions
        terminate_on_interrupt = True

        # command to start process
        args = [f'{ank_bin_dir()}ank-server', '--startup-config', startup_state_path]

    # ensure process is running and return its logfile
    pid, logfile = xprocess.ensure("ank_server", Starter)

    yield

    # clean up whole process tree afterwards
    xprocess.getinfo("ank_server").terminate()

    # cleanup podman
    # try:
    #     res = subprocess.run("podman rm -a -f", timeout=10, shell=True, executable='/bin/bash', check=True, stdout=subprocess.DEVNULL)
    # except Exception as e:
    #     print(f"Error: could not cleanup podman after test\nreason: {e}.", file=sys.stderr)
    #     raise e

    # new cleanup podman
    try:
        res = subprocess.run("podman ps --all --format=json", timeout=5, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
        containers = json.loads(res.stdout)
        ids = [container['Id'] for container in containers]
        ids_string = " ".join(ids)
        res = subprocess.run(f"podman stop {ids_string}", timeout=30, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
        res = subprocess.run(f"podman rm {ids_string}", timeout=30, shell=True, executable='/bin/bash', check=True, capture_output=True, text=True)
    except Exception as e:
        print(f"Error: could not cleanup podman after test\nreason: {e}.", file=sys.stderr)
        raise e


@pytest.fixture
def ank_agent(xprocess, request):

    xprocesses = []

    for agent_name in request.param:
        class Starter(ProcessStarter):
            # startup pattern for loglines to listen, match all
            pattern = "INFO.*state to"

            # startup timeout 
            timeout = 5
            
            # clean up for upon interruptions
            terminate_on_interrupt = True

            # command to start process
            args = [f'{ank_bin_dir()}ank-agent', agent_name]

        # ensure process is running and return its logfile
        new_process_name = f"ank_agent_{agent_name.split('=')[1]}"
        pid, logfile = xprocess.ensure(new_process_name, Starter)
        xprocesses.append(new_process_name)

    yield

    for proc_name in xprocesses:
        xprocess.getinfo(proc_name).terminate()
