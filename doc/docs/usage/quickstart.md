# Quickstart

If you have not installed Ankaios, please follow the instructions
[here](installation.md). The following examples assume that the
installation script has been used with default options.

Ankaios needs a startup configuration that contains all the workloads and their
configuration which should be started when Ankaios starts up.

Let's modify the default config which is stored in `/etc/ank/state.yaml`:

```yaml
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    restart: true
    updateStrategy: AT_MOST_ONCE
    accessRights: # (1)
      allow: []
      deny: []
    tags:
      - key: owner
        value: Ankaios team
    runtimeConfig: |
      image: docker.io/nginx:latest
      commandOptions: ["-p", "8081:80"]
```

1.  Note that access rights are currently not implemented.


Then we can start the Ankaios server:

```shell
systemctl start ank-server
```

The Ankaios server will read the config but detect that no agent with the name
`agent_A` is available that could start the workload, see logs with:

```shell
journalctl -u ank-server
```

Now let's start an agent:

```shell
systemctl start ank-agent
```

This Ankaios agent will run the workload that has been assigned to it. We can
use the Ankaios CLI to check the current state:

```shell
ank get state
```

Ankaios also provides adding and removing workloads dynamically.
To add another workload call:

```shell
ank run workload \
--runtime podman \
--agent agent_A \
--config 'image: docker.io/busybox:1.36
commandOptions: [ "-e", "MESSAGE='Hello World'"]
commandArgs: [ "sh", "-c", "echo $MESSAGE"]
' helloworld
```

We can check the state again with `ank get state` and see, that the workload
`helloworld` has been added to `currentState.workloads` and the execution
state is available in `workloadStates`.

As the workload had a one time job its state is `ExecSucceeded` and we can 
delete it from the state again with:

```shell
ank delete workload helloworld
```
