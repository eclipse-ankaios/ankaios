# Ankaios examples

This subfolder contains examples in different programming languages showing how to use the [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) from a workload managed by Ankaios and how to achieve different goals with the workload.

The intention of the examples is to give workload developers a simple introduction of the control interface and to the SDKs and with this, to allow an easy access to the Ankaios system to their applications.

## Control Interface examples

All these examples share the same behavior regardless of the programming language and do the following:

1. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) to start a new workload  (named `dynamic_workload`) which was not part of the initial startup configuration.
2. Every 5 seconds request the workload states from the Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) and output them to the console.

You can track the execution state of the dynamically added workload on the console and see when the workload `dynamic_nginx` is up and running (execution state: EXEC_RUNNING).

Every subfolder of the control interface examples represents one for a specific programming language. Feel free to try them out by running the steps described below.

**Note:** The examples are simplified to focus on the usage of the control interface and designed for easy readability. They are not optimized for production usage.

## Other examples

### Python SDK examples

This examples use the [Python SDK](https://pypi.org/project/ankaios-sdk/) to connect to the Ankaios cluster. They are as follow:

- python_sdk_hello: The basic functionality of starting a workload, waiting for it to reach a certain state and then deleting it.
- python_sdk_interactive: example starts a container that sleeps so that the user can connect and manually run python commands.
- python_sdk_logging: example reads the logs of another workload.

### Rust SDK examples

This examples use the [Rust SDK](https://crates.io/crates/ankaios_sdk) to connect to the Ankaios cluster. They are as follow:

- rust_sdk_hello: The basic functionality of starting a workload, waiting for it to reach a certain state and then deleting it.
- rust_sdk_logging: example reads the logs of another workload.

## How to run the examples?

1. Build Ankaios inside the [devcontainer](../.devcontainer/Dockerfile):

    ```shell
    cargo build --release
    ```

1. Export the path to the Ankaios executables to make them reachable to the examples scripts:

    ```shell
    export ANK_BIN_DIR=/workspaces/ankaios/target/x86_64-unknown-linux-musl/release
    ```

1. Run an example including building the workload container:

    ```shell
    ./run_example.sh <example_subfolder>
    ```

    In case you get errors like `DNS lookup error`, your are probably within a VPN that restricts access to some DNS servers.
    To workaround that [problem caused by buildah](https://github.com/containers/buildah/issues/3806) you need to specify a DNS server that should be used like:

    ```shell
    ./run_example.sh <example_subfolder> --dns=<IP address of DNS server>
    ```

1. Get logs from the running workload:

   ```shell
   ank -k logs -f <example_subfolder>
   ```

**Note:** The examples are always kept in sync with the Ankaios repository and not with the Ankaios releases.

If you are using the source of an Ankaios release and you want to test against the official executables, you could alternatively install the binaries from that release using the [Installation instructions](https://eclipse-ankaios.github.io/ankaios/main/usage/installation/) and can skip exporting the ANK_BIN_DIR environment variable.

## Ankaios logs

Run the following command to see the Ankaios server logs:

   ```shell
   tail -f /tmp/ankaios-server.log
   ```

Run the following command to see the Ankaios agent logs:

   ```shell
   tail -f /tmp/ankaios-agent_A.log
   ```

## How to clean up the system

It is possible to run multiple examples in the same time. This will start the Ankaios server and agent only once and apply
the examples on top of each other. To stop all examples and cleanup the system, run the following:

  ```shell
  ./cleanup.sh
  ```
