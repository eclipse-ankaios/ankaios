# python_sdk_logging example

This example uses the python sdk to read the logs of another workload (in this case called the screamer).

## Starting the workload

### Starting with the latest version of the Python SDK

For the intended production use, with the python_sdk downloaded from pypi, this should be enough:

``` bash
./run_example.sh python_sdk_logging
```

### Starting with a development branch of the Python SDK

For testing the python SDK, you can specify a distinct branch that the SDK should be installed from. This can be done by setting the `PYTHON_SDK_BRANCH` environment variable in the script:

``` bash
./run_example.sh python_sdk_logging --env PYTHON_SDK_BRANCH=python_sdk_branch
```

This will also automatically use the proto files from the local ankaios repository.

For more advanced configurations, you can modify the Dockerfile or directly run the `../tools/setup_python_sdk.sh` script with the required arguments.

## Observing the log collection

To see how the `python_sdk_logging` collect logs, get the log of the workload via the `ank` CLI:

```bash
ank -k logs -f python_sdk_logging
```
