# Ankaios Control Interface Examples

The [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) allows the workload developers to easily integrate the communication between the Ankaios system and their applications.

This subfolder contains examples, written in different programming languages, about how to use the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) within a workload managed by Ankaios.

The intention of the examples is to give a simple introduction in various programming languages about the usage of the Control Interface in self-developed applications.
Furthermore, they shall enable an easy start to develop applications using the Control Interface by providing a basic development environment (Devcontainer) and a running example application.

All examples share the same behavior regardless of the programming language and do the following:

1. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) to start a new workload  (named `dynamic_workload`) dynamically.
2. Send a request to Ankaios cluster via the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) every 30 sec. to get the workload states that are part of the current state of the Ankaios cluster and output them to the console.

You can track the execution state of the dynamically added workload on the console and see when the workload `dynamic_nginx` is up and running (execution state: EXEC_RUNNING).

Every subfolder represents an example for a specific programming language. Feel free to try them out by navigating into the dedicated subfolder and executing the steps explained inside. 

**Note:** The examples are simplified to focus on the usage of the Control Interface and designed for easy readability. They are not optimized for production usage.