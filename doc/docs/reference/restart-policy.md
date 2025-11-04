
# Restart Policy

The restart policy of a workload enables the user to determine whether a workload is automatically restarted when it terminates.
By default, workloads are not restarted. However, the restart policy can be configured to always restart the workload, or to restart the workload under certain conditions.

## Supported Restart Policies

The following restart policies are available for a workload:

| Restart Policy | Description                                                                               | Restart on ExecutionState           |
| -------------- | ----------------------------------------------------------------------------------------- | ----------------------------------- |
| NEVER          | The workload is never restarted. Once the workload exits, it remains in the exited state. | -                                   |
| ON_FAILURE     | If the workload exits with a non-zero exit code, it will be restarted.                    | Failed(ExecFailed)                  |
| ALWAYS         | The workload is restarted upon termination, regardless of the exit code.                  | Succeeded(Ok) or Failed(ExecFailed) |

Ankaios restarts the workload when the workload has exited and the configured restart policy aligns with the workload's `ExecutionState`, as detailed in the aforementioned table. It does not restart the workload if the user explicitly deletes the workload via the Ankaios CLI or if Ankaios receives a delete request for that workload via the Control Interface.

!!! Note

    Ankaios does not consider inter-workload dependencies when restarting a workload because it was already running before it has exited.

## Configure Restart Policies

The field `restartPolicy` enables the user to define the restart policy for each workload within the Ankaios manifest. The field is optional. If the field is not provided, the default restart policy `NEVER` is applied.

The following Ankaios manifest contains workloads with different restart policies:

```yaml linenums="1" hl_lines="6 14 29"
apiVersion: v1
workloads:
  restarted_always:
    runtime: podman
    agent: agent_A
    restartPolicy: ALWAYS # (1)!
    runtimeConfig: |
      image: alpine:latest
      commandOptions: [ "--entrypoint", "/bin/sh" ]
      commandArgs: [ "-c", "echo 'Always restarted.'; sleep 2"]
  restarted_never:
    runtime: podman
    agent: agent_A
    restartPolicy: NEVER # (2)!
    runtimeConfig: |
      image: alpine:latest
      commandOptions: [ "--entrypoint", "/bin/sh" ]
      commandArgs: [ "-c", "echo 'Explicitly never restarted.'; sleep 2"]
  default_restarted_never: # default restart policy is NEVER
    runtime: podman
    agent: agent_A
    runtimeConfig: |
      image: alpine:latest
      commandOptions: [ "--entrypoint", "/bin/sh" ]
      commandArgs: [ "-c", "echo 'Implicitly never restarted.'; sleep 2"]
  restarted_on_failure:
    runtime: podman
    agent: agent_A
    restartPolicy: ON_FAILURE # (3)!
    runtimeConfig: |
      image: alpine:latest
      commandOptions: [ "--entrypoint", "/bin/sh" ]
      commandArgs: [ "-c", "echo 'Restarted on failure.'; sleep 2; exit 1"]
```

1. This workload is always restarted upon termination.
2. This workload is never restarted regardless of the exit code.
3. This workload is restarted only when it exits with a non-zero exit code.
