# Working with CompleteState
## CompleteState
The complete state data structure [CompleteState](./_ankaios.proto.md#completestate) is used for building a request to Ankaios server to change or receive the state of the Ankaios system. It contains the `startupState` which describes the states provided at the start of the Ankaios system via the [startup configuration](./startup-configuration.md), the `currentState` which describes the current state of the Ankaios system and the `workloadStates` which gives the information about the execution state of all the workloads. By using of [CompleteState](./_ankaios.proto.md#completestate) in conjunction with the object field mask specific parts of the Ankaios state could be retrieved or updated.

Example: `ank get state` returns the complete state of Ankaios system:
```bash
requestId: ank-cli
startupState:
  workloads: {}
  configs: {}
  cronJobs: {}
currentState:
  workloads:
    api_sample:
      agent: agent_A
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      accessRights:
        allow: []
        deny: []
      runtime: podman
      name: api_sample
      restart: true
      tags:
      - key: owner
        value: Ankaios team
      runtimeConfig: |
        image: ankaios_workload_api_example
    hello3:
      agent: agent_B
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      accessRights:
        allow: []
        deny: []
      runtime: podman
      name: hello3
      restart: true
      tags:
      - key: owner
        value: Ankaios team
      runtimeConfig: |
        image: alpine:latest
        commandArgs: [ "echo", "Hello Ankaios"]
    hello1:
      agent: agent_B
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      accessRights:
        allow: []
        deny: []
      runtime: podman
      name: hello1
      restart: true
      tags:
      - key: owner
        value: Ankaios team
      runtimeConfig: |
        image: alpine:latest
        commandOptions: [ "--rm"]
        commandArgs: [ "echo", "Hello Ankaios"]
    nginx:
      agent: agent_A
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      accessRights:
        allow: []
        deny: []
      runtime: podman
      name: nginx
      restart: true
      tags:
      - key: owner
        value: Ankaios team
      runtimeConfig: |
        image: docker.io/nginx:latest
        commandOptions: ["-p", "8081:80"]
    hello2:
      agent: agent_B
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      accessRights:
        allow: []
        deny: []
      runtime: podman
      name: hello2
      restart: true
      tags:
      - key: owner
        value: Ankaios team
      runtimeConfig: |
        image: alpine:latest
        commandArgs: [ "echo", "Hello Ankaios"]
  configs: {}
  cronJobs: {}
workloadStates: []
```
It is not necessary to provide the whole structure of the the [CompleteState](./_ankaios.proto.md#completestate) data structure when using it in conjunction with the [object field mask](#object-field-mask). It is sufficient to provide the relevant branch of the [CompleteState](./_ankaios.proto.md#completestate) object. As an example, to change the restart behavior of the nginx workload, only the relevant branch of the [CompleteState](./_ankaios.proto.md#completestate) needs to be provided:
```bash
currentState:
  workloads:
    nginx:
      restart: false
```

## Object field mask
With the object field mask only specific parts of the Ankaios state could be retrieved or updated.
The object field mask can be constructed using the field names of the [CompleteState](./_ankaios.proto.md#completestate) data structure:
```text
<top level field name>.<second level field name>.<third level field name>.<...>
```

1. Example: `ank get state currentState.workloads.nginx` returns only the information about nginx workload:
```bash
currentState:
  workloads:
    nginx:
      agent: agent_A
      dependencies: {}
      updateStrategy: AT_MOST_ONCE
      accessRights:
        allow: []
        deny: []
      runtime: podman
      name: nginx
      restart: true
      tags:
      - key: owner
        value: Ankaios team
      runtimeConfig: |
        image: docker.io/nginx:latest
        commandOptions: ["-p", "8081:80"]
```

2. Example `ank get state currentState.workloads.nginx.runtimeConfig` returns only the runtime configuration of nginx workload:
```bash
currentState:
  workloads:
    nginx:
      runtimeConfig: |
        image: docker.io/nginx:latest
        commandOptions: ["-p", "8081:80"]
```

3. Example `ank set state -f new-state.yaml currentState.workloads.nginx.restart` changes the restart behavior of nginx workload to `false`:
```yaml title="new-state.yaml"
currentState:
  workloads:
    nginx:
      restart: false
```
