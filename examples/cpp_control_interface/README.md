# Ankaios control interface communcation with C++

This section provides an example workload developed in C++ that uses the [Ankaios control interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/).

## Build the example

Navigate to the example subfolder `examples/cpp_control_interface` and open the folder inside VSCode:

```shell
cd examples/cpp_control_interface
code .
```

Confirm the VSCode dialog to reopen the folder inside the devcontainer.

If you do not want to use the devcontainer feature of VSCode just run the following command:

Please replace the host path with your specific absolute path pointing to this example subfolder.

```shell
 docker run -it --rm -v /absolute/path/to/examples/config:/workspaces/app/config -v /absolute/path/to/examples/scripts:/workspaces/app/scripts -v /absolute/path/to/examples/cpp_control_interface:/workspaces/app --workdir /workspaces/app --privileged ghcr.io/eclipse-ankaios/app-ankaios-dev:latest /bin/bash
```

## Build and run the example using script

```shell
scripts/run_example.sh
```

## Build and run the example manually

Build the workload with the following command:

```shell
podman build -t control_interface_prod:0.1 -f .devcontainer/Dockerfile .
```

Start Podman service (only for For Ankaios version < 0.2 ):
```shell
podman system service --time=0 unix:///tmp/podman.sock &
```

Start the Ankaios server as background process:

```shell
ank-server --startup-config config/startupState.yaml > /var/log/ankaios-server.log 2>&1 &
```

Start the Ankaios agent `agent_A` as background process:

```shell
ank-agent --name agent_A -p /tmp/podman.sock > /var/log/ankaios-agent_A.log 2>&1 &
```

Watch and wait until the `dynamic_nginx` workload becomes state EXEC_RUNNING by using the podman logs command:

```shell
podman logs -f $(podman ps -a | grep control_interface | awk '{print $1}')
```

After the new nginx service was added dynamically and is running, you can do a curl to the nginx welcome website:

```shell
curl localhost:8081
```

Optionally, you can check the Ankaios server or agent logs:

```shell
tail -f /var/log/ankaios-server.log
tail -f /var/log/ankaios-agent_A.log
```
