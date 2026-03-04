# Agent Instructions

## 1) Understand the purpose

This file provides operational guidance for AI agents working in the Ankaios repository.
Use it as a decision and execution reference.

Ankaios is a lightweight embedded workload orchestrator for edge devices.

## 2) Follow non-negotiable rules

- Do not invent container images; if none is provided, ask the user for an image
- Prefer the simplest solution that satisfies the request
- Respect existing crate/module patterns; avoid unrelated refactors

## 3) Execute the agent playbook

1. Clarify the target outcome from the user request (what must change, and what must not)
2. Check constraints before acting:
    - Ensure a required container image is provided
    - Keep scope within relevant crates/modules
    - Keep the solution minimal
3. Choose the smallest valid action path:
    - Inspect state: `ank get state ...` / `ank get workloads`
    - Update state: `ank apply ...` or `ank run workload ...`
    - Handle cleanup/lifecycle: `ank delete workload ...` or unschedule via empty agent
4. Verify results against cluster state (`desiredState` and `workloadStates`)
5. Report outcome with concise next-step options (only what is relevant to the request)

## 4) Understand the system model

### Identify cluster roles

- Treat one cluster as exactly one server and one or more agents
- Use the server as the cluster-state authority
- Use agents to run workloads on edge-device parts

### Follow the communication model

- Route all component communication through the Ankaios server
- Access cluster state from managed workloads via the agent's Control Interface
- Forward workload requests through the agent to the server and return responses

## 5) Inspect state and workload lifecycle

### Read complete state

An Ankaios complete state contains:

- Read `desiredState` as the target cluster state (same structure as a manifest)
- Read `workloadStates` as the actual workload runtime states
- Read `agents` as the currently running agents

Useful commands:

- `ank get state`
- `ank get state desiredState.workloads workloadStates` (field-mask example)

### Apply workload lifecycle operations

- Delete workload to remove it from state and stop runtime:
    - `ank delete workload <WORKLOAD_NAME>`
    - `ank delete workload <WL_A> <WL_B> ...`
    - `ank apply -d <MANIFEST_FILE>`
- Unschedule workload to stop runtime but keep it in state by setting workload agent to empty string

## 6) Apply desired state

Ankaios accepts desired state through:

- `ank apply <path to manifest>`
- `ank apply -d <path to manifest>` to delete a previously applied manifest
- `ank run workload <name> --runtime <podman|podman_kube|containerd> --config <runtime specific config> --agent <agent name>`
- Pass a startup manifest to the server

Example:

- `ank run workload workload_3 --runtime podman --config $'image: docker.io/alpine:latest\ncommandArgs: [ "sh", "-c", "while true; do sleep 1; echo bla; done;" ]' --agent agent_A`

## 7) Operate the local devcontainer

### Start or stop the cluster

- `ankaios-start`: cleans old state and starts an Ankaios server and an agent named `agent_A`. Don't call `ankaios-clean` beforehand, this is already done.
- `ankaios-clean`: stops cluster and removes state (also deletes podman containers)

### Use debug-build command aliases

All commands are aliased in the bashrc and can be called from everywhere:

- `ank-server`
- `ank-agent` (requires `--name <AGENT_NAME>`)
- `ank`

`ank` supports the following output options:

- `-v` or `--verbose`: Enable debug traces
- `-q` or `--quiet`: Disable all output`
- `--no-wait`: Do not wait for workloads to be created/deleted`

## 8) Build and test

### Build the project

- Full workspace: `cargo build`
- Server only: `cargo build -p ank-server`
- Agent only: `cargo build -p ank-agent`
- CLI only: `cargo build -p ank`

### Run unit tests

- All unit tests: `just utest`
- Package-specific: `cargo test -p <package_name>` (example: `cargo test -p ank-server`)

### Run system tests (stest)

- Build + run all: `just stest`
- Run only (after previous build): `just stest-only`
- Run one test/filter:
    - `just stest-only "*ON_FAILURE*"`
    - `just stest "Allow write rule with wildcard string allows all writes"`

System tests usually clean up automatically. If state is uncertain, run `ankaios-clean`.

## 9) Create and review manifests

Use this structure when creating or reviewing manifests.
Replace placeholders like `<example>`.
For `[A|B|C]`, the first value is the default.

```yaml
apiVersion: v1 # version v0.1 is deprecated but still supported for backwards compatibility
workloads:
  <workload name>:
    runtime: podman
    agent: <agent name or empty string if not specified> # config values can be used if mapped first in workload.configs, e.g. {{deployment_agents.central}}
    restartPolicy: [NEVER|ON_FAILURE|ALWAYS] # optional, default is NEVER
    controlInterfaceAccess: # optional; defines workload access to Control Interface
      allowRules: # optional; if omitted, Control Interface is not mounted
      - type: StateRule
        operation: Read
        filterMasks:
        - desiredState
      denyRules: # optional; explicit deny overrides allow
      - type: StateRule
        operation: Write
        filterMasks:
        - desiredState.workloads.* # deny writes to all workloads
    tags: # optional
      <tag_key>: <tag_value>
    configs: # optional; required when general configs are referenced in runtimeConfig or agent
      port: web_server_port
      deployment_agents: agent_names
    runtimeConfig: |
      image: <image URL, e.g. docker.io/nginx:latest> # never invent an image; ask the user if unknown
      commandOptions: [] # optional; podman run options, e.g. ["-p", "8088:80"]
      commandArgs: [] # optional; args to container entrypoint, e.g. ["sleep", "5000"]
configs: # optional; usable in runtimeConfig and agent, but must be mapped first in workload.configs
  web_server_port:
    access_port: "8081"
  agent_names:
    central: hpc1
    auxiliary: hpc2
```
