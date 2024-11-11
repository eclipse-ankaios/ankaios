# Working with CompleteState

## CompleteState

The complete state data structure [CompleteState](./_ankaios.proto.md#completestate) is used for building a request to Ankaios server to change or receive the state of the Ankaios system. It contains the `desiredState` which describes the state of the Ankaios system the user wants to have, the `workloadStates` which gives the information about the execution state of all the workloads and the `agents` field containing the names of the Ankaios agents that are currently connected to the Ankaios server. By using of [CompleteState](./_ankaios.proto.md#completestate) in conjunction with the object field mask specific parts of the Ankaios state could be retrieved or updated.

Example: `ank -k get state` returns the complete state of Ankaios system:

!!! Note

    The instructions assume the default installation without mutual TLS (mTLS) for communication. With `-k` or `--insecure` the `ank` CLI will connect without mTLS. Alternatively, set the environment variable `ANK_INSECURE=true` to avoid passing the argument to each `ank` CLI command. For an Ankaios setup with mTLS, see [here](../usage/mtls-setup.md).

```bash
desiredState:
  apiVersion: v0.1
  workloads:
    hello-pod:
      agent: agent_B
      tags:
      - key: owner
        value: Ankaios team
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
      - key: owner
        value: Ankaios team
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
      - key: owner
        value: Ankaios team
      dependencies: {}
      restartPolicy: ALWAYS
      runtime: podman
      runtimeConfig: |
        image: alpine:latest
        commandOptions: [ "--entrypoint", "/bin/sh" ]
        commandArgs: [ "-c", "echo 'Always restarted.'; sleep 2"]
      configs: {}
    nginx:
      agent: agent_A
      tags:
      - key: owner
        value: Ankaios team
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

## Object field mask

With the object field mask only specific parts of the Ankaios state could be retrieved or updated.
The object field mask can be constructed using the field names of the [CompleteState](./_ankaios.proto.md#completestate) data structure:

```text
<top level field name>.<second level field name>.<third level field name>.<...>
```

1. Example: `ank -k get state desiredState.workloads.nginx` returns only the information about nginx workload:

    ```yaml
    desiredState:
      apiVersion: v0.1
      workloads:
        nginx:
          agent: agent_A
          tags:
          - key: owner
            value: Ankaios team
          dependencies: {}
          restartPolicy: ALWAYS
          runtime: podman
          runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: ["-p", "8081:80"]
          configs: {}
    ```

2. Example `ank -k get state desiredState.workloads.nginx.runtimeConfig` returns only the runtime configuration of nginx workload:

    ```yaml
    desiredState:
      apiVersion: v0.1
      workloads:
        nginx:
          runtimeConfig: |
            image: docker.io/nginx:latest
            commandOptions: ["-p", "8081:80"]
    ```

3. Example `ank -k set state desiredState.workloads.nginx.restartPolicy new-state.yaml` changes the restart behavior of nginx workload to `NEVER`:

    ```yaml title="new-state.yaml"
    desiredState:
      workloads:
        nginx:
          restartPolicy: NEVER
    ```
