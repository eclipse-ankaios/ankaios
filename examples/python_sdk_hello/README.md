# python_sdk_hello example

This example is used to test the basic functionality of the ankaios sdk. It creates and starts dynamically a workload `dynamic_nginx`, fetches the execution state in different ways and deletes it.

## Starting the workload

### Starting with the latest version of the Python SDK

For the intended production use, with the python_sdk downloaded from pypi, just run:

``` bash
./run_example.sh python_sdk_hello
```

### Starting with a development branch of the Python SDK

For testing the python SDK, you can specify a distinct branch that the SDK should be installed from. This can be done by setting the `PYTHON_SDK_BRANCH` environment variable in the script:

``` bash
./run_example.sh python_sdk_hello --env PYTHON_SDK_BRANCH=python_sdk_branch
```

This will also automatically use the proto files from the local ankaios repository.

For more advanced configurations, you can modify the Dockerfile or directly run the `../tools/setup_python_sdk.sh` script with the required arguments.
