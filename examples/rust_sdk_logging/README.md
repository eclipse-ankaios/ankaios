# rust_sdk_logging example

This example uses the rust sdk to read the logs of another workload (in this case called the screamer).

## Running

For the intended production use, with the rust_sdk downloaded from crates, this should be enough:

``` bash
./run_example.sh rust_sdk_logging
```

## Development

For testing the rust SDK, you can specify a specific branch that the SDK should be installed from. This can be done by setting the `RUST_SDK_BRANCH` environment variable in the script:

``` bash
./run_example.sh rust_sdk_logging --env RUST_SDK_BRANCH=rust_sdk_branch
```

For more advanced configurations, you can modify the Dockerfile or directly the `setup_rust_sdk.sh` script with the required arguments.
