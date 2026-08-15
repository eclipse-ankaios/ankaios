# Technical Safety Requirements (TSR)

## Eclipse Ankaios Workload Orchestrator

| Document Information | |
|---------------------|---|
| Document ID | ANKAIOS-TSR-001 |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Related FSR | ANKAIOS-FSR-001 |
| Author | Safety Engineering Team |

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Requirements Overview](#2-requirements-overview)
3. [Workload Lifecycle TSRs](#3-workload-lifecycle-tsrs)
4. [State Management TSRs](#4-state-management-tsrs)
5. [Failure Detection and Recovery TSRs](#5-failure-detection-and-recovery-tsrs)
6. [Cascade Prevention TSRs](#6-cascade-prevention-tsrs)
7. [Dependency Management TSRs](#7-dependency-management-tsrs)
8. [Security TSRs](#8-security-tsrs)
9. [Communication TSRs](#9-communication-tsrs)
10. [Resource Management TSRs](#10-resource-management-tsrs)
11. [Health Monitoring TSRs](#11-health-monitoring-tsrs)
12. [Hardware-Software Interface](#12-hardware-software-interface)
13. [Traceability](#13-traceability)
14. [References](#14-references)

---

## 1. Introduction

### 1.1 Purpose

This document specifies the Technical Safety Requirements (TSRs) for Eclipse Ankaios derived from the Functional Safety Requirements. TSRs provide implementation-level specifications for achieving safety goals.

### 1.2 Scope

These requirements specify:
- Implementation timing constraints
- Data integrity mechanisms
- Communication protocols
- Error handling behaviors
- Hardware-software interfaces

### 1.3 Requirements Notation

| Notation | Meaning |
|----------|---------|
| **shall** | Mandatory requirement |
| **should** | Recommended requirement |
| **may** | Optional requirement |
| [ASIL X] | ASIL level of requirement |
| {FSR-XXX} | Source functional requirement |

---

## 2. Requirements Overview

### 2.1 TSR Categories

| Category | TSR Range | Count |
|----------|-----------|-------|
| Workload Lifecycle | TSR-001 to TSR-013 | 13 |
| State Management | TSR-014 to TSR-019 | 6 |
| Failure Detection/Recovery | TSR-020 to TSR-025 | 6 |
| Cascade Prevention | TSR-026 to TSR-030 | 5 |
| Dependency Management | TSR-031 to TSR-034 | 4 |
| Security | TSR-035 to TSR-038 | 4 |
| Communication | TSR-039 to TSR-043 | 5 |
| Resource Management | TSR-044 to TSR-046 | 3 |
| Health Monitoring | TSR-047 to TSR-049 | 3 |
| **Total** | | **49** |

---

## 3. Workload Lifecycle TSRs

### TSR-001: Start Failure Polling Interval

| Attribute | Value |
|-----------|-------|
| ID | TSR-001 |
| Title | Start Failure Polling Interval |
| Description | The state checker **shall** poll container runtime status every 50ms during workload startup to detect failures within 100ms. |
| ASIL | D |
| Source | {FSR-001} |
| Allocation | Agent State Checker (SC) |
| Verification | Unit Test: Verify polling interval ≤ 50ms |

### TSR-002: Start Event Notification

| Attribute | Value |
|-----------|-------|
| ID | TSR-002 |
| Title | Start Event Notification |
| Description | The agent **shall** send UpdateWorkloadState message to server within 50ms of detecting start failure. |
| ASIL | D |
| Source | {FSR-001} |
| Allocation | Agent gRPC Client (GC) |
| Verification | Integration Test: Measure notification latency |

### TSR-003: Retry Counter Implementation

| Attribute | Value |
|-----------|-------|
| ID | TSR-003 |
| Title | Retry Counter Implementation |
| Description | The workload controller **shall** maintain a retry counter per workload with maximum value of 3, reset on successful start. |
| ASIL | D |
| Source | {FSR-002} |
| Allocation | Agent Workload Controller (WC) |
| Verification | Unit Test: Counter behavior |

### TSR-004: Retry Backoff Calculation

| Attribute | Value |
|-----------|-------|
| ID | TSR-004 |
| Title | Retry Backoff Calculation |
| Description | The retry delay **shall** be calculated as: `delay = min(initial_delay × 2^(retry_count-1) + jitter, 300s)` where jitter is random 0-50% of delay. |
| ASIL | D |
| Source | {FSR-002} |
| Allocation | Agent Retry Manager |
| Verification | Unit Test: Delay calculation |

### TSR-005: Permanent Failure Event

| Attribute | Value |
|-----------|-------|
| ID | TSR-005 |
| Title | Permanent Failure Event |
| Description | Upon exhausting retries, the agent **shall** emit a FAILED state with substate EXEC_FAILED and reason string within 50ms. |
| ASIL | D |
| Source | {FSR-003} |
| Allocation | Agent Workload Controller (WC) |
| Verification | Integration Test: Event content and timing |

### TSR-006: Startup Timeout Timer

| Attribute | Value |
|-----------|-------|
| ID | TSR-006 |
| Title | Startup Timeout Timer |
| Description | The workload controller **shall** start a monotonic timer upon issuing container create command, triggering failure on expiry. |
| ASIL | D |
| Source | {FSR-004} |
| Allocation | Agent Workload Controller (WC) |
| Verification | Unit Test: Timer behavior |

### TSR-007: State Timestamp Precision

| Attribute | Value |
|-----------|-------|
| ID | TSR-007 |
| Title | State Timestamp Precision |
| Description | All workload state changes **shall** include timestamp with millisecond precision from monotonic clock source. |
| ASIL | D |
| Source | {FSR-005} |
| Allocation | Agent State Sender |
| Verification | Unit Test: Timestamp format |

### TSR-008: Termination Command Validation

| Attribute | Value |
|-----------|-------|
| ID | TSR-008 |
| Title | Termination Command Validation |
| Description | The agent **shall** verify termination commands originate from authenticated server connection before processing. |
| ASIL | D |
| Source | {FSR-006} |
| Allocation | Agent gRPC Client (GC) |
| Verification | Security Test: Source validation |

### TSR-009: Control Interface Authorization Check

| Attribute | Value |
|-----------|-------|
| ID | TSR-009 |
| Title | Control Interface Authorization Check |
| Description | For control interface delete requests, the authorizer **shall** verify the requesting workload has write permission for target workload. |
| ASIL | D |
| Source | {FSR-006} |
| Allocation | Agent Authorizer (AUTH) |
| Verification | Security Test: Permission check |

### TSR-010: Termination Audit Record

| Attribute | Value |
|-----------|-------|
| ID | TSR-010 |
| Title | Termination Audit Record |
| Description | The agent **shall** write termination log record with format: `[timestamp] TERMINATE workload={name} source={identity} authorized={bool}`. |
| ASIL | D |
| Source | {FSR-007} |
| Allocation | Agent Logger |
| Verification | Review: Log format |

### TSR-011: SIGTERM Signal Delivery

| Attribute | Value |
|-----------|-------|
| ID | TSR-011 |
| Title | SIGTERM Signal Delivery |
| Description | Upon termination request, the runtime connector **shall** send SIGTERM to container main process via runtime API. |
| ASIL | D |
| Source | {FSR-008} |
| Allocation | Runtime Connectors (PC, CDC, PKC) |
| Verification | Integration Test: Signal delivery |

### TSR-012: Grace Period Timer

| Attribute | Value |
|-----------|-------|
| ID | TSR-012 |
| Title | Grace Period Timer |
| Description | The runtime connector **shall** wait up to 10 seconds (configurable) after SIGTERM before sending SIGKILL. |
| ASIL | D |
| Source | {FSR-008} |
| Allocation | Runtime Connectors (PC, CDC, PKC) |
| Verification | Integration Test: Grace period timing |

### TSR-013: Termination Verification

| Attribute | Value |
|-----------|-------|
| ID | TSR-013 |
| Title | Termination Verification |
| Description | The state checker **shall** verify container absence via runtime list API before reporting STOPPED state. |
| ASIL | D |
| Source | {FSR-009} |
| Allocation | Agent State Checker (SC) |
| Verification | Integration Test: Verification behavior |

---

## 4. State Management TSRs

### TSR-014: State Schema Definition

| Attribute | Value |
|-----------|-------|
| ID | TSR-014 |
| Title | State Schema Definition |
| Description | State data **shall** conform to protobuf schemas defined in ank_base.proto with all required fields populated. |
| ASIL | D |
| Source | {FSR-010} |
| Allocation | All Components |
| Verification | Unit Test: Schema compliance |

### TSR-015: State Field Validation

| Attribute | Value |
|-----------|-------|
| ID | TSR-015 |
| Title | State Field Validation |
| Description | The state manager **shall** validate: field name length ≤ 63 chars, allowed characters [a-zA-Z0-9_-], no empty strings. |
| ASIL | D |
| Source | {FSR-010} |
| Allocation | Server State Manager (SM) |
| Verification | Unit Test: Validation rules |

### TSR-016: CRC-32 Implementation

| Attribute | Value |
|-----------|-------|
| ID | TSR-016 |
| Title | CRC-32 Implementation |
| Description | State transmissions **shall** include CRC-32C (Castagnoli) checksum calculated over serialized protobuf payload. |
| ASIL | D |
| Source | {FSR-011} |
| Allocation | gRPC Server (GS), gRPC Client (GC) |
| Verification | Unit Test: CRC calculation |

### TSR-017: State Machine Transitions

| Attribute | Value |
|-----------|-------|
| ID | TSR-017 |
| Title | State Machine Transitions |
| Description | The workload controller **shall** only allow transitions defined in state transition table; invalid transitions rejected with error. |
| ASIL | D |
| Source | {FSR-012} |
| Allocation | Agent Workload Controller (WC) |
| Verification | Unit Test: Transition validation |

**State Transition Table:**

| From State | Allowed To States |
|------------|-------------------|
| PENDING_INITIAL | PENDING_WAITING_TO_START, PENDING_STARTING |
| PENDING_WAITING_TO_START | PENDING_STARTING, REMOVED |
| PENDING_STARTING | RUNNING_OK, PENDING_STARTING_FAILED |
| PENDING_STARTING_FAILED | PENDING_STARTING (retry), FAILED_EXEC_FAILED |
| RUNNING_OK | STOPPING, FAILED_EXEC_FAILED, SUCCEEDED, FAILED_LOST |
| STOPPING | STOPPING_WAITING_TO_STOP, STOPPING_REQUESTED_AT_RUNTIME |
| STOPPING_WAITING_TO_STOP | STOPPING_REQUESTED_AT_RUNTIME, REMOVED |
| STOPPING_REQUESTED_AT_RUNTIME | REMOVED, STOPPING_DELETE_FAILED |
| STOPPING_DELETE_FAILED | STOPPING_REQUESTED_AT_RUNTIME (retry), REMOVED |
| SUCCEEDED | REMOVED, PENDING_STARTING (restart) |
| FAILED_* | REMOVED, PENDING_STARTING (restart) |

### TSR-018: Atomic State Write

| Attribute | Value |
|-----------|-------|
| ID | TSR-018 |
| Title | Atomic State Write |
| Description | The state manager **shall** use Rust's RwLock or equivalent to ensure state writes complete atomically before readers observe changes. |
| ASIL | D |
| Source | {FSR-013} |
| Allocation | Server State Manager (SM) |
| Verification | Concurrency Test: Atomicity |

### TSR-019: Consistency Check Implementation

| Attribute | Value |
|-----------|-------|
| ID | TSR-019 |
| Title | Consistency Check Implementation |
| Description | The server **shall** compare desired vs actual state for each workload every 5 seconds and log discrepancies. |
| ASIL | D |
| Source | {FSR-014} |
| Allocation | Server State Manager (SM) |
| Verification | Integration Test: Check interval |

---

## 5. Failure Detection and Recovery TSRs

### TSR-020: Runtime Polling Interval

| Attribute | Value |
|-----------|-------|
| ID | TSR-020 |
| Title | Runtime Polling Interval |
| Description | The state checker **shall** poll runtime container status every 200ms for running workloads. |
| ASIL | D |
| Source | {FSR-015} |
| Allocation | Agent State Checker (SC) |
| Verification | Unit Test: Polling interval |

### TSR-021: Container Exit Detection

| Attribute | Value |
|-----------|-------|
| ID | TSR-021 |
| Title | Container Exit Detection |
| Description | Upon detecting container not in running state, the state checker **shall** query exit code and report failure within 100ms. |
| ASIL | D |
| Source | {FSR-015} |
| Allocation | Agent State Checker (SC) |
| Verification | Integration Test: Detection timing |

### TSR-022: Recovery Command Priority

| Attribute | Value |
|-----------|-------|
| ID | TSR-022 |
| Title | Recovery Command Priority |
| Description | Restart commands **shall** be processed with high priority, bypassing normal scheduler queue. |
| ASIL | D |
| Source | {FSR-016} |
| Allocation | Agent Workload Scheduler (WS) |
| Verification | Unit Test: Priority handling |

### TSR-023: Restart Policy Lookup

| Attribute | Value |
|-----------|-------|
| ID | TSR-023 |
| Title | Restart Policy Lookup |
| Description | Upon workload failure, the controller **shall** lookup restart policy from workload spec and apply: NEVER (no action), ON_FAILURE (restart if exit code ≠ 0), ALWAYS (restart). |
| ASIL | D |
| Source | {FSR-017} |
| Allocation | Agent Workload Controller (WC) |
| Verification | Unit Test: Policy application |

### TSR-024: Backoff State Machine

| Attribute | Value |
|-----------|-------|
| ID | TSR-024 |
| Title | Backoff State Machine |
| Description | The retry manager **shall** maintain backoff state per workload with: current delay, retry count, last attempt timestamp. |
| ASIL | D |
| Source | {FSR-018} |
| Allocation | Agent Retry Manager |
| Verification | Unit Test: State management |

### TSR-025: Recovery Status Message

| Attribute | Value |
|-----------|-------|
| ID | TSR-025 |
| Title | Recovery Status Message |
| Description | Recovery status **shall** be sent as UpdateWorkloadState with additional_info containing: `retry_count`, `next_retry_time`, `failure_reason`. |
| ASIL | D |
| Source | {FSR-019} |
| Allocation | Agent State Sender |
| Verification | Unit Test: Message content |

---

## 6. Cascade Prevention TSRs

### TSR-026: Process Isolation

| Attribute | Value |
|-----------|-------|
| ID | TSR-026 |
| Title | Process Isolation |
| Description | Each workload **shall** run in separate container with independent PID namespace as enforced by runtime. |
| ASIL | D |
| Source | {FSR-020} |
| Allocation | Runtime Connectors (PC, CDC, PKC) |
| Verification | Integration Test: Namespace isolation |

### TSR-027: Resource Limit Configuration

| Attribute | Value |
|-----------|-------|
| ID | TSR-027 |
| Title | Resource Limit Configuration |
| Description | The runtime connector **shall** configure container cgroup limits for CPU (millicores) and memory (bytes) from workload spec. |
| ASIL | D |
| Source | {FSR-021} |
| Allocation | Runtime Connectors (PC, CDC, PKC) |
| Verification | Integration Test: Limit enforcement |

### TSR-028: Per-Agent Channel Isolation

| Attribute | Value |
|-----------|-------|
| ID | TSR-028 |
| Title | Per-Agent Channel Isolation |
| Description | The gRPC server **shall** maintain separate bidirectional streams per agent such that one agent's failure does not affect others. |
| ASIL | D |
| Source | {FSR-022} |
| Allocation | Server gRPC Server (GS) |
| Verification | Fault Injection Test: Channel isolation |

### TSR-029: State Update Rejection

| Attribute | Value |
|-----------|-------|
| ID | TSR-029 |
| Title | State Update Rejection |
| Description | Upon validation failure, the state manager **shall** reject the entire update and return UpdateStateError without modifying state. |
| ASIL | D |
| Source | {FSR-023} |
| Allocation | Server State Manager (SM) |
| Verification | Unit Test: Rejection behavior |

### TSR-030: Dependency Wait State

| Attribute | Value |
|-----------|-------|
| ID | TSR-030 |
| Title | Dependency Wait State |
| Description | When dependency fails, dependent workload **shall** remain in PENDING_WAITING_TO_START until dependency recovers or is removed. |
| ASIL | D |
| Source | {FSR-024} |
| Allocation | Agent Workload Scheduler (WS) |
| Verification | Integration Test: Wait behavior |

---

## 7. Dependency Management TSRs

### TSR-031: Cycle Detection Algorithm

| Attribute | Value |
|-----------|-------|
| ID | TSR-031 |
| Title | Cycle Detection Algorithm |
| Description | The server **shall** implement iterative DFS cycle detection traversing all dependency edges before accepting state update. |
| ASIL | C |
| Source | {FSR-025} |
| Allocation | Server Cycle Check (CC) |
| Verification | Unit Test: Algorithm correctness |

### TSR-032: Add Condition Evaluation

| Attribute | Value |
|-----------|-------|
| ID | TSR-032 |
| Title | Add Condition Evaluation |
| Description | The scheduler **shall** evaluate add conditions as: ADD_COND_RUNNING (state=RUNNING_OK), ADD_COND_SUCCEEDED (state=SUCCEEDED), ADD_COND_FAILED (state=FAILED_*). |
| ASIL | C |
| Source | {FSR-026} |
| Allocation | Agent Dependency State Validator |
| Verification | Unit Test: Condition evaluation |

### TSR-033: Delete Condition Evaluation

| Attribute | Value |
|-----------|-------|
| ID | TSR-033 |
| Title | Delete Condition Evaluation |
| Description | The scheduler **shall** evaluate delete conditions as: DEL_COND_NOT_PENDING_NOR_RUNNING (state not in PENDING_*, RUNNING_*), DEL_COND_RUNNING (state=RUNNING_OK). |
| ASIL | C |
| Source | {FSR-027} |
| Allocation | Agent Dependency State Validator |
| Verification | Unit Test: Condition evaluation |

### TSR-034: Deadlock Detection Timer

| Attribute | Value |
|-----------|-------|
| ID | TSR-034 |
| Title | Deadlock Detection Timer |
| Description | If workload remains in PENDING_WAITING_TO_START for >60 seconds with no dependency state change, scheduler **shall** report potential deadlock. |
| ASIL | C |
| Source | {FSR-028} |
| Allocation | Agent Workload Scheduler (WS) |
| Verification | Integration Test: Deadlock detection |

---

## 8. Security TSRs

### TSR-035: TLS Version Requirement

| Attribute | Value |
|-----------|-------|
| ID | TSR-035 |
| Title | TLS Version Requirement |
| Description | All TLS connections **shall** use TLS 1.3 or TLS 1.2 with approved cipher suites; TLS 1.1 and below **shall** be rejected. |
| ASIL | C |
| Source | {FSR-029} |
| Allocation | gRPC Server (GS), gRPC Client (GC) |
| Verification | Security Test: TLS version |

**Approved TLS 1.2 Cipher Suites:**
- TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
- TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
- TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
- TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256

### TSR-036: Authorization Rule Processing

| Attribute | Value |
|-----------|-------|
| ID | TSR-036 |
| Title | Authorization Rule Processing |
| Description | The authorizer **shall** process rules in order: (1) check all deny rules, (2) check all allow rules; deny if no allow matches. |
| ASIL | C |
| Source | {FSR-030} |
| Allocation | Agent Authorizer (AUTH) |
| Verification | Unit Test: Rule processing order |

### TSR-037: Authorization Log Format

| Attribute | Value |
|-----------|-------|
| ID | TSR-037 |
| Title | Authorization Log Format |
| Description | Authorization decisions **shall** be logged as: `[timestamp] AUTH decision={allow|deny} workload={name} request={type} path={filter}`. |
| ASIL | C |
| Source | {FSR-031} |
| Allocation | Agent Authorizer (AUTH) |
| Verification | Review: Log format |

### TSR-038: Certificate Validation Steps

| Attribute | Value |
|-----------|-------|
| ID | TSR-038 |
| Title | Certificate Validation Steps |
| Description | Certificate validation **shall** verify: (1) signature chain to trusted CA, (2) not expired, (3) not before date valid, (4) key usage includes digitalSignature. |
| ASIL | C |
| Source | {FSR-032} |
| Allocation | gRPC Server (GS), gRPC Client (GC) |
| Verification | Security Test: Certificate validation |

---

## 9. Communication TSRs

### TSR-039: Heartbeat Interval

| Attribute | Value |
|-----------|-------|
| ID | TSR-039 |
| Title | Heartbeat Interval |
| Description | The gRPC client **shall** send heartbeat messages every 100ms to the server. |
| ASIL | B |
| Source | {FSR-033} |
| Allocation | Agent gRPC Client (GC) |
| Verification | Integration Test: Heartbeat interval |

### TSR-040: Heartbeat Timeout Threshold

| Attribute | Value |
|-----------|-------|
| ID | TSR-040 |
| Title | Heartbeat Timeout Threshold |
| Description | The server **shall** declare agent disconnected after 3 consecutive missed heartbeats (300ms total). |
| ASIL | B |
| Source | {FSR-033} |
| Allocation | Server gRPC Server (GS) |
| Verification | Integration Test: Timeout behavior |

### TSR-041: Offline Operation Duration

| Attribute | Value |
|-----------|-------|
| ID | TSR-041 |
| Title | Offline Operation Duration |
| Description | Upon losing server connection, the agent **shall** continue executing current workloads for 10 seconds before entering offline mode. |
| ASIL | B |
| Source | {FSR-034} |
| Allocation | Agent Agent Manager |
| Verification | Integration Test: Offline behavior |

### TSR-042: Reconnection Backoff

| Attribute | Value |
|-----------|-------|
| ID | TSR-042 |
| Title | Reconnection Backoff |
| Description | The gRPC client **shall** attempt reconnection with delays: 1s, 2s, 4s, 8s, 16s, 32s, 60s (max). |
| ASIL | B |
| Source | {FSR-035} |
| Allocation | Agent gRPC Client (GC) |
| Verification | Integration Test: Backoff timing |

### TSR-043: State Sync Protocol

| Attribute | Value |
|-----------|-------|
| ID | TSR-043 |
| Title | State Sync Protocol |
| Description | Upon reconnection, agent **shall**: (1) send AgentHello, (2) receive ServerHello with current workloads, (3) reconcile local state with server state. |
| ASIL | B |
| Source | {FSR-036} |
| Allocation | Agent Agent Manager, Server |
| Verification | Integration Test: Sync behavior |

---

## 10. Resource Management TSRs

### TSR-044: Resource Measurement Implementation

| Attribute | Value |
|-----------|-------|
| ID | TSR-044 |
| Title | Resource Measurement Implementation |
| Description | The resource monitor **shall** read `/proc/stat` for CPU and `/proc/meminfo` for memory every 2 seconds. |
| ASIL | A |
| Source | {FSR-037} |
| Allocation | Agent Resource Monitor (RES) |
| Verification | Unit Test: Measurement sources |

### TSR-045: Threshold Notification

| Attribute | Value |
|-----------|-------|
| ID | TSR-045 |
| Title | Threshold Notification |
| Description | When CPU >80% or free memory <20% of total, the agent **shall** include warning flag in agent status update. |
| ASIL | A |
| Source | {FSR-038} |
| Allocation | Agent Resource Monitor (RES) |
| Verification | Unit Test: Threshold behavior |

### TSR-046: Runtime Limit Passthrough

| Attribute | Value |
|-----------|-------|
| ID | TSR-046 |
| Title | Runtime Limit Passthrough |
| Description | The runtime connector **shall** pass resource limits to runtime CLI/API: `--memory` for bytes, `--cpus` for cores. |
| ASIL | A |
| Source | {FSR-039} |
| Allocation | Runtime Connectors (PC, CDC, PKC) |
| Verification | Integration Test: Limit passthrough |

---

## 11. Health Monitoring TSRs

### TSR-047: Consecutive Failure Count

| Attribute | Value |
|-----------|-------|
| ID | TSR-047 |
| Title | Consecutive Failure Count |
| Description | The state checker **shall** maintain consecutive failure counter per workload, reset on successful check, increment on failure. |
| ASIL | A |
| Source | {FSR-040} |
| Allocation | Agent State Checker (SC) |
| Verification | Unit Test: Counter behavior |

### TSR-048: Exit Code Interpretation

| Attribute | Value |
|-----------|-------|
| ID | TSR-048 |
| Title | Exit Code Interpretation |
| Description | The workload controller **shall** interpret exit codes: 0=success, 1-125=failure, 126=not executable, 127=not found, 128+N=signal N. |
| ASIL | A |
| Source | {FSR-041} |
| Allocation | Agent Workload Controller (WC) |
| Verification | Unit Test: Exit code mapping |

### TSR-049: Restart Reason Structure

| Attribute | Value |
|-----------|-------|
| ID | TSR-049 |
| Title | Restart Reason Structure |
| Description | Restart reason **shall** be logged as: `RESTART workload={name} reason={exit_code|signal|health_check} value={code|signal_num|check_count}`. |
| ASIL | A |
| Source | {FSR-042} |
| Allocation | Agent Logger |
| Verification | Review: Log format |

---

## 12. Hardware-Software Interface

### 12.1 Timer Interface

| HSI ID | Interface | Description | Safety Relevance |
|--------|-----------|-------------|------------------|
| HSI-001 | Monotonic Clock | System monotonic clock for timing | Accurate timeouts |
| HSI-002 | RTC Clock | Real-time clock for timestamps | Log correlation |

**Requirements:**
- TSR-HSI-001: The system **shall** use monotonic clock source for all internal timing operations.
- TSR-HSI-002: Monotonic clock resolution **shall** be ≤ 1ms.

### 12.2 Network Interface

| HSI ID | Interface | Description | Safety Relevance |
|--------|-----------|-------------|------------------|
| HSI-003 | TCP Socket | Network communication | Reliable delivery |
| HSI-004 | Unix Socket | Local IPC | Control interface |

**Requirements:**
- TSR-HSI-003: TCP sockets **shall** use SO_KEEPALIVE with 30s interval.
- TSR-HSI-004: Unix sockets **shall** use SOCK_STREAM for reliable delivery.

### 12.3 Filesystem Interface

| HSI ID | Interface | Description | Safety Relevance |
|--------|-----------|-------------|------------------|
| HSI-005 | Config Files | Configuration storage | Startup configuration |
| HSI-006 | FIFO Pipes | Named pipes for control interface | Workload communication |
| HSI-007 | Log Files | Audit and debug logs | Traceability |

**Requirements:**
- TSR-HSI-005: Configuration files **shall** be read atomically at startup.
- TSR-HSI-006: FIFO pipes **shall** be created with mode 0600 (owner only).
- TSR-HSI-007: Log files **shall** be flushed after each safety-relevant event.

### 12.4 Process Interface

| HSI ID | Interface | Description | Safety Relevance |
|--------|-----------|-------------|------------------|
| HSI-008 | Signal Handling | Process signals (SIGTERM, SIGKILL) | Graceful shutdown |
| HSI-009 | Child Process | Container runtime CLI | Workload management |

**Requirements:**
- TSR-HSI-008: SIGTERM handler **shall** initiate graceful shutdown within 100ms.
- TSR-HSI-009: Child process commands **shall** have configurable timeout (default 30s).

---

## 13. Traceability

### 13.1 FSR to TSR Traceability Matrix

| FSR ID | TSR IDs | Verification Status |
|--------|---------|---------------------|
| FSR-001 | TSR-001, TSR-002 | Pending |
| FSR-002 | TSR-003, TSR-004 | Pending |
| FSR-003 | TSR-005 | Pending |
| FSR-004 | TSR-006 | Pending |
| FSR-005 | TSR-007 | Pending |
| FSR-006 | TSR-008, TSR-009 | Pending |
| FSR-007 | TSR-010 | Pending |
| FSR-008 | TSR-011, TSR-012 | Pending |
| FSR-009 | TSR-013 | Pending |
| FSR-010 | TSR-014, TSR-015 | Pending |
| FSR-011 | TSR-016 | Pending |
| FSR-012 | TSR-017 | Pending |
| FSR-013 | TSR-018 | Pending |
| FSR-014 | TSR-019 | Pending |
| FSR-015 | TSR-020, TSR-021 | Pending |
| FSR-016 | TSR-022 | Pending |
| FSR-017 | TSR-023 | Pending |
| FSR-018 | TSR-024 | Pending |
| FSR-019 | TSR-025 | Pending |
| FSR-020 | TSR-026 | Pending |
| FSR-021 | TSR-027 | Pending |
| FSR-022 | TSR-028 | Pending |
| FSR-023 | TSR-029 | Pending |
| FSR-024 | TSR-030 | Pending |
| FSR-025 | TSR-031 | Pending |
| FSR-026 | TSR-032 | Pending |
| FSR-027 | TSR-033 | Pending |
| FSR-028 | TSR-034 | Pending |
| FSR-029 | TSR-035 | Pending |
| FSR-030 | TSR-036 | Pending |
| FSR-031 | TSR-037 | Pending |
| FSR-032 | TSR-038 | Pending |
| FSR-033 | TSR-039, TSR-040 | Pending |
| FSR-034 | TSR-041 | Pending |
| FSR-035 | TSR-042 | Pending |
| FSR-036 | TSR-043 | Pending |
| FSR-037 | TSR-044 | Pending |
| FSR-038 | TSR-045 | Pending |
| FSR-039 | TSR-046 | Pending |
| FSR-040 | TSR-047 | Pending |
| FSR-041 | TSR-048 | Pending |
| FSR-042 | TSR-049 | Pending |

### 13.2 TSR to Component Allocation

| Component | TSR IDs |
|-----------|---------|
| Server State Manager (SM) | TSR-014, TSR-015, TSR-018, TSR-019, TSR-029 |
| Server gRPC Server (GS) | TSR-028, TSR-035, TSR-038, TSR-040 |
| Server Cycle Check (CC) | TSR-031 |
| Agent gRPC Client (GC) | TSR-002, TSR-008, TSR-035, TSR-038, TSR-039, TSR-042 |
| Agent Workload Controller (WC) | TSR-003, TSR-006, TSR-017, TSR-023, TSR-048 |
| Agent State Checker (SC) | TSR-001, TSR-013, TSR-020, TSR-021, TSR-047 |
| Agent Workload Scheduler (WS) | TSR-022, TSR-030, TSR-032, TSR-033, TSR-034 |
| Agent Authorizer (AUTH) | TSR-009, TSR-036, TSR-037 |
| Agent Resource Monitor (RES) | TSR-044, TSR-045 |
| Agent Retry Manager | TSR-004, TSR-024 |
| Runtime Connectors | TSR-011, TSR-012, TSR-026, TSR-027, TSR-046 |

---

## 14. References

### 14.1 Input Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-FSR-001 | Functional Safety Requirements |
| ANKAIOS-HARA-001 | Hazard Analysis and Risk Assessment |
| ISO 26262-4:2018 | Product development: system level |

### 14.2 Output Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-SWA-001 | Software Architecture Specification |
| ANKAIOS-SWD-001 | Software Detailed Design |

---

## Appendix A: Timing Budget Summary

| Operation | Budget | Source TSR |
|-----------|--------|------------|
| Start failure detection | 100ms | TSR-001 |
| Start failure notification | 50ms | TSR-002 |
| Failure detection (running) | 500ms | TSR-020, TSR-021 |
| Recovery initiation | 200ms | TSR-022 |
| Heartbeat interval | 100ms | TSR-039 |
| Heartbeat timeout | 300ms | TSR-040 |
| State polling | 200ms | TSR-020 |
| Resource monitoring | 2000ms | TSR-044 |

---

## Appendix B: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Safety Team | Initial release |

---

*Document approved for ISO 26262 compliance activities.*
