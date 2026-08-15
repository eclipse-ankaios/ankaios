# Functional Safety Requirements (FSR)

## Eclipse Ankaios Workload Orchestrator

| Document Information | |
|---------------------|---|
| Document ID | ANKAIOS-FSR-001 |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Related HARA | ANKAIOS-HARA-001 |
| Author | Safety Engineering Team |

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Requirements Overview](#2-requirements-overview)
3. [SG-001: Workload Availability Requirements](#3-sg-001-workload-availability-requirements)
4. [SG-002: Unintended Termination Prevention Requirements](#4-sg-002-unintended-termination-prevention-requirements)
5. [SG-003: State Integrity Requirements](#5-sg-003-state-integrity-requirements)
6. [SG-004: Failure Recovery Requirements](#6-sg-004-failure-recovery-requirements)
7. [SG-005: Cascade Failure Prevention Requirements](#7-sg-005-cascade-failure-prevention-requirements)
8. [SG-006: Dependency Ordering Requirements](#8-sg-006-dependency-ordering-requirements)
9. [SG-007: Authorization Requirements](#9-sg-007-authorization-requirements)
10. [SG-008: Communication Integrity Requirements](#10-sg-008-communication-integrity-requirements)
11. [SG-009: Resource Availability Requirements](#11-sg-009-resource-availability-requirements)
12. [SG-010: Spurious Restart Prevention Requirements](#12-sg-010-spurious-restart-prevention-requirements)
13. [Requirements Traceability](#13-requirements-traceability)
14. [References](#14-references)

---

## 1. Introduction

### 1.1 Purpose

This document specifies the Functional Safety Requirements (FSRs) for Eclipse Ankaios derived from the Safety Goals defined in the HARA. These requirements form the basis for the functional safety concept and drive the technical safety requirements.

### 1.2 Scope

These requirements apply to:
- Ankaios Server component
- Ankaios Agent components
- Communication interfaces
- Runtime integration

### 1.3 Requirements Notation

| Notation | Meaning |
|----------|---------|
| **shall** | Mandatory requirement |
| **should** | Recommended requirement |
| **may** | Optional requirement |
| [ASIL X] | ASIL level of requirement |
| {SG-XXX} | Source safety goal |

---

## 2. Requirements Overview

### 2.1 Requirements Summary

| Safety Goal | FSR Range | Count | ASIL |
|-------------|-----------|-------|------|
| SG-001: Workload Availability | FSR-001 to FSR-005 | 5 | D |
| SG-002: Termination Prevention | FSR-006 to FSR-009 | 4 | D |
| SG-003: State Integrity | FSR-010 to FSR-014 | 5 | D |
| SG-004: Failure Recovery | FSR-015 to FSR-019 | 5 | D |
| SG-005: Cascade Prevention | FSR-020 to FSR-024 | 5 | D |
| SG-006: Dependency Ordering | FSR-025 to FSR-028 | 4 | C |
| SG-007: Authorization | FSR-029 to FSR-032 | 4 | C |
| SG-008: Communication Integrity | FSR-033 to FSR-036 | 4 | B |
| SG-009: Resource Availability | FSR-037 to FSR-039 | 3 | A |
| SG-010: Spurious Restart Prevention | FSR-040 to FSR-042 | 3 | A |
| **Total** | | **42** | |

### 2.2 ASIL Distribution

| ASIL | Count | Percentage |
|------|-------|------------|
| D | 24 | 57% |
| C | 8 | 19% |
| B | 4 | 10% |
| A | 6 | 14% |

---

## 3. SG-001: Workload Availability Requirements

### FSR-001: Workload Start Detection

| Attribute | Value |
|-----------|-------|
| ID | FSR-001 |
| Title | Workload Start Detection |
| Description | The system **shall** detect workload start failures within 100ms of the expected start time. |
| ASIL | D |
| Source | {SG-001} |
| Rationale | Early detection enables timely recovery actions |
| Acceptance Criteria | Start failure detected and reported within 100ms |
| Verification Method | Integration Test |

### FSR-002: Workload Start Retry

| Attribute | Value |
|-----------|-------|
| ID | FSR-002 |
| Title | Workload Start Retry |
| Description | The system **shall** automatically retry failed workload starts up to 3 times with exponential backoff before declaring permanent failure. |
| ASIL | D |
| Source | {SG-001} |
| Rationale | Transient failures may resolve on retry |
| Acceptance Criteria | 3 retry attempts observed with increasing delays |
| Verification Method | Unit Test, Integration Test |

### FSR-003: Start Failure Notification

| Attribute | Value |
|-----------|-------|
| ID | FSR-003 |
| Title | Start Failure Notification |
| Description | The system **shall** notify the safety monitoring system of permanent workload start failures within 500ms of the final retry failure. |
| ASIL | D |
| Source | {SG-001} |
| Rationale | External systems need failure notification for graceful degradation |
| Acceptance Criteria | Notification sent within 500ms of final failure |
| Verification Method | Integration Test |

### FSR-004: Workload Startup Timeout

| Attribute | Value |
|-----------|-------|
| ID | FSR-004 |
| Title | Workload Startup Timeout |
| Description | The system **shall** enforce a configurable startup timeout for each workload, defaulting to 30 seconds, and treat timeout as start failure. |
| ASIL | D |
| Source | {SG-001} |
| Rationale | Prevents indefinite waiting for unresponsive workloads |
| Acceptance Criteria | Timeout triggers start failure after configured duration |
| Verification Method | Integration Test |

### FSR-005: Start State Reporting

| Attribute | Value |
|-----------|-------|
| ID | FSR-005 |
| Title | Start State Reporting |
| Description | The system **shall** report workload state transitions during startup (PENDING → STARTING → RUNNING) with timestamps accurate to 10ms. |
| ASIL | D |
| Source | {SG-001} |
| Rationale | Accurate state tracking enables monitoring and debugging |
| Acceptance Criteria | State transitions reported with 10ms timestamp accuracy |
| Verification Method | Unit Test |

---

## 4. SG-002: Unintended Termination Prevention Requirements

### FSR-006: Termination Authorization

| Attribute | Value |
|-----------|-------|
| ID | FSR-006 |
| Title | Termination Authorization |
| Description | The system **shall** require explicit authorization from the server or authorized control interface request before terminating any safety-critical workload. |
| ASIL | D |
| Source | {SG-002} |
| Rationale | Prevents unauthorized or accidental termination |
| Acceptance Criteria | Termination blocked without valid authorization |
| Verification Method | Unit Test, Security Test |

### FSR-007: Termination Logging

| Attribute | Value |
|-----------|-------|
| ID | FSR-007 |
| Title | Termination Logging |
| Description | The system **shall** log all workload termination requests with: source identity, timestamp, workload identifier, and authorization status. |
| ASIL | D |
| Source | {SG-002} |
| Rationale | Audit trail for security and debugging |
| Acceptance Criteria | All termination attempts logged with required fields |
| Verification Method | Unit Test, Review |

### FSR-008: Safe Termination Sequence

| Attribute | Value |
|-----------|-------|
| ID | FSR-008 |
| Title | Safe Termination Sequence |
| Description | The system **shall** execute a graceful shutdown sequence (SIGTERM, wait up to 10s, SIGKILL) when terminating workloads. |
| ASIL | D |
| Source | {SG-002} |
| Rationale | Allows workload cleanup before forced termination |
| Acceptance Criteria | Graceful shutdown observed with correct timing |
| Verification Method | Integration Test |

### FSR-009: Termination State Confirmation

| Attribute | Value |
|-----------|-------|
| ID | FSR-009 |
| Title | Termination State Confirmation |
| Description | The system **shall** confirm workload termination through state checking and **shall not** report STOPPED state until container is verified terminated. |
| ASIL | D |
| Source | {SG-002} |
| Rationale | Prevents reporting false termination |
| Acceptance Criteria | State only updated after verification |
| Verification Method | Unit Test |

---

## 5. SG-003: State Integrity Requirements

### FSR-010: State Data Validation

| Attribute | Value |
|-----------|-------|
| ID | FSR-010 |
| Title | State Data Validation |
| Description | The system **shall** validate all state data against defined schemas before storing or processing. |
| ASIL | D |
| Source | {SG-003} |
| Rationale | Prevents corruption from invalid data |
| Acceptance Criteria | Invalid state data rejected with error |
| Verification Method | Unit Test |

### FSR-011: State Checksum

| Attribute | Value |
|-----------|-------|
| ID | FSR-011 |
| Title | State Checksum |
| Description | The system **shall** include a CRC-32 checksum with all state data transmissions and storage. |
| ASIL | D |
| Source | {SG-003} |
| Rationale | Detects data corruption |
| Acceptance Criteria | Checksum present and validated on all state operations |
| Verification Method | Unit Test |

### FSR-012: State Transition Validation

| Attribute | Value |
|-----------|-------|
| ID | FSR-012 |
| Title | State Transition Validation |
| Description | The system **shall** validate all workload state transitions against a defined state machine and reject invalid transitions. |
| ASIL | D |
| Source | {SG-003} |
| Rationale | Prevents illegal state changes |
| Acceptance Criteria | Invalid transitions rejected and logged |
| Verification Method | Unit Test |

### FSR-013: Atomic State Updates

| Attribute | Value |
|-----------|-------|
| ID | FSR-013 |
| Title | Atomic State Updates |
| Description | The system **shall** perform state updates atomically such that partial updates are not visible to other components. |
| ASIL | D |
| Source | {SG-003} |
| Rationale | Prevents inconsistent state observation |
| Acceptance Criteria | No partial states observable |
| Verification Method | Concurrency Test |

### FSR-014: State Consistency Check

| Attribute | Value |
|-----------|-------|
| ID | FSR-014 |
| Title | State Consistency Check |
| Description | The system **shall** perform periodic consistency checks between server desired state and agent actual state every 5 seconds. |
| ASIL | D |
| Source | {SG-003} |
| Rationale | Detects state divergence |
| Acceptance Criteria | Consistency check executed at defined interval |
| Verification Method | Integration Test |

---

## 6. SG-004: Failure Recovery Requirements

### FSR-015: Failure Detection Time

| Attribute | Value |
|-----------|-------|
| ID | FSR-015 |
| Title | Failure Detection Time |
| Description | The system **shall** detect workload failures (crash, hang, exit) within 500ms of occurrence. |
| ASIL | D |
| Source | {SG-004} |
| Rationale | Enables timely recovery |
| Acceptance Criteria | Failure detected within 500ms |
| Verification Method | Integration Test |

### FSR-016: Recovery Initiation Time

| Attribute | Value |
|-----------|-------|
| ID | FSR-016 |
| Title | Recovery Initiation Time |
| Description | The system **shall** initiate workload recovery (restart or notification) within 200ms of failure detection. |
| ASIL | D |
| Source | {SG-004} |
| Rationale | Minimizes downtime |
| Acceptance Criteria | Recovery action started within 200ms |
| Verification Method | Integration Test |

### FSR-017: Restart Policy Enforcement

| Attribute | Value |
|-----------|-------|
| ID | FSR-017 |
| Title | Restart Policy Enforcement |
| Description | The system **shall** enforce the configured restart policy (NEVER, ON_FAILURE, ALWAYS) for each workload. |
| ASIL | D |
| Source | {SG-004} |
| Rationale | Correct recovery behavior per configuration |
| Acceptance Criteria | Restart behavior matches policy |
| Verification Method | Unit Test |

### FSR-018: Recovery Backoff

| Attribute | Value |
|-----------|-------|
| ID | FSR-018 |
| Title | Recovery Backoff |
| Description | The system **shall** implement exponential backoff (1s, 2s, 4s, ..., max 5min) for workload restart attempts to prevent recovery storms. |
| ASIL | D |
| Source | {SG-004} |
| Rationale | Prevents resource exhaustion from rapid restarts |
| Acceptance Criteria | Backoff timing matches specification |
| Verification Method | Unit Test |

### FSR-019: Recovery State Reporting

| Attribute | Value |
|-----------|-------|
| ID | FSR-019 |
| Title | Recovery State Reporting |
| Description | The system **shall** report recovery attempt count and status to the server within 100ms of each attempt. |
| ASIL | D |
| Source | {SG-004} |
| Rationale | Enables monitoring of recovery progress |
| Acceptance Criteria | Recovery status reported within 100ms |
| Verification Method | Integration Test |

---

## 7. SG-005: Cascade Failure Prevention Requirements

### FSR-020: Failure Isolation

| Attribute | Value |
|-----------|-------|
| ID | FSR-020 |
| Title | Failure Isolation |
| Description | The system **shall** isolate workload failures such that failure of one workload does not directly cause failure of another workload. |
| ASIL | D |
| Source | {SG-005} |
| Rationale | Prevents cascade failures |
| Acceptance Criteria | Single workload failure does not affect others |
| Verification Method | Fault Injection Test |

### FSR-021: Resource Isolation

| Attribute | Value |
|-----------|-------|
| ID | FSR-021 |
| Title | Resource Isolation |
| Description | The system **shall** enforce resource limits (CPU, memory) per workload to prevent resource exhaustion affecting other workloads. |
| ASIL | D |
| Source | {SG-005} |
| Rationale | Resource exhaustion is common cascade trigger |
| Acceptance Criteria | Resource limits enforced |
| Verification Method | Integration Test |

### FSR-022: Communication Failure Containment

| Attribute | Value |
|-----------|-------|
| ID | FSR-022 |
| Title | Communication Failure Containment |
| Description | The system **shall** contain communication failures to affected channels without impacting other agent communications. |
| ASIL | D |
| Source | {SG-005} |
| Rationale | Network issues should not cascade |
| Acceptance Criteria | Per-agent communication isolation |
| Verification Method | Fault Injection Test |

### FSR-023: State Error Containment

| Attribute | Value |
|-----------|-------|
| ID | FSR-023 |
| Title | State Error Containment |
| Description | The system **shall** reject state updates that would cause errors and **shall not** propagate invalid state to other components. |
| ASIL | D |
| Source | {SG-005} |
| Rationale | Prevents state corruption cascade |
| Acceptance Criteria | Invalid state isolated at entry point |
| Verification Method | Unit Test |

### FSR-024: Dependency Failure Handling

| Attribute | Value |
|-----------|-------|
| ID | FSR-024 |
| Title | Dependency Failure Handling |
| Description | The system **shall** handle dependency workload failures gracefully by maintaining dependent workloads in a defined waiting state rather than failing them. |
| ASIL | D |
| Source | {SG-005} |
| Rationale | Dependency failures should not cascade |
| Acceptance Criteria | Dependents wait rather than fail |
| Verification Method | Integration Test |

---

## 8. SG-006: Dependency Ordering Requirements

### FSR-025: Dependency Cycle Detection

| Attribute | Value |
|-----------|-------|
| ID | FSR-025 |
| Title | Dependency Cycle Detection |
| Description | The system **shall** detect and reject workload configurations containing circular dependencies. |
| ASIL | C |
| Source | {SG-006} |
| Rationale | Cycles cause deadlocks |
| Acceptance Criteria | Cyclic configurations rejected with error |
| Verification Method | Unit Test |

### FSR-026: Startup Order Enforcement

| Attribute | Value |
|-----------|-------|
| ID | FSR-026 |
| Title | Startup Order Enforcement |
| Description | The system **shall** start workloads only when all add-condition dependencies are satisfied. |
| ASIL | C |
| Source | {SG-006} |
| Rationale | Ensures correct startup sequence |
| Acceptance Criteria | Workload waits for dependencies |
| Verification Method | Integration Test |

### FSR-027: Shutdown Order Enforcement

| Attribute | Value |
|-----------|-------|
| ID | FSR-027 |
| Title | Shutdown Order Enforcement |
| Description | The system **shall** stop workloads only when all delete-condition dependencies are satisfied. |
| ASIL | C |
| Source | {SG-006} |
| Rationale | Ensures correct shutdown sequence |
| Acceptance Criteria | Workload waits for dependents |
| Verification Method | Integration Test |

### FSR-028: Dependency Deadlock Detection

| Attribute | Value |
|-----------|-------|
| ID | FSR-028 |
| Title | Dependency Deadlock Detection |
| Description | The system **shall** detect runtime dependency deadlocks within 1000ms and report them to the server. |
| ASIL | C |
| Source | {SG-006} |
| Rationale | Runtime deadlocks need detection |
| Acceptance Criteria | Deadlock detected and reported |
| Verification Method | Integration Test |

---

## 9. SG-007: Authorization Requirements

### FSR-029: Authentication Enforcement

| Attribute | Value |
|-----------|-------|
| ID | FSR-029 |
| Title | Authentication Enforcement |
| Description | The system **shall** authenticate all communication endpoints using mTLS with minimum 2048-bit RSA or 256-bit ECC keys. |
| ASIL | C |
| Source | {SG-007} |
| Rationale | Prevents unauthorized access |
| Acceptance Criteria | Unauthenticated connections rejected |
| Verification Method | Security Test |

### FSR-030: Authorization Rule Enforcement

| Attribute | Value |
|-----------|-------|
| ID | FSR-030 |
| Title | Authorization Rule Enforcement |
| Description | The system **shall** enforce per-workload authorization rules for control interface access with default-deny policy. |
| ASIL | C |
| Source | {SG-007} |
| Rationale | Limits workload capabilities |
| Acceptance Criteria | Unauthorized requests denied |
| Verification Method | Security Test |

### FSR-031: Authorization Decision Logging

| Attribute | Value |
|-----------|-------|
| ID | FSR-031 |
| Title | Authorization Decision Logging |
| Description | The system **shall** log all authorization decisions (allow/deny) with: requester identity, requested resource, and decision timestamp. |
| ASIL | C |
| Source | {SG-007} |
| Rationale | Security audit trail |
| Acceptance Criteria | All decisions logged |
| Verification Method | Review |

### FSR-032: Certificate Validation

| Attribute | Value |
|-----------|-------|
| ID | FSR-032 |
| Title | Certificate Validation |
| Description | The system **shall** validate certificate chain, expiration, and revocation status before accepting connections. |
| ASIL | C |
| Source | {SG-007} |
| Rationale | Ensures valid credentials |
| Acceptance Criteria | Invalid certificates rejected |
| Verification Method | Security Test |

---

## 10. SG-008: Communication Integrity Requirements

### FSR-033: Communication Loss Detection

| Attribute | Value |
|-----------|-------|
| ID | FSR-033 |
| Title | Communication Loss Detection |
| Description | The system **shall** detect server-agent communication loss within 300ms using heartbeat mechanism. |
| ASIL | B |
| Source | {SG-008} |
| Rationale | Timely detection of network issues |
| Acceptance Criteria | Loss detected within 300ms |
| Verification Method | Integration Test |

### FSR-034: Graceful Degradation Mode

| Attribute | Value |
|-----------|-------|
| ID | FSR-034 |
| Title | Graceful Degradation Mode |
| Description | The system **shall** maintain workload operation for at least 10 seconds after detecting communication loss before entering degraded mode. |
| ASIL | B |
| Source | {SG-008} |
| Rationale | Handles transient network issues |
| Acceptance Criteria | Workloads continue for 10s |
| Verification Method | Integration Test |

### FSR-035: Connection Recovery

| Attribute | Value |
|-----------|-------|
| ID | FSR-035 |
| Title | Connection Recovery |
| Description | The system **shall** attempt connection recovery with exponential backoff (1s, 2s, 4s, max 60s) upon communication loss. |
| ASIL | B |
| Source | {SG-008} |
| Rationale | Automatic recovery |
| Acceptance Criteria | Reconnection attempts observed |
| Verification Method | Integration Test |

### FSR-036: State Synchronization on Reconnect

| Attribute | Value |
|-----------|-------|
| ID | FSR-036 |
| Title | State Synchronization on Reconnect |
| Description | The system **shall** perform full state synchronization between server and agent upon connection recovery. |
| ASIL | B |
| Source | {SG-008} |
| Rationale | Ensures consistent state |
| Acceptance Criteria | States synchronized after reconnect |
| Verification Method | Integration Test |

---

## 11. SG-009: Resource Availability Requirements

### FSR-037: Resource Monitoring

| Attribute | Value |
|-----------|-------|
| ID | FSR-037 |
| Title | Resource Monitoring |
| Description | The system **shall** monitor CPU and memory availability per agent every 2 seconds. |
| ASIL | A |
| Source | {SG-009} |
| Rationale | Resource awareness for scheduling |
| Acceptance Criteria | Metrics updated every 2s |
| Verification Method | Integration Test |

### FSR-038: Resource Threshold Warning

| Attribute | Value |
|-----------|-------|
| ID | FSR-038 |
| Title | Resource Threshold Warning |
| Description | The system **shall** generate warnings when resource usage exceeds 80% of available capacity. |
| ASIL | A |
| Source | {SG-009} |
| Rationale | Early warning of resource pressure |
| Acceptance Criteria | Warning generated at threshold |
| Verification Method | Integration Test |

### FSR-039: Resource Limit Enforcement

| Attribute | Value |
|-----------|-------|
| ID | FSR-039 |
| Title | Resource Limit Enforcement |
| Description | The system **shall** enforce configured resource limits for each workload through container runtime mechanisms. |
| ASIL | A |
| Source | {SG-009} |
| Rationale | Prevents resource monopolization |
| Acceptance Criteria | Limits enforced by runtime |
| Verification Method | Integration Test |

---

## 12. SG-010: Spurious Restart Prevention Requirements

### FSR-040: Health Check Confirmation

| Attribute | Value |
|-----------|-------|
| ID | FSR-040 |
| Title | Health Check Confirmation |
| Description | The system **shall** require at least 2 consecutive failed health checks before declaring workload failure. |
| ASIL | A |
| Source | {SG-010} |
| Rationale | Prevents false positive failures |
| Acceptance Criteria | Single failure does not trigger restart |
| Verification Method | Unit Test |

### FSR-041: Exit Code Validation

| Attribute | Value |
|-----------|-------|
| ID | FSR-041 |
| Title | Exit Code Validation |
| Description | The system **shall** distinguish between workload exit codes to determine if restart is appropriate per policy. |
| ASIL | A |
| Source | {SG-010} |
| Rationale | Correct restart behavior |
| Acceptance Criteria | Exit codes interpreted correctly |
| Verification Method | Unit Test |

### FSR-042: Restart Reason Logging

| Attribute | Value |
|-----------|-------|
| ID | FSR-042 |
| Title | Restart Reason Logging |
| Description | The system **shall** log the specific reason (exit code, signal, health check) for each workload restart. |
| ASIL | A |
| Source | {SG-010} |
| Rationale | Debugging and monitoring |
| Acceptance Criteria | Restart reason logged |
| Verification Method | Review |

---

## 13. Requirements Traceability

### 13.1 Safety Goal to FSR Traceability

| Safety Goal | FSR IDs |
|-------------|---------|
| SG-001 | FSR-001, FSR-002, FSR-003, FSR-004, FSR-005 |
| SG-002 | FSR-006, FSR-007, FSR-008, FSR-009 |
| SG-003 | FSR-010, FSR-011, FSR-012, FSR-013, FSR-014 |
| SG-004 | FSR-015, FSR-016, FSR-017, FSR-018, FSR-019 |
| SG-005 | FSR-020, FSR-021, FSR-022, FSR-023, FSR-024 |
| SG-006 | FSR-025, FSR-026, FSR-027, FSR-028 |
| SG-007 | FSR-029, FSR-030, FSR-031, FSR-032 |
| SG-008 | FSR-033, FSR-034, FSR-035, FSR-036 |
| SG-009 | FSR-037, FSR-038, FSR-039 |
| SG-010 | FSR-040, FSR-041, FSR-042 |

### 13.2 FSR to Component Allocation

| FSR Range | Server | Agent | Communication |
|-----------|--------|-------|---------------|
| FSR-001 to FSR-005 | ✓ | ✓ | |
| FSR-006 to FSR-009 | | ✓ | |
| FSR-010 to FSR-014 | ✓ | ✓ | |
| FSR-015 to FSR-019 | | ✓ | |
| FSR-020 to FSR-024 | ✓ | ✓ | |
| FSR-025 to FSR-028 | ✓ | ✓ | |
| FSR-029 to FSR-032 | ✓ | ✓ | ✓ |
| FSR-033 to FSR-036 | ✓ | ✓ | ✓ |
| FSR-037 to FSR-039 | | ✓ | |
| FSR-040 to FSR-042 | | ✓ | |

### 13.3 Forward Traceability to TSR

| FSR ID | TSR IDs |
|--------|---------|
| FSR-001 | TSR-001, TSR-002 |
| FSR-002 | TSR-003, TSR-004 |
| FSR-003 | TSR-005 |
| FSR-004 | TSR-006 |
| FSR-005 | TSR-007 |
| FSR-006 | TSR-008, TSR-009 |
| FSR-007 | TSR-010 |
| FSR-008 | TSR-011, TSR-012 |
| FSR-009 | TSR-013 |
| FSR-010 | TSR-014, TSR-015 |
| FSR-011 | TSR-016 |
| FSR-012 | TSR-017 |
| FSR-013 | TSR-018 |
| FSR-014 | TSR-019 |
| FSR-015 | TSR-020, TSR-021 |
| FSR-016 | TSR-022 |
| FSR-017 | TSR-023 |
| FSR-018 | TSR-024 |
| FSR-019 | TSR-025 |
| FSR-020 | TSR-026 |
| FSR-021 | TSR-027 |
| FSR-022 | TSR-028 |
| FSR-023 | TSR-029 |
| FSR-024 | TSR-030 |
| FSR-025 | TSR-031 |
| FSR-026 | TSR-032 |
| FSR-027 | TSR-033 |
| FSR-028 | TSR-034 |
| FSR-029 | TSR-035 |
| FSR-030 | TSR-036 |
| FSR-031 | TSR-037 |
| FSR-032 | TSR-038 |
| FSR-033 | TSR-039, TSR-040 |
| FSR-034 | TSR-041 |
| FSR-035 | TSR-042 |
| FSR-036 | TSR-043 |
| FSR-037 | TSR-044 |
| FSR-038 | TSR-045 |
| FSR-039 | TSR-046 |
| FSR-040 | TSR-047 |
| FSR-041 | TSR-048 |
| FSR-042 | TSR-049 |

---

## 14. References

### 14.1 Input Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-ID-001 | Item Definition |
| ANKAIOS-HARA-001 | Hazard Analysis and Risk Assessment |
| ISO 26262-3:2018 | Concept Phase |

### 14.2 Output Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-TSR-001 | Technical Safety Requirements |
| ANKAIOS-FSC-001 | Functional Safety Concept |

---

## Appendix A: Requirements Attributes Definition

| Attribute | Description |
|-----------|-------------|
| ID | Unique requirement identifier |
| Title | Short descriptive name |
| Description | Full requirement text |
| ASIL | Safety integrity level |
| Source | Parent requirement or safety goal |
| Rationale | Justification for requirement |
| Acceptance Criteria | Testable success criteria |
| Verification Method | How requirement will be verified |

---

## Appendix B: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Safety Team | Initial release |

---

*Document approved for ISO 26262 compliance activities.*
