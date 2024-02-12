# Ankaios Command Line Interface - SW Design

## About this document

This document describes the Software Design for the Ankaios Ank. The Ankaios Ank is the Command Line Interface (CLI) of Ankaios.

Ankaios is a workload orchestrator supporting a subset of the Kubernetes configurations and is targeted at the automotive use case.

The CLI is a command line tool, which allows developers to directly interact with the cluster. E.g. it allows to get or set current state, get workloads...

## Context View

The Ankaios Ank is connected to the Server through the same interface as the Agent.

![Context](drawio/context_view.drawio.svg)

## Constraints, risks and decisions

No Constraints or risks are known at the time of writing this document.

### Design decisions

The following section holds the design decisions taken during the development of the CLI.

#### CLI uses proprietary tracing

`swdd~cli-use-proprietary-tracing~1`

Status: approved

The CLI shall use its own proprietary tracing functions with following features:

| message type     | output       | features                                                                                      |
| ---------------- | ------------ | --------------------------------------------------------------------------------------------- |
| error            | `io::stderr` | writes a message to the output and terminates the application with exit code `1`          |
| command response | `io::stdout` | writes a message to the output and terminates the application with exit code `0`          |
| debug            | `io::stdout` | writes a message to the output if the verbose mode is enabled (does not terminate the application) |

Rationale:

The CLI is an application different than Ankaios server or Ankaios agent.
The CLI interacts directly with the user and the user expects a response to the command (if available).
Therefore the information shall not be written to the log, but it shall be provided to the user.

Existing crates either do not behave as it is required or they are too complex for such task.

Needs:

* impl

Considered alternatives:

* keep on using the environment logger
* use another crate which provides tracing functions

## Structural view

Following diagram shows the structural view of the Ankaios Ank.

![Unit overview](drawio/unit_overview.drawio.svg)

### CLI (parser)

The CLI parses the commands entered by the user.
This also includes error handling when the user enters unsupported command or forgets to set a mandatory parameter.

### CliCommands

The CliCommands implements the commands.
It uses FromServer Channel and ToServer Channel to interact with the server.

### External Libraries

#### Communication Middleware

The Communication Middleware is responsible for the connection between the Ankaios Server and the Ankaios Agent or the Ankaios CLI.

#### FromServer Channel, ToServer Channel

The channels are defined in the `common` library.

## Behavioral view

This chapter defines the runtime behavior of the Ankaios Ank in details. The following chapters show essential parts of the behavior and describe the requirements towards the Ankaios Ank.

### Startup

The Ankaios Ank is a standalone application which starts when the user enters a command and terminates as soon as the command exits.
Here is an overview how each command looks like:

![Command Overview](plantuml/seq_cmd_overview.svg)

The startup section is detailed in the next diagram.
Implementation of each command is detailed in the next sub-chapters.

![Startup](plantuml/seq_cmd_startup.svg)

#### Ankaios CLI communicates only with the Ankaios Server
`swdd~server-handle-cli-communication~1`

Status: approved

The Ankaios CLI shall only directly communicate with the Ankaios Server.

Tags:
- CliStartup

Needs:
- impl
- itest

#### All communication between Server and Ankaios CLI goes through Communication Middleware
`swdd~cli-communication-over-middleware~1`

Status: approved

All communication between Server and Ankaios CLI goes through a Communication Middleware plugin configured at compile time for the Ankaios Server and Ankaios CLI.

Rationale: Ankaios shall provide a possibility to exchange the communication layer, but dynamic reconfigurations at startup or runtime are not required.

Tags:
- CliStartup

Needs:
- impl
- itest

#### CLI supports environment variables
`swdd~cli-shall-support-environment-variables~1`

Status: approved

The Ankaios CLI shall support the usage of the following environment variables:

- `ANK_SERVER_URL`, for providing the server url

Rationale:
This increases usability for the Ankaios CLI when the Ankaios CLI is used in different terminal windows to connect to the same Ankaios server remotely.

Tags:
- CliStartup

Needs:
- impl

#### CLI prioritizes cli argument over environment variable
`swdd~cli-prioritizes-cli-argument-over-environment-variable~1`

Status: approved

Command line arguments provided to the the Ankaios CLI shall overwrite environment variables.

Tags:
- CliStartup

Needs:
- impl

#### CLI is a standalone application
`swdd~cli-standalone-application~1`

Status: approved

The Ankaios CLI shall be a standalone application (separate from the Ankaios Server and Client).

Tags:
- CliStartup

Needs:
- impl
- itest

### `ank get state`

![Get current state](plantuml/seq_get_state.svg)

#### CLI provides the get current state
`swdd~cli-provides-get-current-state~1`

