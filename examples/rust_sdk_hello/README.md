# rust_sdk_hello example

This example is used to test the basic functionality of the ankaios sdk. It creates and starts dynamically a workload `dynamic_nginx`, fetches the execution state in different ways and deletes it.

## Starting the workload

### Starting with the latest version of the Rust SDK

For the intended production use, with the rust_sdk downloaded from crates, just run:

``` bash
./run_example.sh rust_sdk_hello
```

### Starting with a development branch of the Rust SDK

For testing the rust SDK, you can specify a distinct branch that the SDK should be installed from. This can be done by setting the `RUST_SDK_BRANCH` environment variable in the script:

``` bash
./run_example.sh rust_sdk_hello --env RUST_SDK_BRANCH=rust_sdk_branch
```

This will also automatically use the proto files from the local ankaios repository.

For more advanced configurations, you can modify the Dockerfile or directly the `setup_rust_sdk.sh` script with the required arguments.
