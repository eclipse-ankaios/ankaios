# Quickstart

If you have not installed Ankaios, please follow the instructions [here](https://eclipse-ankaios.github.io/ankaios/main/usage/installation/index.md). The following examples assumes that the installation script has been used with default options.

You can start workloads in Ankaios in a number of ways. For example, you can define a file with the startup configuration and use systemd to start Ankaios. The startup manifest file contains all of the workloads and their configuration that you want to be started by Ankaios.

Let's modify the default config which is stored in `/etc/ankaios/state.yaml`:

```
apiVersion: v1
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    restartPolicy: ALWAYS
    tags:
      owner: Ankaios team
    runtimeConfig: |
      image: docker.io/nginx:latest
      commandOptions: ["-p", "8081:80"]
```

Then we can start the Ankaios server:

```
sudo systemctl start ank-server
```

The Ankaios server will read the config but detect that no agent with the name `agent_A` is available that could start the workload, see logs with:

```
sudo journalctl -t ank-server
```

Now let's start an agent:

```
sudo systemctl start ank-agent
```

This Ankaios agent will run the workload that has been assigned to it. We can use the Ankaios CLI to check the current state:

```
ank get state
```

which creates:

```
desiredState:
  apiVersion: v1
  workloads:
    nginx:
      agent: agent_A
      tags:
        owner: Ankaios team
      dependencies: {}
      restartPolicy: ALWAYS
      runtime: podman
      runtimeConfig: |
        image: docker.io/nginx:latest
        commandOptions: ["-p", "8081:80"]
      configs: {}
      files: []
  configs: {}
workloadStates:
  agent_A:
    nginx:
      cc74dd34189ef3181a2f15c6c5f5b0e76aaefbcd55397e15314e7a25bad0864b:
        state: Running
        subState: Ok
        additionalInfo: ''
agents:
  agent_A:
    cpuUsage: 2
    freeMemory: 7989682176
```

or

```
ank get workloads
```

which results in:

```
WORKLOAD NAME   AGENT     RUNTIME   EXECUTION STATE   ADDITIONAL INFO
nginx           agent_A   podman    Running(Ok)
```

Ankaios also supports adding and removing workloads dynamically. The following command assumes that the `containerd` runtime is installed according to the [installation instructions](https://eclipse-ankaios.github.io/ankaios/main/usage/installation/index.md).

To add another workload call:

```
ank run workload \
helloworld \
--runtime containerd \
--agent agent_A \
--config 'image: docker.io/busybox:1.36
commandOptions: [ "-e", "MESSAGE=Hello World"]
commandArgs: [ "sh", "-c", "echo $MESSAGE"]'
```

We can check the state again with `ank get state` and see, that the workload `helloworld` with runtime `containerd` has been added to `desiredState.workloads` and the execution state is available in `workloadStates`:

```
WORKLOAD NAME   AGENT     RUNTIME      EXECUTION STATE   ADDITIONAL INFO
helloworld      agent_A   containerd   Succeeded(Ok)
nginx           agent_A   podman       Running(Ok)
```

As the workload had a one time job its state is `Succeeded(Ok)` and we can delete it from the state again with:

```
ank delete workload helloworld
```

Note

Workload names shall not be longer then 63 symbols and can contain only regular characters, digits, the "-" and "\_" symbols.

For next steps follow the [tutorial on sending and receiving vehicle data](https://eclipse-ankaios.github.io/ankaios/main/usage/tutorial-vehicle-signals/index.md) with workloads orchestrated by Ankaios. Then also check the reference documentation for the [startup configuration](https://eclipse-ankaios.github.io/ankaios/main/reference/startup-configuration/index.md) including the `podman-kube` runtime and also working with the [complete state data structure](https://eclipse-ankaios.github.io/ankaios/main/reference/complete-state/index.md).
