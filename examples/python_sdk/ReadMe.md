# python_sdk example

This example uses the python sdk to read the logs of another workload (in this case called the screamer).

## Running

For the intended production use, with the python_sdk downloaded from pypi, this should be enough:

``` bash
./run_example.sh python_sdk --manifest-file logging_manifest.yaml
```

The manifest file must be specified to container the additional workload and to give access rights to the python sdk to read the logs.

## Development

For testing the python SDK, you can specify a specific branch that the SDK should be installed from. This can be done by setting the `SDK_BRANCH` enviroment variable in the script:

``` bash
./run_example.sh python_sdk --manifest-file logging_manifest.yaml --env SDK_BRANCH=python_sdk_branch
```

This will also automatically use the proto files from the local ankaios repository.

For more advanced configurations, you can modify the Dockerfile and changing directly the arguments for the `setup_python_sdk.sh` script.
