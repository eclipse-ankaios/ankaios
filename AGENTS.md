# Agent Instructions

## Golden rules (always follow)

- Do not invent container images; if none is provided, ask the user for the image.
- Prefer the simplest solution that matches the user’s request.
- Respect existing crate/module patterns; avoid unrelated refactors.

## Ankaios overview (context)

Ankaios is a lightweight embedded workload orchestrator for edge devices.

### Cluster roles

- An Ankaios cluster consists of **one Ankaios server** and **one or more Ankaios agents**.
- The **server** manages the cluster.
- **Agents** run workloads on parts of the edge device.

### Communication model

- All communication between Ankaios components always goes through the Ankaios server.
- Managed workloads can access the cluster state through the agent managing them using the **Control Interface**.
- The agent forwards requests to the server and returns the response to the workload.

### Applying state

- Ankaios accepts manifests via the `ank` CLI:
    - `ank apply <path to manifest>`
    - `ank run workload <name> --runtime <podman|podman_kube|containerd> --config <runtime specific config> --agent <agent name>`, e.g., `ank run workload workload_3 --runtime podman --config $'image: docker.io/alpine:latest\ncommandArgs: [ "sh", "-c",  "while true; do sleep 1; echo bla; done;"]' --agent agent_A`
- or as a startup manifest provided to the server.

### Quick local cluster in devcontainer

- `ankaios-start` cleans previous state, then starts:
    - an Ankaios server
    - a single agent named `agent_A`

- To stop the cluster and clean up state: `ankaios-clean` (also deletes all podman containers).

## State + workloads

### Complete state

An Ankaios complete state consists of:

- `desiredState`: the target state of the Ankaios cluster (same structure as an Ankaios manifest)
- `workloadStates`: the current state of workloads running in the cluster
- `agents`: running agents (workloads can be scheduled to agents)

Get complete state:

- `ank get state`
- Field masks example: `ank get state desiredState.workloads workloadStates`

### Workload lifecycle

Workloads can be:

- **Deleted**: removed from cluster state and stopped via the commands:
    - Delete workload directly: `ank delete workload <WORKLOAD_NAME>`; this command also supports a list of workload names separated with spaces.
    - Delete via manifest: `ank apply -d <MANIFEST_FILE>`
- **Unscheduled**: stop the workload but keep it in cluster state by setting the assigned agent to an empty string

## Manifest reference

An Ankaios manifest has this structure (replace placeholders like `<example>`; `[A|B|C]` means a list of allowed values where the first is default):

```yaml
apiVersion: v1 # version v0.1 is deprecated, but still supported for backwards compatibility;
workloads:
  <workload name>:
    runtime: podman
    agent: <agent name or empty string if not specified> # config items can be used here if mapped first in the workload configs object, e.g. {{deployment_agents.central}}
    restartPolicy: [NEVER|ON_FAILURE|ALWAYS] # restartPolicy is optional and can be omitted. The default value is NEVER.
    controlInterfaceAccess: # optional; specifies if the workload can access the Control Interface.
      allowRules: # optional; if no allowRules are specified, the Control Interface is not mounted
      - type: StateRule
        operation: Read
        filterMasks:
        - desiredState
      denyRules: # optional; explicitly deny access even if an allow rule would grant it
      - type: StateRule
        operation: Write
        filterMasks:
        - desiredState.workloads.* # deny write access to all workloads
    tags: # optional
      <tag_key>: <tag_value>
    configs: # optional; needed if general configs are used in runtimeConfig or agent
      port: web_server_port
      deployment_agents: agent_names
    runtimeConfig: |
      image: <the url of the image to start, e.g. docker.io/nginx:latest> # never make out images; either use the one stated like this or ask the customer to provide their own image
      commandOptions: [] # optional; list of "podman run" options, e.g. ["-p", "8088:80"]
      commandArgs: [] # optional; args to the container entrypoint, e.g. ["sleep", "5000"]
configs: # optional; may be used in runtimeConfig and agent, but must be mapped first in workload.configs
  web_server_port:
    access_port: "8081"
  agent_names:
    central: hpc1
    auxiliary: hpc2
```

## Build, run, tests (devcontainer)

### Build

- Build everything: `cargo build`
- Build specific components:
    - server: `cargo build -p ank-server`
    - agent: `cargo build -p ank-agent`
    - CLI: `cargo build -p ank`

### Run

- Start cluster: `ankaios-start`
- Start another agent (after server started): `ank-agent --name <AGENT_NAME>`
- Stop cluster + clean up state: `ankaios-clean`

Aliases in debug build mode:

- `ank-server`
- `ank-agent` (requires `--name <AGENT_NAME>`)
- `ank`

### Unit tests

- All unit tests: `just utest`
- Package-specific: `cargo test -p <package_name>` (e.g. `cargo test -p ank-server`)

### System tests (stest)

- Build and run all system tests: `just stest`
- Run systems tests only (after a previous build): `just stest-only`
- To run a specific test, run one of the commands above (stest or stest-only) followed by a filter for the test name which supports `*` wildcard, e.g., `just stest-only "*ON_FAILURE*"`, or `just stest "Allow write rule with wildcard string allows all writes"`

The systems tests normally cleanup after execution. If there are doubts about the current state of the machine, a complete cleanup can be explicitly done by calling `ankaios-clean` in the terminal.
