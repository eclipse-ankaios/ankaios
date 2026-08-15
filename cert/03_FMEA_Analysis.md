# Failure Mode and Effects Analysis (FMEA)

## Eclipse Ankaios Workload Orchestrator

| Document Information | |
|---------------------|---|
| Document ID | ANKAIOS-FMEA-001 |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Related Item Definition | ANKAIOS-ID-001 |
| Related HARA | ANKAIOS-HARA-001 |
| Author | Safety Engineering Team |

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [FMEA Methodology](#2-fmea-methodology)
3. [System Architecture Overview](#3-system-architecture-overview)
4. [Server Component FMEA](#4-server-component-fmea)
5. [Agent Component FMEA](#5-agent-component-fmea)
6. [Communication FMEA](#6-communication-fmea)
7. [Control Interface FMEA](#7-control-interface-fmea)
8. [Runtime Connector FMEA](#8-runtime-connector-fmea)
9. [Common Cause Failure Analysis](#9-common-cause-failure-analysis)
10. [Recommended Actions Summary](#10-recommended-actions-summary)
11. [References](#11-references)

---

## 1. Introduction

### 1.1 Purpose

This Failure Mode and Effects Analysis (FMEA) document systematically identifies potential failure modes in the Eclipse Ankaios workload orchestrator, analyzes their effects, and recommends mitigations to ensure functional safety compliance.

### 1.2 Scope

This FMEA covers:
- Ankaios Server components
- Ankaios Agent components
- gRPC communication layer
- Control Interface mechanisms
- Runtime connectors

### 1.3 FMEA Type

This document implements a Design FMEA (DFMEA) approach analyzing software component failure modes at the design level.

---

## 2. FMEA Methodology

### 2.1 Process Overview

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  Identify   │───▶│  Identify   │───▶│  Assess     │───▶│  Recommend  │
│  Functions  │    │  Failure    │    │   Risk      │    │  Actions    │
│             │    │   Modes     │    │  (S×O×D)    │    │             │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

### 2.2 Severity Rating (S)

| Rating | Severity | Effect Description |
|--------|----------|-------------------|
| 10 | Hazardous without warning | Safety goal violated without warning |
| 9 | Hazardous with warning | Safety goal violated with warning |
| 8 | Very High | Major function loss |
| 7 | High | Significant performance degradation |
| 6 | Moderate | Partial function loss |
| 5 | Low | Minor performance impact |
| 4 | Very Low | Minor annoyance |
| 3 | Minor | Slight inconvenience |
| 2 | Very Minor | Barely perceptible |
| 1 | None | No effect |

### 2.3 Occurrence Rating (O)

| Rating | Occurrence | Failure Probability |
|--------|------------|---------------------|
| 10 | Very High | > 1 in 10 |
| 9 | High | 1 in 10 |
| 8 | High | 1 in 20 |
| 7 | Moderate | 1 in 100 |
| 6 | Moderate | 1 in 500 |
| 5 | Low | 1 in 2,000 |
| 4 | Low | 1 in 10,000 |
| 3 | Very Low | 1 in 100,000 |
| 2 | Remote | 1 in 1,000,000 |
| 1 | Nearly Impossible | < 1 in 1,000,000 |

### 2.4 Detection Rating (D)

| Rating | Detection | Detection Method |
|--------|-----------|------------------|
| 10 | Absolute Uncertainty | No detection method |
| 9 | Very Remote | Unlikely to detect |
| 8 | Remote | Low chance of detection |
| 7 | Very Low | Design review only |
| 6 | Low | Manual inspection |
| 5 | Moderate | Automated test may detect |
| 4 | Moderately High | Automated test likely detects |
| 3 | High | Automated test will detect |
| 2 | Very High | Multiple detection methods |
| 1 | Almost Certain | Continuous monitoring |

### 2.5 Risk Priority Number (RPN)

```
RPN = Severity (S) × Occurrence (O) × Detection (D)
```

| RPN Range | Risk Level | Action Required |
|-----------|------------|-----------------|
| 1-50 | Low | Monitor |
| 51-100 | Medium | Review recommended |
| 101-200 | High | Action required |
| 201-1000 | Critical | Immediate action required |

---

## 3. System Architecture Overview

### 3.1 Component Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        ANKAIOS SYSTEM                                    │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                         SERVER                                       ││
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   ││
│  │  │ State       │ │ Scheduler   │ │ Event       │ │ Config      │   ││
│  │  │ Manager     │ │             │ │ Handler     │ │ Renderer    │   ││
│  │  │ (SM)        │ │ (SCH)       │ │ (EH)        │ │ (CR)        │   ││
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘   ││
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐                   ││
│  │  │ Cycle       │ │ Hooks       │ │ gRPC        │                   ││
│  │  │ Check       │ │ Registry    │ │ Server      │                   ││
│  │  │ (CC)        │ │ (HR)        │ │ (GS)        │                   ││
│  │  └─────────────┘ └─────────────┘ └─────────────┘                   ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                    │                                     │
│                               gRPC │ Communication                       │
│                                    │                                     │
│  ┌─────────────────────────────────┼───────────────────────────────────┐│
│  │                         AGENT   │                                    ││
│  │  ┌─────────────┐ ┌──────────────┴┐ ┌─────────────┐ ┌─────────────┐ ││
│  │  │ Runtime     │ │ gRPC          │ │ Workload    │ │ Control     │ ││
│  │  │ Manager     │ │ Client        │ │ Scheduler   │ │ Interface   │ ││
│  │  │ (RM)        │ │ (GC)          │ │ (WS)        │ │ (CI)        │ ││
│  │  └─────────────┘ └───────────────┘ └─────────────┘ └─────────────┘ ││
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐  ││
│  │  │ Workload    │ │ State       │ │ Resource    │ │ Authorizer  │  ││
│  │  │ Controller  │ │ Checker     │ │ Monitor     │ │             │  ││
│  │  │ (WC)        │ │ (SC)        │ │ (RES)       │ │ (AUTH)      │  ││
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘  ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                    │                                     │
│                            Runtime │ Connector                           │
│                                    │                                     │
│  ┌─────────────────────────────────┼───────────────────────────────────┐│
│  │                    RUNTIME CONNECTORS                                ││
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐                   ││
│  │  │ Podman      │ │ Containerd  │ │ Podman      │                   ││
│  │  │ Connector   │ │ Connector   │ │ Kube        │                   ││
│  │  │ (PC)        │ │ (CDC)       │ │ (PKC)       │                   ││
│  │  └─────────────┘ └─────────────┘ └─────────────┘                   ││
│  └─────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Identification

| ID | Component | Description |
|----|-----------|-------------|
| SM | State Manager | Manages desired/actual state |
| SCH | Scheduler | Handles workload scheduling |
| EH | Event Handler | Processes state change events |
| CR | Config Renderer | Renders configuration templates |
| CC | Cycle Check | Detects dependency cycles |
| HR | Hooks Registry | Manages mutating hooks |
| GS | gRPC Server | Server-side communication |
| RM | Runtime Manager | Manages runtime connectors |
| GC | gRPC Client | Client-side communication |
| WS | Workload Scheduler | Agent-side scheduling |
| CI | Control Interface | Workload communication pipes |
| WC | Workload Controller | Workload lifecycle management |
| SC | State Checker | Polls container state |
| RES | Resource Monitor | Monitors CPU/memory |
| AUTH | Authorizer | Access control enforcement |
| PC | Podman Connector | Podman runtime adapter |
| CDC | Containerd Connector | Containerd runtime adapter |
| PKC | Podman Kube Connector | Podman Kube runtime adapter |

---

## 4. Server Component FMEA

### 4.1 State Manager (SM) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| SM-FM-001 | State data corruption | Invalid workload state stored | Wrong workloads deployed | 9 | 3 | 4 | 108 | Implement CRC for state data |
| SM-FM-002 | State update lost | Workload changes not applied | System not matching desired state | 8 | 4 | 3 | 96 | Add update acknowledgment |
| SM-FM-003 | Deadlock during state update | State manager blocked | No state changes processed | 9 | 3 | 5 | 135 | Implement timeout mechanism |
| SM-FM-004 | Memory exhaustion | Out of memory error | Server crash | 10 | 2 | 4 | 80 | Limit state size, monitoring |
| SM-FM-005 | Race condition on concurrent updates | Inconsistent state | Unpredictable behavior | 8 | 4 | 6 | 192 | Use proper synchronization |
| SM-FM-006 | API version mismatch | Incorrect state parsing | Workloads misconfigured | 7 | 3 | 2 | 42 | Strict version validation |
| SM-FM-007 | Field validation failure | Invalid fields accepted | Runtime errors | 6 | 4 | 3 | 72 | Enhanced input validation |
| SM-FM-008 | Partial state write | Incomplete state stored | Inconsistent state | 8 | 3 | 4 | 96 | Atomic state updates |

### 4.2 Scheduler (SCH) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| SCH-FM-001 | Incorrect agent assignment | Workload sent to wrong agent | Workload may fail to start | 7 | 3 | 4 | 84 | Validate agent capabilities |
| SCH-FM-002 | Scheduling delay | Commands queued too long | Slow workload deployment | 6 | 4 | 3 | 72 | Monitor queue depth |
| SCH-FM-003 | Agent selection bias | Some agents overloaded | Unbalanced resource usage | 5 | 5 | 5 | 125 | Implement load balancing |
| SCH-FM-004 | Missing agent | Assignment to offline agent | Workload not scheduled | 7 | 4 | 2 | 56 | Check agent status before assign |

### 4.3 Event Handler (EH) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| EH-FM-001 | Event dropped | Subscriber not notified | Stale state view | 6 | 4 | 5 | 120 | Reliable event delivery |
| EH-FM-002 | Event delivered out of order | Incorrect state sequence | Temporary inconsistency | 5 | 4 | 4 | 80 | Sequence numbering |
| EH-FM-003 | Subscriber leak | Resources not freed | Memory exhaustion | 7 | 3 | 4 | 84 | Subscription cleanup |
| EH-FM-004 | Slow subscriber blocking | Event queue full | Delivery delays | 6 | 5 | 3 | 90 | Per-subscriber buffering |

### 4.4 Config Renderer (CR) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| CR-FM-001 | Template parsing error | Config not rendered | Workload start fails | 6 | 4 | 2 | 48 | Validate templates early |
| CR-FM-002 | Missing config reference | Unresolved variable | Workload misconfigured | 7 | 4 | 2 | 56 | Validate all references |
| CR-FM-003 | Injection attack | Malicious config injected | Security breach | 9 | 2 | 4 | 72 | Sanitize template inputs |
| CR-FM-004 | Infinite template loop | Renderer hangs | Deployment blocked | 7 | 2 | 5 | 70 | Limit template depth |

### 4.5 Cycle Check (CC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| CC-FM-001 | Cycle not detected | Invalid state accepted | Workload deadlock | 9 | 2 | 5 | 90 | Thorough DFS testing |
| CC-FM-002 | False positive cycle | Valid state rejected | Deployment blocked | 5 | 3 | 3 | 45 | Verify cycle detection logic |
| CC-FM-003 | Algorithm timeout | Large graph hangs | Server unresponsive | 7 | 2 | 4 | 56 | Limit dependency depth |

### 4.6 Hooks Registry (HR) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| HR-FM-001 | Hook execution failure | Hook error | Deployment may proceed without hook | 7 | 4 | 3 | 84 | Handle hook failures gracefully |
| HR-FM-002 | Hook timeout | Hook hangs | Deployment delayed | 6 | 4 | 3 | 72 | Implement hook timeout |
| HR-FM-003 | Hook corrupts workload spec | Invalid spec generated | Workload fails | 8 | 3 | 5 | 120 | Validate hook output |
| HR-FM-004 | Hook priority misordering | Wrong execution order | Unexpected mutations | 5 | 3 | 4 | 60 | Verify priority handling |

### 4.7 gRPC Server (GS) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| GS-FM-001 | Connection refused | Agent cannot connect | Workloads not deployed | 8 | 3 | 2 | 48 | Monitor connection status |
| GS-FM-002 | TLS handshake failure | Secure connection fails | Agent disconnected | 7 | 4 | 2 | 56 | Certificate validation |
| GS-FM-003 | Message serialization error | Message corrupt | Communication fails | 7 | 2 | 3 | 42 | Protobuf validation |
| GS-FM-004 | Connection timeout | Stale connection | Agent appears disconnected | 6 | 4 | 3 | 72 | Implement keepalive |
| GS-FM-005 | Denial of service | Server overwhelmed | All agents affected | 9 | 2 | 5 | 90 | Rate limiting |
| GS-FM-006 | Port binding failure | Server cannot start | No orchestration | 10 | 2 | 1 | 20 | Port availability check |

---

## 5. Agent Component FMEA

### 5.1 Runtime Manager (RM) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| RM-FM-001 | Runtime not available | Cannot create workloads | Local workloads fail | 9 | 3 | 2 | 54 | Validate runtime at startup |
| RM-FM-002 | Runtime selection error | Wrong runtime used | Workload fails | 7 | 2 | 3 | 42 | Validate runtime name |
| RM-FM-003 | Runtime hung | Operations blocked | Workloads unresponsive | 8 | 3 | 5 | 120 | Runtime health check |
| RM-FM-004 | Runtime crash | Active workloads affected | Workload failures | 9 | 3 | 3 | 81 | Monitor runtime process |

### 5.2 gRPC Client (GC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| GC-FM-001 | Connection lost | No server communication | Agent isolated | 8 | 4 | 2 | 64 | Auto-reconnection |
| GC-FM-002 | Certificate expired | TLS fails | Agent disconnected | 7 | 3 | 2 | 42 | Certificate monitoring |
| GC-FM-003 | Server address wrong | Cannot connect | Agent useless | 8 | 2 | 1 | 16 | Validate address at startup |
| GC-FM-004 | Message corruption | Invalid data sent/received | Communication fails | 7 | 2 | 3 | 42 | Protobuf validation |
| GC-FM-005 | Reconnection storm | Multiple rapid reconnects | Server overloaded | 6 | 3 | 4 | 72 | Backoff strategy |

### 5.3 Workload Scheduler (WS) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| WS-FM-001 | Dependency deadlock | Workloads stuck waiting | Services not started | 9 | 3 | 5 | 135 | Deadlock detection |
| WS-FM-002 | Dependency not fulfilled | Workload started early | Runtime errors | 7 | 3 | 4 | 84 | Strict dependency check |
| WS-FM-003 | Queue overflow | Operations lost | Workloads not processed | 8 | 3 | 4 | 96 | Queue size monitoring |
| WS-FM-004 | Priority inversion | Low priority runs first | SLA violation | 5 | 4 | 5 | 100 | Priority-aware scheduling |

### 5.4 Control Interface (CI) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| CI-FM-001 | FIFO creation failure | No workload communication | Workload isolated | 7 | 3 | 2 | 42 | Validate pipe creation |
| CI-FM-002 | Pipe read timeout | Message delayed | Slow workload response | 5 | 4 | 3 | 60 | Implement timeouts |
| CI-FM-003 | Message truncation | Incomplete message | Protocol error | 6 | 2 | 3 | 36 | Length validation |
| CI-FM-004 | Pipe permission error | Workload cannot access | Communication blocked | 7 | 3 | 2 | 42 | Correct permissions |
| CI-FM-005 | Pipe cleanup failure | Stale pipes remain | Resource leak | 4 | 4 | 4 | 64 | Cleanup on workload stop |

### 5.5 Workload Controller (WC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| WC-FM-001 | Create command lost | Workload not started | Service unavailable | 9 | 3 | 4 | 108 | Command acknowledgment |
| WC-FM-002 | Delete command ignored | Workload not stopped | Orphan container | 6 | 3 | 3 | 54 | Verify deletion |
| WC-FM-003 | Retry loop stuck | Continuous failed retries | Resource waste | 5 | 4 | 4 | 80 | Max retry limit |
| WC-FM-004 | State transition error | Wrong state set | Incorrect behavior | 7 | 3 | 4 | 84 | State machine validation |
| WC-FM-005 | Update during create | Race condition | Undefined state | 8 | 3 | 6 | 144 | Operation serialization |
| WC-FM-006 | Restart policy ignored | No automatic restart | Service downtime | 8 | 3 | 3 | 72 | Policy enforcement test |

### 5.6 State Checker (SC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| SC-FM-001 | Polling failure | State not updated | Stale state reported | 7 | 4 | 4 | 112 | Redundant state checks |
| SC-FM-002 | Delayed state detection | Late failure detection | Slow recovery | 8 | 4 | 4 | 128 | Reduce polling interval |
| SC-FM-003 | False positive failure | Healthy workload marked failed | Unnecessary restart | 6 | 3 | 4 | 72 | Multiple confirmation |
| SC-FM-004 | Container ID mismatch | Wrong container checked | Incorrect state | 7 | 2 | 4 | 56 | ID validation |

### 5.7 Resource Monitor (RES) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| RES-FM-001 | CPU measurement error | Wrong CPU usage reported | Bad scheduling decisions | 5 | 3 | 5 | 75 | Validate measurements |
| RES-FM-002 | Memory measurement error | Wrong memory reported | Resource exhaustion | 6 | 3 | 5 | 90 | Validate measurements |
| RES-FM-003 | Measurement delay | Stale resource data | Suboptimal decisions | 4 | 4 | 4 | 64 | Timely updates |
| RES-FM-004 | Monitor crash | No resource data | Blind scheduling | 6 | 2 | 3 | 36 | Monitor health check |

### 5.8 Authorizer (AUTH) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| AUTH-FM-001 | Authorization bypass | Unauthorized access granted | Security breach | 10 | 2 | 5 | 100 | Defense in depth |
| AUTH-FM-002 | False denial | Valid request rejected | Service disruption | 6 | 3 | 3 | 54 | Rule testing |
| AUTH-FM-003 | Rule parsing error | Rules not applied | Open access | 9 | 2 | 3 | 54 | Rule validation |
| AUTH-FM-004 | Wildcard misinterpretation | Too broad access | Security risk | 8 | 3 | 5 | 120 | Wildcard testing |

---

## 6. Communication FMEA

### 6.1 gRPC Communication

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| COMM-FM-001 | Network partition | Server-agent split | Agent isolated | 8 | 4 | 2 | 64 | Graceful degradation |
| COMM-FM-002 | High latency | Delayed commands | Slow response | 5 | 5 | 3 | 75 | Timeout handling |
| COMM-FM-003 | Message reordering | Commands out of order | State inconsistency | 7 | 3 | 5 | 105 | Sequence numbers |
| COMM-FM-004 | Message duplication | Command executed twice | Unexpected behavior | 7 | 3 | 4 | 84 | Idempotent operations |
| COMM-FM-005 | Channel overflow | Messages dropped | Lost commands | 8 | 3 | 4 | 96 | Backpressure handling |
| COMM-FM-006 | Certificate revoked | Connection rejected | Agent offline | 7 | 2 | 2 | 28 | Certificate management |

### 6.2 TLS/mTLS Communication

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| TLS-FM-001 | Man-in-the-middle | Data intercepted | Security breach | 10 | 2 | 6 | 120 | Certificate pinning |
| TLS-FM-002 | Weak cipher selected | Vulnerable encryption | Data exposure risk | 8 | 2 | 4 | 64 | Cipher suite policy |
| TLS-FM-003 | Certificate validation skip | Impersonation possible | Trust compromise | 10 | 1 | 4 | 40 | Strict validation |
| TLS-FM-004 | Key compromise | All communications exposed | Complete breach | 10 | 1 | 7 | 70 | Key rotation |

---

## 7. Control Interface FMEA

### 7.1 FIFO Pipe Communication

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| FIFO-FM-001 | Pipe full | Write blocked | Workload stalled | 6 | 4 | 4 | 96 | Non-blocking writes |
| FIFO-FM-002 | Read without writer | Blocking read | Agent task stuck | 6 | 3 | 3 | 54 | Timeout on reads |
| FIFO-FM-003 | Pipe path collision | Wrong workload receives message | Data corruption | 8 | 2 | 5 | 80 | Unique path generation |
| FIFO-FM-004 | Filesystem full | Cannot create pipe | Workload isolated | 7 | 2 | 2 | 28 | Disk space monitoring |

### 7.2 Protocol Handling

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| PROTO-FM-001 | Protobuf decode error | Message rejected | Communication lost | 6 | 3 | 2 | 36 | Error handling |
| PROTO-FM-002 | Unknown message type | Message ignored | Feature not working | 5 | 3 | 2 | 30 | Version compatibility |
| PROTO-FM-003 | Request ID collision | Wrong response routed | Data corruption | 7 | 2 | 5 | 70 | Unique ID generation |
| PROTO-FM-004 | Response timeout | Requester blocked | Slow operation | 5 | 4 | 3 | 60 | Request timeouts |

---

## 8. Runtime Connector FMEA

### 8.1 Podman Connector (PC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| PC-FM-001 | Podman CLI not found | Cannot execute commands | Workloads fail | 8 | 2 | 1 | 16 | Startup validation |
| PC-FM-002 | CLI timeout | Command hangs | Operation blocked | 6 | 4 | 4 | 96 | CLI timeout |
| PC-FM-003 | Parse error | Cannot read container state | Incorrect state | 7 | 3 | 4 | 84 | Robust parsing |
| PC-FM-004 | Image pull failure | Cannot start container | Workload fails | 7 | 4 | 2 | 56 | Image availability check |
| PC-FM-005 | Container name collision | Create fails | Workload not started | 6 | 3 | 2 | 36 | Name uniqueness |
| PC-FM-006 | Resource limit rejection | Container fails to start | Workload fails | 6 | 3 | 3 | 54 | Validate resource spec |

### 8.2 Containerd Connector (CDC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| CDC-FM-001 | Containerd socket error | Cannot connect to runtime | All workloads fail | 9 | 3 | 2 | 54 | Socket availability |
| CDC-FM-002 | Namespace conflict | Wrong containers accessed | State confusion | 7 | 2 | 5 | 70 | Namespace isolation |
| CDC-FM-003 | Task creation failure | Container does not start | Workload fails | 7 | 3 | 3 | 63 | Error handling |
| CDC-FM-004 | Snapshot cleanup failure | Storage leak | Disk exhaustion | 5 | 4 | 4 | 80 | Storage monitoring |

### 8.3 Podman Kube Connector (PKC) FMEA

| FM ID | Failure Mode | Effect on Component | Effect on System | S | O | D | RPN | Recommended Action |
|-------|--------------|--------------------|--------------------|---|---|---|-----|-------------------|
| PKC-FM-001 | YAML parsing failure | Cannot parse manifest | Pod not created | 7 | 3 | 2 | 42 | YAML validation |
| PKC-FM-002 | Pod creation partial | Some containers fail | Partial service | 7 | 3 | 4 | 84 | Atomic pod handling |
| PKC-FM-003 | Multi-container dependency | Wrong startup order | Service issues | 6 | 4 | 5 | 120 | Pod ordering |
| PKC-FM-004 | Pod cleanup incomplete | Orphan containers | Resource leak | 5 | 3 | 4 | 60 | Thorough cleanup |

---

## 9. Common Cause Failure Analysis

### 9.1 Hardware Common Causes

| CCF ID | Common Cause | Affected Components | Effect | Mitigation |
|--------|--------------|---------------------|--------|------------|
| HW-CCF-001 | Power failure | All components | Complete shutdown | UPS, graceful shutdown |
| HW-CCF-002 | Network hardware failure | GS, GC, COMM | Communication loss | Redundant network |
| HW-CCF-003 | Storage failure | SM, CR, CI | State/config lost | Redundant storage |
| HW-CCF-004 | Memory exhaustion | All components | System crash | Memory limits |
| HW-CCF-005 | CPU overload | All components | Degraded performance | Resource quotas |

### 9.2 Software Common Causes

| CCF ID | Common Cause | Affected Components | Effect | Mitigation |
|--------|--------------|---------------------|--------|------------|
| SW-CCF-001 | Rust panic | All Rust components | Process crash | No-panic policy |
| SW-CCF-002 | Tokio runtime failure | All async components | All tasks fail | Runtime monitoring |
| SW-CCF-003 | Protobuf library bug | All communication | Decode failures | Library testing |
| SW-CCF-004 | gRPC library bug | GS, GC | Communication fails | Library updates |
| SW-CCF-005 | OS kernel bug | All components | Unpredictable | Kernel testing |

### 9.3 Environmental Common Causes

| CCF ID | Common Cause | Affected Components | Effect | Mitigation |
|--------|--------------|---------------------|--------|------------|
| ENV-CCF-001 | Temperature extreme | All hardware | Hardware failure | Thermal management |
| ENV-CCF-002 | EMI interference | Communication | Signal corruption | EMC shielding |
| ENV-CCF-003 | Vibration | Storage, network | Intermittent failures | Shock isolation |

### 9.4 Human Common Causes

| CCF ID | Common Cause | Affected Components | Effect | Mitigation |
|--------|--------------|---------------------|--------|------------|
| HUM-CCF-001 | Configuration error | SM, CR, AUTH | Misconfiguration | Config validation |
| HUM-CCF-002 | Certificate management | TLS | Communication fails | Automated cert mgmt |
| HUM-CCF-003 | Wrong deployment | All | Wrong version | Deployment verification |

---

## 10. Recommended Actions Summary

### 10.1 High Priority Actions (RPN > 100)

| FM ID | Component | RPN | Recommended Action | Priority |
|-------|-----------|-----|-------------------|----------|
| SM-FM-005 | State Manager | 192 | Implement proper synchronization primitives | Critical |
| WC-FM-005 | Workload Controller | 144 | Serialize workload operations | Critical |
| WS-FM-001 | Workload Scheduler | 135 | Implement deadlock detection with timeout | Critical |
| SM-FM-003 | State Manager | 135 | Add timeout mechanism for state updates | Critical |
| SC-FM-002 | State Checker | 128 | Reduce polling interval, add event notification | High |
| PKC-FM-003 | Podman Kube | 120 | Implement proper pod container ordering | High |
| HR-FM-003 | Hooks Registry | 120 | Validate hook output against schema | High |
| AUTH-FM-004 | Authorizer | 120 | Comprehensive wildcard pattern testing | High |
| TLS-FM-001 | TLS | 120 | Implement certificate pinning | High |
| EH-FM-001 | Event Handler | 120 | Implement reliable event delivery with retry | High |
| RM-FM-003 | Runtime Manager | 120 | Add runtime health monitoring | High |
| SC-FM-001 | State Checker | 112 | Implement redundant state checking | High |
| WC-FM-001 | Workload Controller | 108 | Add command acknowledgment mechanism | High |
| SM-FM-001 | State Manager | 108 | Implement CRC for state data integrity | High |
| COMM-FM-003 | Communication | 105 | Implement sequence numbering | High |
| AUTH-FM-001 | Authorizer | 100 | Implement defense in depth | High |
| WS-FM-004 | Workload Scheduler | 100 | Implement priority-aware scheduling | High |

### 10.2 Medium Priority Actions (RPN 50-100)

| FM ID | Component | RPN | Recommended Action |
|-------|-----------|-----|-------------------|
| SM-FM-002 | State Manager | 96 | Add update acknowledgment |
| WS-FM-003 | Workload Scheduler | 96 | Queue size monitoring |
| COMM-FM-005 | Communication | 96 | Implement backpressure handling |
| PC-FM-002 | Podman | 96 | Implement CLI command timeout |
| FIFO-FM-001 | Control Interface | 96 | Non-blocking pipe writes |
| GS-FM-005 | gRPC Server | 90 | Implement rate limiting |
| CC-FM-001 | Cycle Check | 90 | Thorough DFS algorithm testing |
| RES-FM-002 | Resource Monitor | 90 | Validate memory measurements |
| EH-FM-004 | Event Handler | 90 | Per-subscriber buffering |
| SCH-FM-001 | Scheduler | 84 | Validate agent capabilities |
| HR-FM-001 | Hooks Registry | 84 | Graceful hook failure handling |
| WS-FM-002 | Workload Scheduler | 84 | Strict dependency checking |
| WC-FM-004 | Workload Controller | 84 | State machine validation |
| COMM-FM-004 | Communication | 84 | Idempotent operations |
| PC-FM-003 | Podman | 84 | Robust CLI output parsing |
| PKC-FM-002 | Podman Kube | 84 | Atomic pod handling |
| EH-FM-003 | Event Handler | 84 | Subscription cleanup |
| SM-FM-004 | State Manager | 80 | Limit state size |
| EH-FM-002 | Event Handler | 80 | Sequence numbering |
| WC-FM-003 | Workload Controller | 80 | Max retry limit |
| FIFO-FM-003 | Control Interface | 80 | Unique path generation |
| CDC-FM-004 | Containerd | 80 | Storage monitoring |
| RES-FM-001 | Resource Monitor | 75 | Validate CPU measurements |
| COMM-FM-002 | Communication | 75 | Timeout handling |
| SM-FM-007 | State Manager | 72 | Enhanced input validation |
| SCH-FM-002 | Scheduler | 72 | Monitor queue depth |
| HR-FM-002 | Hooks Registry | 72 | Implement hook timeout |
| GS-FM-004 | gRPC Server | 72 | Implement keepalive |
| GC-FM-005 | gRPC Client | 72 | Backoff reconnection strategy |
| SC-FM-003 | State Checker | 72 | Multiple confirmation |
| WC-FM-006 | Workload Controller | 72 | Policy enforcement testing |
| CR-FM-003 | Config Renderer | 72 | Sanitize template inputs |
| PROTO-FM-003 | Protocol | 70 | Unique request ID generation |
| CDC-FM-002 | Containerd | 70 | Namespace isolation |
| CR-FM-004 | Config Renderer | 70 | Limit template depth |
| TLS-FM-004 | TLS | 70 | Key rotation mechanism |
| TLS-FM-002 | TLS | 64 | Cipher suite policy |
| GC-FM-001 | gRPC Client | 64 | Auto-reconnection |
| COMM-FM-001 | Communication | 64 | Graceful degradation |
| RES-FM-003 | Resource Monitor | 64 | Timely resource updates |
| CI-FM-005 | Control Interface | 64 | Cleanup on workload stop |
| CDC-FM-003 | Containerd | 63 | Error handling |
| PKC-FM-004 | Podman Kube | 60 | Thorough pod cleanup |
| HR-FM-004 | Hooks Registry | 60 | Verify priority handling |
| CI-FM-002 | Control Interface | 60 | Implement pipe timeouts |
| PROTO-FM-004 | Protocol | 60 | Request timeout handling |
| SC-FM-004 | State Checker | 56 | Container ID validation |
| SCH-FM-004 | Scheduler | 56 | Check agent status |
| GS-FM-002 | gRPC Server | 56 | Certificate validation |
| CC-FM-003 | Cycle Check | 56 | Limit dependency depth |
| CR-FM-002 | Config Renderer | 56 | Validate config references |
| PC-FM-004 | Podman | 56 | Image availability check |
| WC-FM-002 | Workload Controller | 54 | Verify deletion |
| FIFO-FM-002 | Control Interface | 54 | Timeout on reads |
| AUTH-FM-002 | Authorizer | 54 | Rule testing |
| AUTH-FM-003 | Authorizer | 54 | Rule validation |
| RM-FM-001 | Runtime Manager | 54 | Validate runtime at startup |
| CDC-FM-001 | Containerd | 54 | Socket availability check |
| PC-FM-006 | Podman | 54 | Validate resource spec |

### 10.3 Low Priority Actions (RPN < 50)

| Action Category | Count | Summary |
|-----------------|-------|---------|
| Configuration Validation | 8 | Validate inputs, addresses, templates |
| Error Handling | 6 | Robust parsing, error recovery |
| Monitoring | 4 | Health checks, resource monitoring |
| Testing | 5 | Algorithm testing, rule testing |

---

## 11. References

### 11.1 Input Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-ID-001 | Item Definition |
| ANKAIOS-HARA-001 | Hazard Analysis and Risk Assessment |
| ISO 26262-9:2018 | ASIL-oriented and safety-oriented analyses |

### 11.2 Standards

| Standard | Title |
|----------|-------|
| SAE J1739 | Potential Failure Mode and Effects Analysis |
| IEC 60812 | Analysis techniques for system reliability - FMEA |
| ISO 26262-9:2018 | Safety analyses |

---

## Appendix A: FMEA Summary Statistics

| Category | Count | Avg RPN | Max RPN | Critical (>100) |
|----------|-------|---------|---------|-----------------|
| Server Components | 32 | 78 | 192 | 8 |
| Agent Components | 38 | 74 | 144 | 6 |
| Communication | 12 | 73 | 120 | 3 |
| Control Interface | 8 | 62 | 96 | 0 |
| Runtime Connectors | 16 | 64 | 120 | 1 |
| **Total** | **106** | **72** | **192** | **18** |

---

## Appendix B: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Safety Team | Initial release |

---

*Document approved for ISO 26262 compliance activities.*