Status: approved

The Ankaios CLI shall provide a function to get the current state.

Tags:
- GetCurrentState

Needs:
- impl
- utest

#### CLI blocks until the Ankaios Server responds to the request to get the current state
`swdd~cli-blocks-until-ankaios-server-responds-get-current-state~1`

Status: approved

When the user invokes the CLI with a request to the get current state, the CLI shall block and wait until the response from the Ankaios Server is received.

Tags:
- GetCurrentState

Needs:
- impl
- utest

#### CLI returns the current state from Ankaios Server via CLI communication interface
`swdd~cli-returns-current-state-from-server~1`

Status: approved

When the CLI receives the current state from Ankaios Server, the CLI shall return this response to the user.

Tags:
- GetCurrentState

Needs:
- impl
- utest

#### CLI shall support presenting the current state as JSON
`swdd~cli-shall-support-current-state-json~1`

Status: approved

When the CLI receives the current state from Ankaios Server via CLI communication interface,
the CLI shall support the possibility to present the current state as a JSON to the user.

Tags:
- GetCurrentState

Needs:
- impl
- utest

#### CLI shall support presenting the current state as YAML
`swdd~cli-shall-support-current-state-yaml~1`

Status: approved

When the CLI receives the current state from Ankaios Server via CLI communication interface,
the CLI shall support the possibility to present the current state as a YAML to the user.

Tags:
- GetCurrentState

Needs:
- impl
- utest

#### CLI provides object field mask as arguments to get only the given parts of current state
`swdd~cli-provides-object-field-mask-arg-to-get-partial-current-state~1`

Status: approved

The Ankaios CLI shall provide an option to request and deliver only a part of the current state.

Tags:
- GetCurrentState

Needs:
- impl
- utest

#### CLI returns a compact state when provided object field mask arguments
`swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1`

Status: approved

When an object field mask is provided as additional argument, the Ankaios CLI shall return the compact state containing the values of the given fields.

Tags:
- GetCurrentState

Needs:
- impl
- utest

### `ank get workload`

![Get a list of Workloads](plantuml/seq_get_workload.svg)

#### CLI provides the list of workloads
`swdd~cli-provides-list-of-workloads~1`

Status: approved

The Ankaios CLI shall provide a function to get the list of workloads.

Tags:
- GetWorkloads

Needs:
- impl
- utest

#### CLI blocks until the Ankaios Server responds to the request to get the list of workloads
`swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1`

Status: approved

When the user invokes the CLI with a request to get the list of workloads, the CLI shall block and wait until the response from the Ankaios Server is received.

Tags:
- GetWorkloads

Needs:
- impl
- utest

#### CLI returns the list of workloads from Ankaios Server via CLI communication interface
`swdd~cli-returns-list-of-workloads-from-server~1`

Status: approved

When the CLI receives the list of workloads from Ankaios Server, the CLI shall return this response to the user.

Tags:
- GetWorkloads

Needs:
- swdd
- impl
- utest

#### CLI shall present the list of workloads as a table
`swdd~cli-shall-present-list-workloads-as-table~1`

Status: approved

When the CLI receives the list of workloads from the Ankaios Server via CLI communication interface, the CLI shall present the list as a table with following columns:

| WORKLOAD NAME | AGENT | RUNTIME | EXECUTION STATE | ADDITIONAL INFO    |
| ------------- | ----- | ------- | --------------- | ------------------ |
| workload1     | agent | runtime | state           | state related info |
| workload2     | agent | runtime | state           | state related info |

Note:
The column runtime is not filled when the workload has been deleted.
This can happen when the workload has been deleted from the current state and the workload state is reported as "removed".

Tags:
- GetWorkloads

Needs:
- impl
- utest

#### CLI shall sort the list of workloads
`swdd~cli-shall-sort-list-of-workloads~1`

Status: approved

When the CLI receives the list of workloads from the Ankaios Server via CLI communication interface, the CLI shall sort the list by workload name.

Tags:
- GetWorkloads

Needs:
- impl
- utest

#### CLI shall filter the list of workloads
`swdd~cli-shall-filter-list-of-workloads~1`

When the CLI receives the list of workloads from the Ankaios Server via CLI communication interface,
the CLI shall filter the workloads from the server using filtering criteria entered by the user in the command.

Tags:
- GetWorkloads

Needs:
- impl
- utest

#### CLI shall print empty table
`swdd~cli-shall-print-empty-table~1`

When the CLI receives the list of workloads from the Ankaios Server via CLI communication interface and the list of workloads is empty,
the CLI shall present only the header of the table to the user.

Tags:
- GetWorkloads

Needs:
- impl
- utest

