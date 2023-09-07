import pytest
from utils.common import write_yaml
from utils.commands import run_command
from utils.formatters import yaml_to_dict
import os




@pytest.mark.parametrize('ank_server', ['./startupState.yaml'], indirect=True)
@pytest.mark.parametrize('ank_agent', [['--name=agent_A']], indirect=True)
@pytest.mark.parametrize('ank_cli', [""], indirect=True)
def test_update_workload(ank_server, ank_agent, ank_cli):
    # check startup config
    status, content = ank_cli.run("get state", format_func=yaml_to_dict)
    assert content["currentState"]["workloads"]["nginx"]["runtimeConfig"]["ports"][0]["hostPort"] == 8081

    # curl to nginx service 8081
    assert run_command("curl localhost:8081", wait=True, max_retires=100).returncode == 0

    # update workload => hostport to 8082
    content["currentState"]["workloads"]["nginx"]["runtimeConfig"]["ports"][0]["hostPort"] = 8082
    current_path = os.path.dirname(os.path.abspath(__file__))
    new_state_file = os.path.join(current_path, "newState.yaml")
    write_yaml(content, new_state_file)

    status, content = ank_cli.run(f"set state currentState.workloads.nginx -f {new_state_file}")
    assert status

    # curl to nginx service 8082
    assert run_command("curl localhost:8082", wait=True, max_retires=100).returncode == 0