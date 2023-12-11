# Quickstart

If you have not installed Ankaios, please follow the instructions
[here](installation.md). The following examples assumes that the
installation script has been used with default options.

Ankaios needs a startup configuration that contains all the workloads and their
configuration which should be started when Ankaios starts up.

Let's modify the default config which is stored in `/etc/ankaios/state.yaml`:

```yaml
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    restart: true
    updateStrategy: AT_MOST_ONCE
    accessRights:
      allow: []
      deny: []
    tags:
      - key: owner
        value: Ankaios team
    runtimeConfig: |
      image: docker.io/nginx:latest
      commandOptions: ["-p", "8081:80"]
```

Then we can start the Ankaios server:

```shell
sudo systemctl start ank-server
```

The Ankaios server will read the config but detect that no agent with the name
`agent_A` is available that could start the workload, see logs with:

```shell
journalctl -t ank-server
```

Now let's start an agent:

```shell
sudo systemctl start ank-agent
```

This Ankaios agent will run the workload that has been assigned to it. We can
use the Ankaios CLI to check the current state:

```shell
ank get state
```

which creates:

```yaml
requestId: ank-cli
startupState:
  workloads: {}
  configs: {}
  cronJobs: {}
currentState:
  workloads:
    nginx:
      agent: agent_A
      name: nginx
      tags:
      - key: owner
        value: Ankaios team
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      restart: true
      accessRights:
        allow: []
        deny: []
      runtime: podman
      runtimeConfig: |
        image: docker.io/nginx:latest
        commandOptions: ["-p", "8081:80"]
  configs: {}
  cronJobs: {}
workloadStates:
- workloadName: nginx
  agentName: agent_A
  executionState: ExecRunning
```

or

```shell
ank get workloads
```

which results in:

```text
 WORKLOAD NAME   AGENT     RUNTIME   EXECUTION STATE
 nginx           agent_A   podman    Running
```

Ankaios also supports adding and removing workloads dynamically.
To add another workload call:

```shell
ank run workload \
helloworld \
--runtime podman \
--agent agent_A \
--config 'image: docker.io/busybox:1.36
commandOptions: [ "-e", "MESSAGE=Hello World"]
commandArgs: [ "sh", "-c", "echo $MESSAGE"]'
```

We can check the state again with `ank get state` and see, that the workload
`helloworld` has been added to `currentState.workloads` and the execution
state is available in `workloadStates`.

As the workload had a one time job its state is `ExecSucceeded` and we can
delete it from the state again with:

```shell
ank delete workload helloworld
```

For next steps see the reference documentation for the
[startup configuration](../reference/startup-configuration.md) including the
`podman-kube` runtime and also working with the
[complete state data structure](../reference/complete-state.md).
