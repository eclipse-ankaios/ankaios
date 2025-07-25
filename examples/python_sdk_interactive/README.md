# python_sdk_interactive example

The purpose of this example is to be used for manual interaction with the Control Interface. The container includes the python sdk and runs an endless sleep to ensure that the workloads stays in a running state.

## Starting the workload

### Starting with the latest version of the Python SDK

For the intended production use, with the python_sdk downloaded from pypi, just run:

``` bash
./run_example.sh python_sdk_interactive
```

### Starting with a development branch of the Python SDK

For testing the python SDK, you can specify a distinct branch that the SDK should be installed from. This can be done by setting the `PYTHON_SDK_BRANCH` environment variable in the script:

``` bash
./run_example.sh python_sdk_interactive --env PYTHON_SDK_BRANCH=python_sdk_branch
```

This will also automatically use the proto files from the local ankaios repository.

For more advanced configurations, you can modify the Dockerfile or directly run the `../tools/setup_python_sdk.sh` script with the required arguments.

## Manual interaction with the Control Interface

Once the workload is up and running, start an interactive python interpreter inside the container:

```bash
podman exec -it $(podman ps -a | grep python_sdk_interactive | awk '{print $1}') python3
```

In the interpreter you can now directly connect to Ankaios via the Control Interface, e.g.:

```python
from ankaios_sdk import Ankaios, AnkaiosException, WorkloadInstanceName
ank=Ankaios()

# get the workload states as dict and print them
print(ank.get_workload_states().get_as_dict())
```

Feel free to try out other commands supported by the [Python SDK](https://pypi.org/project/ankaios-sdk/) to learn how the Control Interface works.