### `ank set state`

![Set current state](plantuml/seq_set_state.svg)

#### CLI provides a function to set the current state
`swdd~cli-provides-set-current-state~1`

Status: approved

The Ankaios CLI shall provide a function to set the current state.

Tags:
- SetCurrentState

Needs:
- swdd
- impl
- utest

#### CLI blocks until the Ankaios Server responds to the request to set the current state
`swdd~cli-blocks-until-ankaios-server-responds-set-current-state~1`

Status: approved

When the user invokes the CLI with a request to set the current state, the CLI shall block and wait until the response from the Ankaios Server is received.

Tags:
- SetCurrentState

Needs:
- impl
- utest

#### CLI shall support YAML files with the state object to set current state
`swdd~cli-supports-yaml-to-set-current-state~1`

Status: approved

When the user invokes the CLI with a request to the set current state, the CLI shall support files in YAML format with the state object.

Tags:
- SetCurrentState

Needs:
- impl
- utest

### `ank delete workload`

![Delete workload](plantuml/seq_delete_workload.svg)

#### CLI provides a function to delete workloads
`swdd~cli-provides-delete-workload~1`

Status: approved

The Ankaios CLI shall provide a function to delete workloads.

Tags:
- DeleteWorkload

Needs:
- swdd
- impl
- utest

#### CLI blocks until the Ankaios Server responds to the request to delete workloads
`swdd~cli-blocks-until-ankaios-server-responds-delete-workload~1`

Status: approved

When the user invokes the CLI with a request to delete workloads, the CLI shall block and wait until the response from the Ankaios Server is received.

Tags:
- DeleteWorkload

Needs:
- impl
- utest

#### Do not send the request to delete workloads when they are not found
`swdd~no-delete-workloads-when-not-found~1`

Status: approved

When the user invokes the CLI with a request to delete workloads and the CLI does not find the workloads in the current state, the CLI shall not send the request to delete workloads to the server.

Tags:
- DeleteWorkload

Needs:
- impl
- utest

### `ank run workload`

![Run workload](plantuml/seq_run_workload.svg)

#### CLI provides a function to run a workload
`swdd~cli-provides-run-workload~1`

Status: approved

The Ankaios CLI shall provide a function to run workload.

Tags:
- RunWorkload

Needs:
- impl
- utest

#### CLI blocks until the Ankaios Server responds to the request to run workloads
`swdd~cli-blocks-until-ankaios-server-responds-run-workload~1`

Status: approved

When the user invokes the CLI with a request to run a workload, the CLI shall block and wait until the response from the Ankaios Server is received.

Tags:
- RunWorkload

Needs:
- impl
- utest

### `ank apply [-d] [--agent agent_name] <manifest.yaml> ...`

#### Ankaios manifest

The Ankaios manifest is a YAML (or a JSON) file composed of a list of workload specifications under the `workloads` keyword.

```yaml
# Example of a list of two workload specifications with the names 'nginx' and 'hello1'.
workloads:
  nginx:
    agent: agent_A
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
  hello1:
    # For this workload the following are not set:
    # - agent name
    # - dependencies -> defaults to {}
    tags: []
    restart: true
    updateStrategy: AT_MOST_ONCE
    accessRights:
      allow: []
      deny: []
    runtime: podman-kube
    runtimeConfig: |
      image: alpine:latest
      commandOptions: [ "--rm"]
      commandArgs: [ "echo", "Hello Ankaios"]
```

##### Workload specification

A workload specification consists of the following properties:

