# Startup configuration

Depending on the use-case, the Ankaios cluster can be started with an optional predefined list of [workloads](./glossary.md#workload) - the startup configuration.
Currently the startup configuration is provided as a file which is in YAML file format and can be passed to the Ankaios server through a command line argument.
If Ankaios is started without or with an empty startup configuration, workloads can still be added to the cluster dynamically during runtime.

**Note:** To be able to run a workload an Ankaios agent must be started on the same or on a different [node](./glossary.md#node).

## Configuration structure

The startup configuration is composed of a list of workload specifications within the `workloads` object.
A workload specification must contain the following information:

* `workload name`_(via field key)_, specify the workload name to identify the workload in the Ankaios system.
* `runtime`, specify the type of the runtime. Currently supported values are `podman` and `podman-kube`.
* `agent`, specify the name of the owning agent which is going to execute the workload.
* `restartPolicy`, specify how the workload should be restarted upon exiting (not implemented yet).
* `tags`, specify a list of `key` `value`  pairs.
* `runtimeConfig`, specify as a _string_ the configuration for the [runtime](./glossary.md#runtime) whose configuration structure is specific for each runtime, e.g., for `podman` runtime the [PodmanRuntimeConfig](#podmanruntimeconfig) is used.
* `controlInterfaceAccess`, specify the access rights of the workload for the control interface.

Example `startup-config.yaml` file:

```yaml
apiVersion: v0.1
workloads:
  nginx: # this is used as the workload name which is 'nginx'
    runtime: podman
    agent: agent_A
    restartPolicy: ALWAYS
    tags:
      - key: owner
        value: Ankaios team
    runtimeConfig: |
      image: docker.io/nginx:latest
      commandOptions: ["-p", "8081:80"]
    controlInterfaceAccess:
      allowRules:
      - type: StateRule
        operation: Read
        filterMask:
        - "workloadStates"
```

### PodmanRuntimeConfig

The runtime configuration for the `podman` runtime is specified as follows:

```yaml
generalOptions: [<comma>, <separated>, <options>]
image: <registry>/<image name>:<version>
commandOptions: [<comma>, <separated>, <options>]
commandArgs: [<comma>, <separated>, <arguments>]
```

where each attribute is passed directly to `podman run`.

If we take as an example the `podman run` command:

```podman --events-backend file run --env VAR=able docker.io/alpine:latest echo Hello!```

it would translate to the following runtime configuration:

```yaml
generalOptions: ["--events-backend", "file"]
image: docker.io/alpine:latest
commandOptions: ["--env", "VAR=able"]
commandArgs: ["echo", "Hello!"]
```

### PodmanKubeRuntimeConfig

The runtime configuration for the `podman-kube` runtime is specified as follows:

```yaml
generalOptions: [<comma>, <separated>, <options>]
playOptions: [<comma>, <separated>, <options>]
downOptions: [<comma>, <separated>, <options>]
manifest: <string containing the K8s manifest>
```

where each attribute is passed directly to `podman play kube`.

If we take as an example the `podman play kube` command:

```podman --events-backend file play kube --userns host manifest.yaml```

and the corresponding command for deleting the manifest file:

```podman --events-backend file play kube manifest.yaml --down --force```

they would translate to the following runtime configuration:

```yaml
generalOptions: ["--events-backend", "file"]
playOptions: ["--userns", "host"]
downOptions: ["--force"]
manifest: <contents of manifest.yaml>
```
