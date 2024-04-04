# Ankaios control interface examples

This subfolder contains examples in different programming languages showing how to use the [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) from a workload managed by Ankaios.

The intention of the examples is to give workload developers a simple introduction of the control interface and with this allow an easy access to the Ankaios system to their applications.

All examples share the same behavior regardless of the programming language and do the following:

1. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) to start a new workload  (named `dynamic_workload`) which was not part of the initial startup configuration.
2. Every 5 seconds request the workload states from the Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) and output them to the console.

You can track the execution state of the dynamically added workload on the console and see when the workload `dynamic_nginx` is up and running (execution state: EXEC_RUNNING).

Every subfolder represents an example for a specific programming language. Feel free to try them out by running the steps described below.

**Note:** The examples are simplified to focus on the usage of the control interface and designed for easy readability. They are not optimized for production usage.

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

1. Open an additional terminal in the dev container and run the following shell command to see the logs of the example workload:

   ```shell
   podman logs -f $(podman ps -a | grep control_interface | awk '{print $1}')
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

## How to clean up after running an example

If you want to run another example (or to just stop the one currently running), you need to stop Ankaios including all workloads.
To reduce the overhead of these actions we've provided a script that takes care of the clean up for you:

  ```shell
  ./shutdown_example.sh
  ```
