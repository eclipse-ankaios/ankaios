# Ankaios control interface examples

The [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) allows workload developers to easily integrate the communication between the Ankaios system and their applications.

This subfolder contains examples in different programming languages showing how to use [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) within a workload managed by Ankaios.

The intention of the examples is to give a simple introduction in various programming languages about the usage of the control interface in self-developed applications.

All examples share the same behavior regardless of the programming language and do the following:

1. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) to start a new workload  (named `dynamic_workload`) which was not part of the initial startup configuration.
2. Every 5 seconds request the workload states from the Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) and output them to the console.

You can track the execution state of the dynamically added workload on the console and see when the workload `dynamic_nginx` is up and running (execution state: EXEC_RUNNING).

Every subfolder represents an example for a specific programming language. Feel free to try them out by running the steps described below.

**Note:** The examples are simplified to focus on the usage of the control interface and designed for easy readability. They are not optimized for production usage.

## How to run the examples?

1. Install the latest release [here](https://eclipse-ankaios.github.io/ankaios/main/usage/installation/) or build Ankaios inside the [devcontainer](../.devcontainer/Dockerfile).
2. Build and run an example workload:

   ```shell
   ./run_example.sh <example_subfolder>
   ```

   If the Ankaios executables are not inside the default path mentioned in the [Installation instructions](https://eclipse-ankaios.github.io/ankaios/main/usage/installation/), you can specify an alternative Ankaios executable path like the following:

   ```shell
   ANK_BIN_DIR=/absolute/path/to/ankaios/executables ./run_example.sh <example_subfolder>
   ```

   In case you get errors like `DNS lookup error` your are probably within a VPN that restricts access to some DNS servers.
   To workaround that [problem caused by buildah](https://github.com/containers/buildah/issues/3806) you need to specify a DNS server that should be used like:

   ```shell
   ./run_example.sh <example_subfolder> --dns=<IP address of DNS server>
   ```

3. Open an additional terminal in the dev container and run the following shell command to see the logs of the example workload:

   ```shell
   podman logs -f $(podman ps -a | grep control_interface | awk '{print $1}')
   ```

## Ankaios logs

Run the following command to see the Ankaios server logs:

   ```shell
   tail -f /tmp/ankaios-server.log
   ```

Run the following command to see the Ankaios agent logs:

   ```shell
   tail -f /tmp/ankaios-agent_A.log
   ```
