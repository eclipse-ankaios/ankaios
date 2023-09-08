import pytest
from utils.common import write_yaml
from utils.formatters import table_to_dict


@pytest.mark.parametrize('ank_server', ['./startupState.yaml'], indirect=True)
@pytest.mark.parametrize('ank_agent', [['--name=agent_A', '--name=agent_B']], indirect=True)
@pytest.mark.parametrize('ank_cli', [""], indirect=True)
@pytest.mark.parametrize('precondition', [""], indirect=True)
def test_list_workloads(ank_server, ank_agent, ank_cli, precondition):
    precondition.wait_for_initial_execution_state(timeout=20)

    status, content = ank_cli.run("get workloads", format_func=table_to_dict)

    expected_states = {"nginx": "Running", "api_sample": "Running", "hello1": "Removed", "hello2": "Succeeded", "hello3": "Succeeded"}

    assert content
    for row in content:
        workload = row["WORKLOAD NAME"]
        assert workload in expected_states, "Unexpected workload."
        expected_state = expected_states[workload]
        assert row["EXECUTION STATE"] == expected_state, f"Expected state {expected_state} for {workload}"
