# Working with CompleteState

## CompleteState

The complete state data structure [CompleteState](./_ankaios.proto.md#completestate) is used for building a request to Ankaios server to change or receive the state of the Ankaios system. It contains the `desiredState` which describes the state of the Ankaios system the user wants to have, the `workloadStates` which gives the information about the execution state of all the workloads and the `agents` field containing the names of the Ankaios agents that are currently connected to the Ankaios server. By using of [CompleteState](./_ankaios.proto.md#completestate) in conjunction with the object field mask specific parts of the Ankaios state could be retrieved or updated.

Example: `ank get state` returns the complete state of Ankaios system with runtimes `podman`, `containerd` and `podman-kube`:

```bash
desiredState:
  apiVersion: v1
  workloads:
    hello-pod:
      agent: agent_B
      tags:
        owner: Ankaios team
      dependencies: {}
      restartPolicy: NEVER
      runtime: podman-kube
      runtimeConfig: |
        manifest: |
          apiVersion: v1
          kind: Pod
          metadata:
            name: hello-pod
          spec:
            restartPolicy: Never
            containers:
            - name: looper
              image: alpine:latest
              command:
              - sleep
              - 50000
            - name: greater
              image: alpine:latest
              command:
              - echo
              - "Hello from a container in a pod"
      configs: {}
    hello1:
      agent: agent_B
      tags:
        owner: Ankaios team
      dependencies: {}
      runtime: podman
      runtimeConfig: |
        image: alpine:latest
        commandOptions: [ "--rm"]
        commandArgs: [ "echo", "Hello Ankaios"]
      configs: {}
    hello2:
      agent: agent_B
      tags:
        owner: Ankaios team
      dependencies: {}
      restartPolicy: ALWAYS
      runtime: containerd
      runtimeConfig: |
        image: alpine:latest
        commandOptions: [ "--entrypoint", "/bin/sh" ]
        commandArgs: [ "-c", "echo 'Always restarted.'; sleep 2"]
      configs: {}
    nginx:
      agent: agent_A
      tags:
        owner: Ankaios team
      dependencies: {}
      restartPolicy: ON_FAILURE
      runtime: podman
      runtimeConfig: |
        image: docker.io/nginx:latest
        commandOptions: ["-p", "8081:80"]
      configs: {}
  configs: {}
workloadStates: []
agents: {}
```

It is not necessary to provide the whole structure of the [CompleteState](./_ankaios.proto.md#completestate) data structure when using it in conjunction with the [object field mask](#object-field-mask). It is sufficient to provide the relevant branch of the [CompleteState](./_ankaios.proto.md#completestate) object. As an example, to change the restart behavior of the nginx workload, only the relevant branch of the [CompleteState](./_ankaios.proto.md#completestate) needs to be provided:

```bash
desiredState:
  workloads:
    nginx:
      restartPolicy: ALWAYS
```

!!! Note

    In case of workload names, the naming convention states that their names shall:<br>
    - contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_"<br>
    - have a minimal length of 1 character<br>
    - have a maximal length of 63 characters<br>
    Also, agent name shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_".

!!! Note

    Although updating the runtime of a workload is not prohibited, in practice it is not possible. Thus, updating the state of a workload with a different runtime will yield in sending a `Pending(StartingFailed)` workload state with an unsupported runtime message and the workload shall not start.

## Object field mask

With the object field mask only specific parts of the Ankaios state could be retrieved or updated.
The object field mask can be constructed using the field names of the [CompleteState](./_ankaios.proto.md#completestate) data structure:

```text
<top level field name>.<second level field name>.<third level field name>.<...>
```

1. Example: `ank get state desiredState.workloads.nginx` returns only the information about nginx workload:

    ```yaml
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
    ```

2. Example `ank get state desiredState.workloads.nginx.runtimeConfig` returns only the runtime configuration of nginx workload:

    ```yaml
    desiredState:
      apiVersion: v1
      workloads:
        nginx:
          runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: ["-p", "8081:80"]
    ```

3. Example `ank set state desiredState.workloads.nginx.restartPolicy new-state.yaml` changes the restart behavior of nginx workload to `NEVER`:

    ```yaml title="new-state.yaml"
    desiredState:
      workloads:
        nginx:
          restartPolicy: NEVER
    ```

## Dry run

A dry run shows which workloads would be added, updated or deleted by an update of the state without applying it. The Ankaios server performs the same checks as for a regular update, e.g., the validation of the workload configurations and the check for cyclic dependencies, but does not change the state and does not send anything to the Ankaios agents.

The `ank` CLI provides the `--dry-run` option for the `ank set state` and the `ank apply` commands. As an example, the dry run of example 3 above:

```bash
ank set state --dry-run desiredState.workloads.nginx.restartPolicy new-state.yaml
```

shows that the nginx workload would be updated:

```text
WORKLOAD NAME   AGENT     ACTION
nginx           agent_A   updated
```

The action is one of:

- `added`: the workload would be created.
- `updated`: the workload would be replaced by its new configuration, i.e., it would be restarted. If the agent of the workload changes, the old and the new agent are shown, e.g., `agent_A -> agent_B`.
- `deleted`: the workload would be removed.

If the update would not change any workload, the CLI prints `No workloads would be changed.`. If the update would be rejected, the CLI shows the same error as for the update without the `--dry-run` option.

The option can be combined with the other options of `ank apply`, e.g., `ank apply -d --dry-run manifest.yaml` shows which workloads would be deleted.

Workloads can request a dry run via the [control interface](./control-interface.md) by sending an [UpdateStateDryRunRequest](./_ankaios.proto.md#updatestatedryrunrequest) instead of an [UpdateStateRequest](./_ankaios.proto.md#updatestaterequest). Both requests have the same fields. The response is an [UpdateStateSuccess](./_ankaios.proto.md#updatestatesuccess) message with the instance names of the workloads that would be added and deleted, or an error. A dry run needs the same [write access](./control-interface.md#authorization) as the update itself.

!!! Note

    Mutating hooks configured in the Ankaios server are not executed during a dry run. If mutating hooks are configured, the actual update can differ from the result of the dry run.
