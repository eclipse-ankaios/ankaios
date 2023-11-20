# Ankaios control interface examples

The [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) allows the workload developers to easily integrate the communication between the Ankaios system and their applications.

This subfolder contains examples, written in different programming languages, about how to use the [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) within a workload managed by Ankaios.

The intention of the examples is to give a simple introduction in various programming languages about the usage of the control interface in self-developed applications.
Furthermore, they shall enable an easy start to develop applications using the control interface by providing a basic development environment (devcontainer) and a running example application.

All examples share the same behavior regardless of the programming language and do the following:

1. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) to start a new workload  (named `dynamic_workload`) dynamically.
2. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) every 30 sec. to get the workload states that are part of the current state of the Ankaios cluster and output them to the console.

You can track the execution state of the dynamically added workload on the console and see when the workload `dynamic_nginx` is up and running (execution state: EXEC_RUNNING).

Every subfolder represents an example for a specific programming language. Feel free to try them out by navigating into the dedicated subfolder and executing the steps explained inside. 

**Note:** The examples are simplified to focus on the usage of the control interface and designed for easy readability. They are not optimized for production usage.

## How to run the examples?
1. Open one of the example project folders (e.g. [rust_control_interface](./rust_control_interface/)) in a dev container with VSCode
2. Build the example container and start the Ankaios cluster by running the shell command: 
```shell
./scripts/run_example.sh
```
3. Open an additional terminal in the dev container and run the following shell command to see the Ankaios agent logs: 
```shell
tail -f /var/log/ankaios-agent_A.log
```
4. Open an additional terminal in the dev container and run the following shell command to see the logs of the example workload: 
```shell
podman logs -f $(podman ps -a | grep control_interface | awk '{print $1}')
```