| Property | Description | Value | Is required? |
|----------|:-----------:|:------|:----------:|
| **workload name** (_as field key_) | It specifies an unique workload name to identify the workload in the Ankaios system. | A string of any characters (if properly quoted by either single quotes 'example' or double quotes "example"). | true |
| **agent** | It specifies the name of the owning agent which is going to execute the workload. | A string of any characters (if properly quoted by either single quotes 'example' or double quotes "example"). |   false |
| **tags** | It specifies a list use defined key/value objects. | A list of { `key`: some_string, `value`: some_string} or empty| true |
| **dependencies** | It specifies inter workload dependencies. | Not specified yet and shall be set to empty.| false |
| **updateStrategy** | It specifies the update strategy. | One of: `UNSPECIFIED`, `AT_LEAST_ONCE`, `AT_MOST_ONCE` | true |
| **restart** | It specifies whether the workload shall be restarted when it exits | One of: `true`,`false` | true |
| **accessRights** | It specifies lists of access rules fpr `allow` and `deny`. | Not fully specified yet and shall be set to empty list for both. | true |
| **runtime** | It specifies the type of the runtime. | One of: `podman`, `podman-kube`. | true |
| **runtimeConfig** | It specifies the configuration for the runtime whose configuration structure is specific for each runtime as a _string_ | As a _string_ from one of: [PodmanRuntimeConfig](#podmanruntimeconfig), [PodmanKubeRuntimeConfig](#podmankuberuntimeconfig),  | true |

###### PodmanRuntimeConfig

The runtime configuration for the podman runtime is specified as follows:

```YAML
generalOptions: [<comma>, <separated>, <options>]
image: <registry>/<image name>:<version>
commandOptions: [<comma>, <separated>, <options>]
commandArgs: [<comma>, <separated>, <arguments>]
```

###### PodmanKubeRuntimeConfig

The runtime configuration for the podman-kube runtime is specified as follows:

```YAML
generalOptions: [<comma>, <separated>, <options>]
play_options: [<comma>, <separated>, <options>]
down_options: [<comma>, <separated>, <options>]
manifest: <string containing the K8s manifest>
```

#### CLI supports Ankaios manifest
`swdd~cli-supports-ankaios-manifest~1`

Status: approved

The Ankaios CLI shall support the Ankaios manifest file format.

Tags:
- AnkaiosManifest

Needs:
- impl
- utest

#### CLI provides a function to accept a list of Ankaios manifest files
`swdd~cli-apply-accepts-list-of-ankaios-manifests~1`

Status: approved

**When** the user provides a list of Ankaios manifest files via the CLI command `ank apply [OPTIONS] manifest1.yaml manifest2.yaml ...`,

**Then** the Ankaios CLI shall accept the content of all the given Ankaios manifest files.

Needs:
- impl
- utest
- stest

#### CLI provides a function to accept an Ankaios manifest content from `stdin`
`swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1`

Status: approved

**When** the user provides the manifest content via the CLI command `ank apply [OPTIONS] -` through `stdin`,

**Then** the Ankaios CLI shall accept the given manifest content from `stdin`.

Needs:
- impl
- utest
- stest

#### CLI provides a function to generate a state object from Ankaios manifests
`swdd~cli-apply-generates-state-object-from-ankaios-manifests~1`

Status: approved

**Where** the user does not provide the optional argument `-d`,

**When** the Ankaios CLI accepts the manifest content from file(s) or from `stdin`,

**Then** the Ankaios CLI shall parse the manifest content into a state object.

Needs:
- impl
- utest
- stest

#### CLI provides a function to generate filter masks from Ankaios manifests
`swdd~cli-apply-generates-filter-masks-from-ankaios-manifests~1`

Status: approved

**When** the Ankaios CLI accepts the manifest content from file(s) or from `stdin`,

**Then** the Ankaios CLI shall parse the manifest content into a list of filter masks.

Needs:
- impl
- utest
- stest

#### CLI provides a function to send update state request for `ank apply ...`
`swdd~cli-apply-send-update-state~1`

Status: approved

**When** the Ankaios CLI parses the manifest content into a state object

**And** the Ankaios CLI parses the manifest content into a list of filter masks,

**Then** then Ankaios CLI shall send an update state request to the Ankaios server containing the built state object and filter mask.

Needs:
- impl
- utest
- stest

#### CLI provides a function to send update state request for `ank apply -d ...`
`swdd~cli-apply-send-update-state-for-deletion~1`

Status: approved

**Where** the user provides the optional argument `-d`,

**When** the Ankaios CLI parses the manifest content into a list of filter masks,

**Then** then Ankaios CLI shall send an update state request to the Ankaios server containing an empty state object and the filter mask.

Needs:
- impl
- utest
- stest

#### CLI provides a function to overwrite the agent names
`swdd~cli-apply-ankaios-manifest-agent-name-overwrite~1`

Status: approved

**Where** the user provides the optional argument `--agent`,

**When** the Ankaios CLI parses the manifest content into a state object,

**Then** then Ankaios CLI shall overwrite the agent names in the state object, built as specified in the manifest content, with the one given by the argument.

Needs:
- impl
- utest
- stest

#### CLI emits an error on absence of agent name
`swdd~cli-apply-ankaios-manifest-error-on-agent-name-absence~1`

Status: approved

**If** the agent name is not specified in a workload specification

**And** the user does not provide the agent name via the optional argument `--agent`,

**When** the user runs the CLI command `ank apply [OPTIONS] ...`,

**Then** the Ankaios CLI shall emit an agent name not specified error.

Needs:
- impl
- utest
- stest

## Data view

![Data view](plantuml/class_data-structures.svg)

## Error management view

## Deployment view

## References

## Glossary

<!-- markdownlint-disable-file MD004 MD022 MD032 -->
