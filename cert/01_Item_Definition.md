# Item Definition Document

## Eclipse Ankaios Workload Orchestrator

| Document Information | |
|---------------------|---|
| Document ID | ANKAIOS-ID-001 |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Classification | ASIL D (Target) |
| Author | Safety Engineering Team |

---

## Table of Contents

1. [Purpose and Scope](#1-purpose-and-scope)
2. [Item Overview](#2-item-overview)
3. [Item Boundaries and Interfaces](#3-item-boundaries-and-interfaces)
4. [Functional Description](#4-functional-description)
5. [Operating Modes and States](#5-operating-modes-and-states)
6. [Environmental Conditions](#6-environmental-conditions)
7. [Dependencies and Assumptions](#7-dependencies-and-assumptions)
8. [Legal and Regulatory Requirements](#8-legal-and-regulatory-requirements)
9. [Known Limitations](#9-known-limitations)
10. [References](#10-references)

---

## 1. Purpose and Scope

### 1.1 Document Purpose

This Item Definition document describes Eclipse Ankaios as a safety-related item within automotive High-Performance Computing (HPC) platforms according to ISO 26262:2018. It establishes the foundation for subsequent hazard analysis, safety concept development, and safety requirements derivation.

### 1.2 Scope of Item

The item encompasses the complete Ankaios workload orchestration system including:
- Ankaios Server component
- Ankaios Agent components
- Ankaios CLI (ank) component
- Communication infrastructure (gRPC)
- Control Interface mechanisms
- Runtime connectors (Podman, Podman Kube, Containerd)

### 1.3 Document Applicability

This document applies to:
- ISO 26262:2018 Part 3 compliance
- Functional safety development activities
- System architecture design
- Integration with vehicle E/E systems

---

## 2. Item Overview

### 2.1 Item Identification

| Attribute | Value |
|-----------|-------|
| Item Name | Eclipse Ankaios Workload Orchestrator |
| Item Abbreviation | ANK |
| Version | 1.0.x |
| Item Type | Software System |
| Development Organization | Eclipse Foundation / Ankaios Project |
| Target ASIL | D |

### 2.2 Item Description

Eclipse Ankaios is a workload and container orchestration system designed specifically for automotive High-Performance Computing (HPC) platforms. It provides centralized management of containerized applications across multiple computing nodes within a vehicle, enabling dynamic deployment, scaling, and lifecycle management of software workloads.

### 2.3 Primary Functions

| Function ID | Function Name | Description |
|-------------|---------------|-------------|
| F-001 | Workload Orchestration | Manage lifecycle of containerized workloads across nodes |
| F-002 | State Management | Maintain desired and actual state of all workloads |
| F-003 | Workload Scheduling | Schedule workloads based on dependencies and resources |
| F-004 | Health Monitoring | Monitor workload health and execution states |
| F-005 | Failure Recovery | Automatically restart failed workloads per policy |
| F-006 | Configuration Management | Manage workload configurations and templates |
| F-007 | Inter-Workload Dependencies | Enforce startup/shutdown ordering |
| F-008 | Resource Monitoring | Track CPU and memory usage per agent |
| F-009 | Access Control | Authorize workload operations via control interface |
| F-010 | Secure Communication | Provide mTLS-secured communication channels |

### 2.4 System Context

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           VEHICLE SYSTEM                                     │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                    HIGH-PERFORMANCE COMPUTING PLATFORM                 │  │
│  │                                                                        │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │                     MANAGED WORKLOADS                            │  │  │
│  │  │                                                                  │  │  │
│  │  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │  │  │
│  │  │  │    ADAS      │  │  Telematics  │  │  Diagnostics │          │  │  │
│  │  │  │  Workloads   │  │  Workloads   │  │  Workloads   │          │  │  │
│  │  │  │  (ASIL B-D)  │  │    (QM)      │  │  (ASIL A)    │          │  │  │
│  │  │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │  │  │
│  │  │         │                 │                 │                   │  │  │
│  │  └─────────┼─────────────────┼─────────────────┼───────────────────┘  │  │
│  │            │                 │                 │                      │  │
│  │            └─────────────────┼─────────────────┘                      │  │
│  │                              │                                        │  │
│  │  ┌───────────────────────────▼───────────────────────────────────┐   │  │
│  │  │                                                                │   │  │
│  │  │                    ANKAIOS ORCHESTRATOR                        │   │  │
│  │  │                         (ITEM)                                 │   │  │
│  │  │                                                                │   │  │
│  │  │  ┌─────────────────────────────────────────────────────────┐  │   │  │
│  │  │  │                    ANKAIOS SERVER                        │  │   │  │
│  │  │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │  │   │  │
│  │  │  │  │ State    │ │ Scheduler│ │ Event    │ │ Config   │   │  │   │  │
│  │  │  │  │ Manager  │ │          │ │ Handler  │ │ Renderer │   │  │   │  │
│  │  │  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │  │   │  │
│  │  │  └─────────────────────────────────────────────────────────┘  │   │  │
│  │  │                              │                                 │   │  │
│  │  │                         gRPC │ (mTLS)                          │   │  │
│  │  │                              │                                 │   │  │
│  │  │  ┌───────────────────────────┼───────────────────────────┐    │   │  │
│  │  │  │                           │                            │    │   │  │
│  │  │  │    ┌──────────────────────┼──────────────────────┐    │    │   │  │
│  │  │  │    │                      │                      │    │    │   │  │
│  │  │  ▼    ▼                      ▼                      ▼    │    │   │  │
│  │  │ ┌──────────┐           ┌──────────┐           ┌──────────┐   │   │  │
│  │  │ │ AGENT 1  │           │ AGENT 2  │           │ AGENT N  │   │   │  │
│  │  │ │ (Node 1) │           │ (Node 2) │           │ (Node N) │   │   │  │
│  │  │ ├──────────┤           ├──────────┤           ├──────────┤   │   │  │
│  │  │ │ Runtime  │           │ Runtime  │           │ Runtime  │   │   │  │
│  │  │ │ Connector│           │ Connector│           │ Connector│   │   │  │
│  │  │ ├──────────┤           ├──────────┤           ├──────────┤   │   │  │
│  │  │ │ Podman   │           │Containerd│           │ PodmanK  │   │   │  │
│  │  │ └──────────┘           └──────────┘           └──────────┘   │   │  │
│  │  │                                                               │   │  │
│  │  └───────────────────────────────────────────────────────────────┘   │  │
│  │                                                                       │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                  │  │
│  │  │ Ethernet    │  │ Vehicle     │  │ External    │                  │  │
│  │  │ Network     │  │ Bus (CAN)   │  │ Interfaces  │                  │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘                  │  │
│  │                                                                       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐                │
│  │ Power Supply   │  │ Vehicle        │  │ Sensors &      │                │
│  │                │  │ Network        │  │ Actuators      │                │
│  └────────────────┘  └────────────────┘  └────────────────┘                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Item Boundaries and Interfaces

### 3.1 Item Boundary Definition

#### 3.1.1 Components Within Item Boundary

| Component | Description | Included |
|-----------|-------------|----------|
| Ankaios Server | Central orchestration server | Yes |
| Ankaios Agent | Node-level workload manager | Yes |
| Ankaios CLI (ank) | Command-line interface | Yes |
| gRPC Communication | Server-Agent communication | Yes |
| Control Interface | Workload-to-Server communication | Yes |
| Runtime Connectors | Podman, Containerd, PodmanKube adapters | Yes |
| Configuration Parser | YAML manifest processing | Yes |
| State Manager | Desired/Actual state management | Yes |
| Workload Scheduler | Dependency-aware scheduling | Yes |
| Authorization Module | Access control enforcement | Yes |

#### 3.1.2 Components Outside Item Boundary

| Component | Description | Interface |
|-----------|-------------|-----------|
| Container Runtimes | Podman, Containerd | Runtime Connector API |
| Linux Operating System | Host OS for agents | System calls |
| Container Images | Workload container images | OCI Registry |
| Network Infrastructure | Physical/virtual network | TCP/IP |
| Hardware Platform | HPC compute nodes | HAL |
| Managed Workloads | Application containers | Control Interface |
| External Services | Cloud, fleet management | Network API |

### 3.2 External Interfaces

#### 3.2.1 Interface Overview

| Interface ID | Interface Name | Type | Direction | Protocol |
|--------------|----------------|------|-----------|----------|
| IF-001 | gRPC Server-Agent | Network | Bidirectional | gRPC over mTLS |
| IF-002 | Control Interface | IPC | Bidirectional | Protobuf over FIFO |
| IF-003 | CLI Interface | Process | Request-Response | gRPC |
| IF-004 | Runtime Connector | Process | Bidirectional | CLI/API |
| IF-005 | Configuration Input | File | Input | YAML |
| IF-006 | Log Output | File/Stream | Output | Text |
| IF-007 | Certificates | File | Input | PEM |

#### 3.2.2 IF-001: gRPC Server-Agent Interface

| Attribute | Value |
|-----------|-------|
| Protocol | gRPC (HTTP/2) |
| Security | mTLS (mandatory in secure mode) |
| Port | Configurable (default: 25551) |
| Serialization | Protocol Buffers |
| Connection | Persistent, bidirectional streaming |
| Timeout | Configurable reconnection timeout |

**Message Types:**

| Message | Direction | Description |
|---------|-----------|-------------|
| AgentHello | Agent → Server | Agent registration with capabilities |
| ServerHello | Server → Agent | Initial workload assignments |
| UpdateWorkload | Server → Agent | Workload create/update/delete commands |
| UpdateWorkloadState | Agent → Server | Workload state changes |
| CompleteStateRequest | Bidirectional | State query with field masks |
| CompleteStateResponse | Bidirectional | Filtered state response |

#### 3.2.3 IF-002: Control Interface

| Attribute | Value |
|-----------|-------|
| Mechanism | Named pipes (FIFO) |
| Path | `/run/ankaios/agents/{agent}/workloads/{workload}@{id}/` |
| Pipes | `input` (to Ankaios), `output` (from Ankaios) |
| Serialization | Protocol Buffers (length-prefixed) |
| Authorization | Per-workload rules |

**Message Types:**

| Message | Direction | Description |
|---------|-----------|-------------|
| ToAnkaios | Workload → Agent | Request messages |
| FromAnkaios | Agent → Workload | Response messages |
| CompleteStateRequest | Workload → Agent | State query |
| UpdateStateRequest | Workload → Agent | State modification |
| LogsRequest | Workload → Agent | Log streaming |

#### 3.2.4 IF-004: Runtime Connector Interface

| Runtime | Interface Type | Operations |
|---------|----------------|------------|
| Podman | CLI (podman) | create, start, stop, rm, logs, ps |
| Containerd | CLI (nerdctl) | create, start, stop, rm, logs, ps |
| Podman Kube | CLI (podman kube) | play, down, logs |

**Operations:**

| Operation | Description | Parameters |
|-----------|-------------|------------|
| CreateWorkload | Start container from spec | RuntimeConfig, name, mounts |
| DeleteWorkload | Stop and remove container | Container ID |
| ListWorkloads | Enumerate running containers | Filter |
| CollectLogs | Retrieve container logs | Options (follow, tail, since) |
| GetState | Check container status | Container ID |

#### 3.2.5 IF-005: Configuration Input Interface

| Attribute | Value |
|-----------|-------|
| Format | YAML |
| Schema | Ankaios Manifest Schema v1 |
| Validation | JSON Schema validation |
| Location | File path or stdin |

**Configuration Elements:**

```yaml
apiVersion: v1
workloads:
  <workload_name>:
    agent: <agent_name>
    runtime: podman|containerd|podman-kube
    runtimeConfig: <runtime_specific_config>
    restartPolicy: NEVER|ON_FAILURE|ALWAYS
    dependencies:
      <workload>: <condition>
    tags:
      <key>: <value>
    controlInterfaceAccess:
      allowRules: [...]
      denyRules: [...]
    configs:
      <alias>: <config_name>
    files:
      - mountPoint: <path>
        data: <content>
configs:
  <config_name>:
    String: <value>
```

### 3.3 Internal Interfaces

#### 3.3.1 Server Internal Interfaces

| Interface | From | To | Type |
|-----------|------|-----|------|
| StateUpdate | Event Handler | Server State | Function call |
| WorkloadCommand | Server State | gRPC Server | Channel |
| EventNotification | Server State | Event Handler | Channel |
| ConfigRender | Config Renderer | Server State | Function call |

#### 3.3.2 Agent Internal Interfaces

| Interface | From | To | Type |
|-----------|------|-----|------|
| WorkloadOperation | Scheduler | Runtime Facade | Channel |
| StateChange | Runtime Facade | State Sender | Channel |
| ControlMessage | Control Interface | Authorizer | Channel |
| LogStream | Log Fetcher | Control Interface | Channel |

---

## 4. Functional Description

### 4.1 Workload Orchestration (F-001)

#### 4.1.1 Description

Ankaios manages the complete lifecycle of containerized workloads across distributed computing nodes. The server maintains the desired state and coordinates with agents to ensure actual state matches desired state.

#### 4.1.2 Functional Behavior

| Input | Processing | Output |
|-------|------------|--------|
| Workload manifest | Parse, validate, store | Desired state update |
| Agent connection | Assign workloads | UpdateWorkload commands |
| Delete request | Remove from state | Stop/remove commands |
| Update request | Compare states | Update commands |

#### 4.1.3 Timing Requirements

| Operation | Requirement |
|-----------|-------------|
| State update propagation | < 100ms |
| Workload assignment | < 500ms after agent connect |
| Command acknowledgment | < 1000ms |

### 4.2 State Management (F-002)

#### 4.2.1 Description

Maintain consistent desired state and actual state across all components.

#### 4.2.2 State Structure

```
CompleteState
├── desiredState
│   ├── apiVersion
│   ├── workloads
│   │   └── <name>: WorkloadSpec
│   └── configs
│       └── <name>: ConfigValue
├── workloadStates
│   └── <agent>
│       └── <workload>
│           └── <instance_id>: ExecutionState
└── agents
    └── <name>: AgentAttributes
```

#### 4.2.3 State Consistency

| Invariant | Description |
|-----------|-------------|
| Single source of truth | Server holds authoritative state |
| Eventual consistency | Agents converge to desired state |
| Version tracking | API version validated on all updates |
| Cycle-free dependencies | No circular workload dependencies |

### 4.3 Workload Scheduling (F-003)

#### 4.3.1 Description

Schedule workload operations based on inter-workload dependencies and resource availability.

#### 4.3.2 Dependency Conditions

| Condition | Type | Description |
|-----------|------|-------------|
| ADD_COND_RUNNING | Add | Dependency must be running |
| ADD_COND_SUCCEEDED | Add | Dependency completed successfully |
| ADD_COND_FAILED | Add | Dependency failed |
| DEL_COND_NOT_PENDING_NOR_RUNNING | Delete | Dependency not active |
| DEL_COND_RUNNING | Delete | Dependency still running |

#### 4.3.3 Scheduling Algorithm

1. Receive workload operation command
2. Check dependency fulfillment
3. If fulfilled: execute immediately
4. If not fulfilled: enqueue operation
5. On state change: re-evaluate queued operations

### 4.4 Health Monitoring (F-004)

#### 4.4.1 Description

Continuously monitor workload health and detect failures.

#### 4.4.2 Monitoring Mechanisms

| Mechanism | Interval | Detection |
|-----------|----------|-----------|
| Container state polling | 2 seconds | State checker |
| Process exit detection | Event-driven | Runtime notification |
| Resource monitoring | 2 seconds | Agent metrics |

#### 4.4.3 Health States

| State | Healthy | Description |
|-------|---------|-------------|
| RUNNING_OK | Yes | Container running normally |
| FAILED_EXEC_FAILED | No | Container exited with error |
| FAILED_UNKNOWN | No | Unknown failure state |
| FAILED_LOST | No | Container disappeared |

### 4.5 Failure Recovery (F-005)

#### 4.5.1 Description

Automatically restart failed workloads according to configured restart policy.

#### 4.5.2 Restart Policies

| Policy | Behavior |
|--------|----------|
| NEVER | No automatic restart |
| ON_FAILURE | Restart on non-zero exit code |
| ALWAYS | Always restart regardless of exit code |

#### 4.5.3 Retry Mechanism

| Parameter | Value |
|-----------|-------|
| Initial delay | 1 second |
| Backoff multiplier | 2x |
| Maximum delay | 5 minutes |
| Jitter | Random 0-100% of delay |

### 4.6 Configuration Management (F-006)

#### 4.6.1 Description

Manage workload configurations with template rendering and config references.

#### 4.6.2 Template Processing

```yaml
# Template syntax
runtimeConfig: |
  image: {{ configs.image_name }}
  env:
    DATABASE_URL: {{ configs.db_url }}
```

#### 4.6.3 Multi-line Config Support

- Indent preservation
- Handlebars template syntax
- Runtime rendering

### 4.7 Inter-Workload Dependencies (F-007)

#### 4.7.1 Description

Enforce ordered startup and shutdown of interdependent workloads.

#### 4.7.2 Dependency Graph

```
┌─────────────┐
│  Database   │
│  (no deps)  │
└──────┬──────┘
       │ ADD_COND_RUNNING
       ▼
┌─────────────┐
│   Backend   │
│  (depends   │
│  on DB)     │
└──────┬──────┘
       │ ADD_COND_RUNNING
       ▼
┌─────────────┐
│  Frontend   │
│  (depends   │
│  on Backend)│
└─────────────┘
```

#### 4.7.3 Cycle Detection

- Iterative DFS algorithm
- Rejects cyclic dependencies
- Checked on every state update

### 4.8 Resource Monitoring (F-008)

#### 4.8.1 Description

Track resource usage (CPU, memory) per agent node.

#### 4.8.2 Metrics

| Metric | Type | Update Interval |
|--------|------|-----------------|
| CPU Usage | Percentage | 2 seconds |
| Free Memory | Bytes | 2 seconds |

#### 4.8.3 Agent Attributes

```protobuf
AgentAttributes {
  status: AgentStatus {
    cpu_usage: CpuUsage,
    free_memory: FreeMemory
  }
  tags: Tags
}
```

### 4.9 Access Control (F-009)

#### 4.9.1 Description

Authorize workload access to Ankaios state and operations.

#### 4.9.2 Authorization Model

| Rule Type | Effect | Scope |
|-----------|--------|-------|
| AllowRule | Permit | State paths, log access |
| DenyRule | Deny | State paths, log access |
| Default | Deny | All operations |

#### 4.9.3 Filter Masks

```
desiredState.workloads.*           # All workloads
desiredState.workloads.myapp       # Specific workload
workloadStates.**                  # All states recursively
```

### 4.10 Secure Communication (F-010)

#### 4.10.1 Description

Provide TLS-secured communication between all components.

#### 4.10.2 Security Modes

| Mode | Configuration | Security |
|------|---------------|----------|
| mTLS | Certificates provided | Full mutual authentication |
| Insecure | --insecure flag | No encryption (development only) |

#### 4.10.3 Certificate Requirements

| Certificate | Purpose |
|-------------|---------|
| CA Certificate | Trust anchor |
| Server Certificate | Server identity |
| Server Key | Server private key |
| Client Certificate | Agent/CLI identity |
| Client Key | Agent/CLI private key |

---

## 5. Operating Modes and States

### 5.1 System Operating Modes

#### 5.1.1 Normal Operation Mode

| Aspect | Description |
|--------|-------------|
| Trigger | System startup complete |
| Server State | Running, accepting connections |
| Agent State | Connected, executing workloads |
| Communication | mTLS active |
| Functions | All functions available |

#### 5.1.2 Degraded Operation Mode

| Aspect | Description |
|--------|-------------|
| Trigger | Agent disconnection, workload failures |
| Server State | Running with reduced capacity |
| Agent State | Partial connectivity |
| Communication | Reconnection attempts active |
| Functions | Limited based on available agents |

#### 5.1.3 Startup Mode

| Aspect | Description |
|--------|-------------|
| Trigger | System power-on |
| Server State | Initializing |
| Agent State | Connecting |
| Communication | Establishing connections |
| Functions | Configuration loading |

#### 5.1.4 Shutdown Mode

| Aspect | Description |
|--------|-------------|
| Trigger | Shutdown command |
| Server State | Terminating |
| Agent State | Stopping workloads |
| Communication | Graceful disconnection |
| Functions | Cleanup only |

### 5.2 Workload Execution States

#### 5.2.1 State Diagram

```
                                    ┌─────────────────┐
                                    │ NOT_SCHEDULED   │
                                    │ (no agent)      │
                                    └────────┬────────┘
                                             │ Agent assigned
                                             ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           PENDING STATES                                 │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐   │
│  │ PENDING_INITIAL │────▶│ PENDING_WAITING │────▶│ PENDING_STARTING│   │
│  │                 │     │   _TO_START     │     │                 │   │
│  └─────────────────┘     └─────────────────┘     └────────┬────────┘   │
│                                                           │             │
│                          ┌─────────────────┐              │             │
│                          │ PENDING_STARTING│◀─────────────┘             │
│                          │     _FAILED     │    Runtime failure         │
│                          └─────────────────┘                            │
└─────────────────────────────────────────────────────────────────────────┘
                                             │
                                             │ Started successfully
                                             ▼
                              ┌─────────────────────────┐
                              │      RUNNING_OK         │
                              │                         │
                              └───────────┬─────────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
                    ▼                     ▼                     ▼
         ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
         │    SUCCEEDED    │   │     FAILED      │   │    STOPPING     │
         │ (exit code 0)   │   │ (error/lost)    │   │                 │
         └─────────────────┘   └─────────────────┘   └────────┬────────┘
                                                              │
                                                              ▼
                                                   ┌─────────────────┐
                                                   │     REMOVED     │
                                                   │                 │
                                                   └─────────────────┘
```

#### 5.2.2 State Definitions

| State | Substate | Description |
|-------|----------|-------------|
| NOT_SCHEDULED | - | No agent assigned |
| AGENT_DISCONNECTED | - | Agent connection lost |
| PENDING | INITIAL | Created, not yet scheduled |
| PENDING | WAITING_TO_START | Waiting for dependencies |
| PENDING | STARTING | Sent to runtime |
| PENDING | STARTING_FAILED | Runtime creation failed |
| RUNNING | OK | Executing normally |
| STOPPING | - | Deletion in progress |
| STOPPING | WAITING_TO_STOP | Waiting for dependents |
| STOPPING | REQUESTED_AT_RUNTIME | Deletion sent to runtime |
| STOPPING | DELETE_FAILED | Runtime deletion failed |
| SUCCEEDED | - | Completed with exit code 0 |
| FAILED | EXEC_FAILED | Runtime error |
| FAILED | UNKNOWN | Unknown failure |
| FAILED | LOST | Container disappeared |
| REMOVED | - | Deleted (internal state) |

### 5.3 Agent Connection States

| State | Description | Server Behavior |
|-------|-------------|-----------------|
| DISCONNECTED | No connection | Mark workloads as AGENT_DISCONNECTED |
| CONNECTING | Handshake in progress | Wait for AgentHello |
| CONNECTED | Active connection | Normal operation |
| RECONNECTING | Connection lost, retrying | Preserve workload state |

### 5.4 Server States

| State | Description |
|-------|-------------|
| INITIALIZING | Loading configuration |
| READY | Accepting connections |
| RUNNING | Normal operation |
| SHUTTING_DOWN | Graceful shutdown |
| ERROR | Unrecoverable error |

---

## 6. Environmental Conditions

### 6.1 Operating Environment

#### 6.1.1 Hardware Platform Requirements

| Parameter | Requirement |
|-----------|-------------|
| Processor | ARM64 or x86_64 |
| Memory | Minimum 512MB per agent |
| Storage | Minimum 1GB for container images |
| Network | Ethernet (100Mbps minimum) |

#### 6.1.2 Software Platform Requirements

| Component | Requirement |
|-----------|-------------|
| Operating System | Linux (kernel 4.18+) |
| Container Runtime | Podman 4.0+ or Containerd 1.6+ |
| libc | glibc 2.17+ or musl |

### 6.2 Physical Environment

#### 6.2.1 Temperature

| Parameter | Range |
|-----------|-------|
| Operating | -40°C to +85°C (automotive grade) |
| Storage | -40°C to +125°C |

#### 6.2.2 Other Conditions

| Parameter | Requirement |
|-----------|-------------|
| Humidity | 0% to 100% RH |
| Vibration | Per ISO 16750-3 |
| EMC | Per ISO 11452 / ISO 7637 |

### 6.3 Network Environment

| Parameter | Requirement |
|-----------|-------------|
| Latency | < 10ms intra-vehicle |
| Bandwidth | 100Mbps minimum |
| Reliability | 99.9% availability |
| Topology | Switched Ethernet |

---

## 7. Dependencies and Assumptions

### 7.1 System Dependencies

| Dependency | Type | Description |
|------------|------|-------------|
| Linux Kernel | Hard | Requires Linux 4.18+ |
| Container Runtime | Hard | Podman or Containerd required |
| Network Stack | Hard | TCP/IP networking |
| Filesystem | Hard | Writable filesystem for state |
| Time Source | Hard | System clock for timestamps |

### 7.2 External Dependencies

| Dependency | Type | Description |
|------------|------|-------------|
| Container Registry | Soft | For pulling images |
| DNS | Soft | For resolving hostnames |
| NTP | Soft | For time synchronization |
| PKI | Conditional | For mTLS certificates |

### 7.3 Assumptions

| ID | Assumption | Rationale |
|----|------------|-----------|
| A-001 | Network is available at startup | Required for agent registration |
| A-002 | Container runtime is operational | Required for workload execution |
| A-003 | Sufficient resources exist | CPU/memory for workloads |
| A-004 | Certificates are valid | For mTLS communication |
| A-005 | System time is synchronized | For log correlation |
| A-006 | Filesystem is persistent | For configuration storage |
| A-007 | Power supply is stable | Graceful shutdown assumed |

---

## 8. Legal and Regulatory Requirements

### 8.1 Applicable Standards

| Standard | Applicability |
|----------|--------------|
| ISO 26262:2018 | Functional Safety |
| ISO/SAE 21434:2021 | Cybersecurity |
| UN R155 | Cybersecurity Management |
| UN R156 | Software Update Management |
| GDPR | Data protection (if applicable) |

### 8.2 Certification Requirements

| Requirement | Description |
|-------------|-------------|
| ASIL Capability | Target ASIL D |
| Tool Qualification | Per ISO 26262 Part 8 |
| Process Assessment | ISO 26262 Part 2 |

### 8.3 Legal Constraints

| Constraint | Description |
|------------|-------------|
| Open Source License | Apache License 2.0 |
| Export Control | Check for cryptography |
| Data Privacy | No personal data processing |

---

## 9. Known Limitations

### 9.1 Functional Limitations

| ID | Limitation | Impact |
|----|------------|--------|
| L-001 | Single server architecture | No server redundancy |
| L-002 | No persistent state storage | State lost on server restart |
| L-003 | Container runtimes required | Cannot run bare processes |
| L-004 | Linux-only support | No Windows/RTOS support |
| L-005 | Eventual consistency | Temporary state divergence possible |

### 9.2 Performance Limitations

| ID | Limitation | Value |
|----|------------|-------|
| L-006 | Maximum agents | ~100 (tested) |
| L-007 | Maximum workloads per agent | ~1000 |
| L-008 | State update latency | 10-100ms typical |
| L-009 | Channel capacity | 20 messages |

### 9.3 Security Limitations

| ID | Limitation | Impact |
|----|------------|--------|
| L-010 | No key rotation | Manual certificate renewal |
| L-011 | No audit logging | Limited forensics |
| L-012 | Single authentication method | mTLS only |

---

## 10. References

### 10.1 Project Documentation

| Document | Location |
|----------|----------|
| Architecture Overview | doc/docs/architecture.md |
| API Reference | ankaios_api/proto/*.proto |
| User Guide | doc/docs/usage/ |
| Development Guide | DEVELOPMENT.md |

### 10.2 Standards

| Standard | Title |
|----------|-------|
| ISO 26262:2018 | Road vehicles - Functional safety |
| ISO/SAE 21434:2021 | Road vehicles - Cybersecurity engineering |
| ISO 11898 | CAN specification |
| ISO 14229 | UDS specification |

### 10.3 Related Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-HARA-001 | Hazard Analysis and Risk Assessment |
| ANKAIOS-FMEA-001 | Failure Mode and Effects Analysis |
| ANKAIOS-FSR-001 | Functional Safety Requirements |
| ANKAIOS-TSR-001 | Technical Safety Requirements |
| ANKAIOS-CSR-001 | Cybersecurity Requirements |

---

## Appendix A: Glossary

| Term | Definition |
|------|------------|
| Agent | Ankaios component running on each node |
| ASIL | Automotive Safety Integrity Level |
| Control Interface | Workload-to-Ankaios communication mechanism |
| Desired State | User-specified workload configuration |
| HPC | High-Performance Computing |
| mTLS | Mutual TLS authentication |
| Runtime Connector | Adapter for container runtimes |
| Server | Central Ankaios orchestration component |
| Workload | Containerized application managed by Ankaios |

---

## Appendix B: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Safety Team | Initial release |

---

*Document approved for ISO 26262 compliance activities.*
