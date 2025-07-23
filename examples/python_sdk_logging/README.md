# python_sdk_logging example

This example uses the python sdk to read the logs of another workload (in this case called the screamer).

## Running

For the intended production use, with the python_sdk downloaded from pypi, this should be enough:

``` bash
./run_example.sh python_sdk
```

## Development

For testing the python SDK, you can specify a specific branch that the SDK should be installed from. This can be done by setting the `PYTHON_SDK_BRANCH` environment variable in the script:

``` bash
./run_example.sh python_sdk --env PYTHON_SDK_BRANCH=python_sdk_branch
```

This will also automatically use the proto files from the local ankaios repository.

For more advanced configurations, you can modify the Dockerfile and changing directly the arguments for the `setup_python_sdk.sh` script.
