# ISO 26262 ASIL D Certification Guide for Eclipse Ankaios

This document outlines the requirements, processes, and considerations for achieving ISO 26262 ASIL D certification for Eclipse Ankaios with TÜV assessment.

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Safety Management (ISO 26262 Part 2)](#2-safety-management-iso-26262-part-2)
3. [Concept Phase (ISO 26262 Part 3)](#3-concept-phase-iso-26262-part-3)
4. [System Design (ISO 26262 Part 4)](#4-system-design-iso-26262-part-4)
5. [Software Development (ISO 26262 Part 6)](#5-software-development-iso-26262-part-6)
6. [Tool Qualification (ISO 26262 Part 8)](#6-tool-qualification-iso-26262-part-8)
7. [Dependent Failures Analysis (ISO 26262 Part 9)](#7-dependent-failures-analysis-iso-26262-part-9)
8. [Documentation Requirements](#8-documentation-requirements)
9. [Practical Recommendations for Ankaios](#9-practical-recommendations-for-ankaios)
10. [Estimated Effort and Timeline](#10-estimated-effort-and-timeline)
11. [Key Questions to Answer First](#11-key-questions-to-answer-first)
12. [Appendices](#12-appendices)

---

## 1. Executive Summary

### 1.1 Overview

ISO 26262 is the international standard for functional safety of electrical and electronic systems in production automobiles. ASIL D (Automotive Safety Integrity Level D) is the highest safety integrity level, requiring the most rigorous development processes, verification, and documentation.

### 1.2 What is Eclipse Ankaios?

Eclipse Ankaios is a workload and container orchestration platform designed specifically for automotive High Performance Computing (HPC) platforms. It provides:

- Container orchestration for automotive use cases
- Multi-node management with a single API
- Server-agent architecture for distributed workload management
- Support for multiple container runtimes (Podman, containerd, native applications)

### 1.3 Certification Scope

Certifying Ankaios to ASIL D requires comprehensive work across:

- Safety management and planning
- Hazard analysis and risk assessment
- System and software architecture design
- Software development with rigorous verification
- Tool qualification
- Extensive documentation
- Independent assessment by TÜV

### 1.4 Primary Programming Languages

| Language | Usage in Ankaios |
|----------|------------------|
| Rust | Primary language - core system (server, agent, CLI, API, common libraries, gRPC layer) |
| Protocol Buffers | API definitions for gRPC communication between components |
| Python | SDK examples, system tests, tutorials, tooling |
| C++ | Control interface examples |
| JavaScript/Node.js | Control interface examples |
| Shell/Bash | Build scripts, tooling, CI/CD |

### 1.5 Key Challenges

1. **Rust toolchain qualification** - The Rust compiler is not pre-qualified for safety-critical applications
2. **Third-party dependency management** - Extensive crate dependencies require assessment
3. **Architecture partitioning** - Separating safety-critical from non-safety functions
4. **Documentation gap** - Significant documentation effort required for ISO 26262 compliance

---

## 2. Safety Management (ISO 26262 Part 2)

### 2.1 Overview

Part 2 of ISO 26262 specifies the requirements for functional safety management during the entire safety lifecycle. This includes organizational requirements, safety culture, and management of safety activities.

### 2.2 Safety Plan and Organization

#### 2.2.1 Safety Plan Requirements

The safety plan shall define:

- The activities to be performed during the safety lifecycle
- The work products to be generated
- The responsibilities and resources
- The schedule and milestones
- The methods and tools to be used
- The confirmation measures to be applied

#### 2.2.2 Organizational Requirements

| Requirement | Description | ASIL D Applicability |
|-------------|-------------|---------------------|
| Safety culture | Establish and maintain a safety culture within the organization | Mandatory |
| Competence management | Ensure personnel have necessary competencies | Mandatory |
| Quality management | Implement quality management system aligned with safety | Mandatory |
| Safety manager | Appoint a safety manager with appropriate authority | Mandatory |
| Independence | Ensure appropriate independence in safety activities | Mandatory |

#### 2.2.3 Safety Organization Structure

```
┌─────────────────────────────────────────────────────────────┐
│                    Project Management                        │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ Safety Manager │    │ Development   │    │ Quality       │
│               │    │ Team Lead     │    │ Assurance     │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ Safety        │    │ Software      │    │ Independent   │
│ Engineers     │    │ Engineers     │    │ Reviewers     │
└───────────────┘    └───────────────┘    └───────────────┘
```

### 2.3 Confirmation Measures

#### 2.3.1 Types of Confirmation Measures

| Measure | Description | When Applied |
|---------|-------------|--------------|
| Confirmation review | Verification that work products meet requirements | Throughout development |
| Functional safety audit | Assessment of implementation of safety processes | Periodic |
| Functional safety assessment | Judgment of functional safety achievement | Milestone-based |

#### 2.3.2 Independence Requirements for ASIL D

| Activity | Independence Level Required |
|----------|---------------------------|
| Confirmation review | I2 (Different person, different team) or I3 (Different organization) |
| Functional safety audit | I2 or I3 |
| Functional safety assessment | I3 (Different organization - typically TÜV) |

### 2.4 TÜV Engagement Strategy

#### 2.4.1 Engagement Timeline

| Phase | TÜV Activities |
|-------|---------------|
| Planning | Initial consultation, scope definition, assessment planning |
| Development | Interim assessments, process reviews, work product reviews |
| Verification | Test witness, coverage review, analysis review |
| Certification | Final assessment, certificate issuance |

#### 2.4.2 Types of TÜV Certificates

| Certificate Type | Description |
|-----------------|-------------|
| Process Assessment | Confirms development process compliance |
| Product Assessment | Confirms product meets safety requirements |
| Certificate of Conformity | Full ISO 26262 compliance certification |

### 2.5 Configuration Management

#### 2.5.1 Requirements

- Unique identification of all configuration items
- Version control for all work products
- Change management process
- Baseline management
- Build reproducibility

#### 2.5.2 Current Ankaios State

Ankaios currently uses:
- Git for version control
- Cargo.lock for dependency pinning
- GitHub workflows for CI/CD

Additional requirements for ASIL D:
- Formal configuration management plan
- Documented baseline management
- Change impact analysis process
- Traceability of changes to safety requirements

---

## 3. Concept Phase (ISO 26262 Part 3)

### 3.1 Overview

Part 3 covers the concept phase, including item definition, hazard analysis and risk assessment (HARA), and the functional safety concept.

### 3.2 Item Definition

#### 3.2.1 Purpose

The item definition describes Ankaios as a safety-related item within the vehicle E/E architecture, including:

- Functionality and behavior
- Interfaces and interactions
- Environmental conditions
- Legal and regulatory requirements

#### 3.2.2 Ankaios Item Definition Elements

| Element | Description |
|---------|-------------|
| Item name | Eclipse Ankaios Workload Orchestrator |
| Item function | Manage containerized workloads across automotive HPC nodes |
| Item boundaries | Server component, Agent components, CLI, gRPC interfaces |
| Operating conditions | Automotive environment (temperature, vibration, EMC) |
| Dependencies | Container runtimes (Podman, containerd), Linux OS, network |
| Interfaces | gRPC API, Control Interface pipes, configuration files |

#### 3.2.3 System Context Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Vehicle E/E System                                │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    HPC Platform                                  │   │
│  │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐         │   │
│  │  │ ADAS        │    │ Infotainment│    │ Vehicle     │         │   │
│  │  │ Workloads   │    │ Workloads   │    │ Control     │         │   │
│  │  │ (ASIL B-D)  │    │ (QM)        │    │ (ASIL A-C)  │         │   │
│  │  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘         │   │
│  │         │                  │                  │                 │   │
│  │         └──────────────────┼──────────────────┘                 │   │
│  │                            │                                     │   │
│  │                   ┌────────▼────────┐                           │   │
│  │                   │  ANKAIOS        │                           │   │
│  │                   │  Orchestrator   │                           │   │
│  │                   │  ┌───────────┐  │                           │   │
│  │                   │  │  Server   │  │                           │   │
│  │                   │  └─────┬─────┘  │                           │   │
│  │                   │        │        │                           │   │
│  │         ┌─────────┼────────┼────────┼─────────┐                 │   │
│  │         │         │        │        │         │                 │   │
│  │    ┌────▼───┐ ┌───▼────┐ ┌─▼──────┐ │         │                 │   │
│  │    │ Agent  │ │ Agent  │ │ Agent  │ │         │                 │   │
│  │    │ Node 1 │ │ Node 2 │ │ Node N │ │         │                 │   │
│  │    └────┬───┘ └───┬────┘ └───┬────┘ │         │                 │   │
│  │         │         │          │      │         │                 │   │
│  │    ┌────▼───┐ ┌───▼────┐ ┌───▼────┐ │         │                 │   │
│  │    │ Podman │ │contain-│ │ Native │ │         │                 │   │
│  │    │        │ │  erd   │ │  Apps  │ │         │                 │   │
│  │    └────────┘ └────────┘ └────────┘ │         │                 │   │
│  │                                      │         │                 │   │
│  └──────────────────────────────────────┴─────────┘                 │   │
│                                                                      │   │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐        │   │
│  │ Vehicle Bus    │  │ Ethernet       │  │ External       │        │   │
│  │ (CAN/LIN)      │  │ Network        │  │ Interfaces     │        │   │
│  └────────────────┘  └────────────────┘  └────────────────┘        │   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.3 Hazard Analysis and Risk Assessment (HARA)

#### 3.3.1 HARA Process

1. Identify operational situations and operating modes
2. Identify hazardous events
3. Classify hazardous events using S, E, C parameters
4. Determine ASIL for each hazardous event
5. Define safety goals

#### 3.3.2 ASIL Determination Parameters

| Parameter | Description | Levels |
|-----------|-------------|--------|
| Severity (S) | Potential harm to persons | S0, S1, S2, S3 |
| Exposure (E) | Probability of operational situation | E0, E1, E2, E3, E4 |
| Controllability (C) | Ability to avoid harm | C0, C1, C2, C3 |

#### 3.3.3 ASIL Determination Matrix

| | C1 | C2 | C3 |
|---|---|---|---|
| **S1** | | | |
| E1 | QM | QM | QM |
| E2 | QM | QM | QM |
| E3 | QM | QM | A |
| E4 | QM | A | B |
| **S2** | | | |
| E1 | QM | QM | QM |
| E2 | QM | QM | A |
| E3 | QM | A | B |
| E4 | A | B | C |
| **S3** | | | |
| E1 | QM | QM | A |
| E2 | QM | A | B |
| E3 | A | B | C |
| E4 | B | C | D |

#### 3.3.4 Example Hazard Analysis for Ankaios

| ID | Hazardous Event | Operational Situation | S | E | C | ASIL | Safety Goal |
|----|-----------------|----------------------|---|---|---|------|-------------|
| HE-001 | Failure to start safety-critical ADAS workload | Highway driving | S3 | E4 | C3 | D | SG-001: Ankaios shall ensure safety-critical workloads are started within specified time |
| HE-002 | Incorrect termination of vehicle control workload | Any driving | S3 | E4 | C2 | C | SG-002: Ankaios shall not terminate safety-critical workloads without authorization |
| HE-003 | Workload state corruption | Highway driving | S3 | E3 | C3 | C | SG-003: Ankaios shall maintain workload state integrity |
| HE-004 | Server-agent communication loss causing workload failure | Highway driving | S3 | E3 | C2 | B | SG-004: Ankaios shall handle communication failures gracefully |
| HE-005 | Delayed workload restart after failure | Emergency situation | S3 | E2 | C3 | B | SG-005: Ankaios shall restart failed workloads within specified time |
| HE-006 | Resource exhaustion preventing critical workload execution | Any operation | S3 | E3 | C3 | C | SG-006: Ankaios shall ensure resources for safety-critical workloads |
| HE-007 | Incorrect workload scheduling priority | Time-critical operation | S2 | E3 | C2 | A | SG-007: Ankaios shall schedule workloads according to priority |
| HE-008 | Authentication bypass allowing unauthorized workload changes | Any operation | S3 | E2 | C2 | A | SG-008: Ankaios shall authenticate all workload modifications |

### 3.4 Functional Safety Concept

#### 3.4.1 Purpose

The functional safety concept specifies the functional safety requirements and their allocation to system elements to achieve the safety goals.

#### 3.4.2 Functional Safety Requirements Derivation

| Safety Goal | Functional Safety Requirement | Allocation |
|-------------|------------------------------|------------|
| SG-001 | FSR-001: System shall detect workload start failures within 100ms | Agent, Server |
| SG-001 | FSR-002: System shall retry workload start up to 3 times | Agent |
| SG-001 | FSR-003: System shall notify safety monitor of start failure | Server |
| SG-002 | FSR-004: System shall require authorization for workload termination | Server, Agent |
| SG-002 | FSR-005: System shall log all termination requests | Server |
| SG-003 | FSR-006: System shall use checksums for state data | Agent, Server |
| SG-003 | FSR-007: System shall validate state transitions | Agent |
| SG-004 | FSR-008: System shall detect communication loss within 500ms | Agent, Server |
| SG-004 | FSR-009: System shall maintain workload operation during communication loss | Agent |
| SG-005 | FSR-010: System shall monitor workload health | Agent |
| SG-005 | FSR-011: System shall initiate restart within 200ms of failure detection | Agent |
| SG-006 | FSR-012: System shall reserve resources for safety-critical workloads | Agent |
| SG-006 | FSR-013: System shall prevent QM workloads from consuming safety resources | Agent |

#### 3.4.3 Safety Mechanisms

| Mechanism | Description | Safety Goals Addressed |
|-----------|-------------|----------------------|
| Watchdog monitoring | Monitor workload heartbeats | SG-001, SG-005 |
| State machine validation | Validate all state transitions | SG-003 |
| Redundant communication | Dual communication paths | SG-004 |
| Resource isolation | Separate resources for ASIL/QM | SG-006 |
| Authentication | Cryptographic authentication | SG-008 |
| Plausibility checks | Validate all inputs | SG-003 |

---

## 4. System Design (ISO 26262 Part 4)

### 4.1 Overview

Part 4 covers product development at the system level, including the technical safety concept, system design, and system integration.

### 4.2 Technical Safety Concept

#### 4.2.1 Purpose

The technical safety concept specifies the technical safety requirements and system architecture to implement the functional safety concept.

#### 4.2.2 Technical Safety Requirements

| ID | Technical Safety Requirement | Derived From | ASIL |
|----|------------------------------|--------------|------|
| TSR-001 | Server shall send heartbeat to agents every 100ms | FSR-008 | D |
| TSR-002 | Agent shall declare server offline after 3 missed heartbeats | FSR-008 | D |
| TSR-003 | Agent shall continue workload operation for 10s after server offline | FSR-009 | D |
| TSR-004 | Workload state shall include CRC-32 checksum | FSR-006 | D |
| TSR-005 | State transition shall be validated against allowed transitions | FSR-007 | D |
| TSR-006 | Workload start shall be confirmed within 100ms | FSR-001 | D |
| TSR-007 | Workload restart shall be initiated within 200ms of failure | FSR-011 | D |
| TSR-008 | CPU and memory limits shall be enforced per workload | FSR-012, FSR-013 | C |
| TSR-009 | All API calls shall require valid authentication token | FSR-004 | D |
| TSR-010 | Authentication tokens shall use minimum 256-bit keys | FSR-004 | D |

#### 4.2.3 Hardware-Software Interface (HSI)

| HSI Element | Description | Safety Relevance |
|-------------|-------------|------------------|
| Timer/Watchdog | Hardware timer for deadline monitoring | Detect timing violations |
| Memory protection | MMU/MPU for memory isolation | Prevent interference |
| Network interface | Ethernet for gRPC communication | Reliable communication |
| Storage | Persistent storage for state | State recovery |
| CPU cores | Multi-core processor | Workload isolation |

### 4.3 System Architecture

#### 4.3.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         ANKAIOS SYSTEM                                   │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    SAFETY-CRITICAL PARTITION (ASIL D)               │ │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐    │ │
│  │  │ Core State      │  │ Watchdog/       │  │ Safe State      │    │ │
│  │  │ Machine         │  │ Heartbeat       │  │ Handler         │    │ │
│  │  │                 │  │                 │  │                 │    │ │
│  │  │ - State storage │  │ - Timing monitor│  │ - Graceful      │    │ │
│  │  │ - Transitions   │  │ - Deadline check│  │   degradation   │    │ │
│  │  │ - Validation    │  │ - Recovery      │  │ - Error states  │    │ │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘    │ │
│  │                                                                    │ │
│  │  ┌─────────────────┐  ┌─────────────────┐                         │ │
│  │  │ Authentication  │  │ Resource        │                         │ │
│  │  │ Module          │  │ Manager         │                         │ │
│  │  │                 │  │                 │                         │ │
│  │  │ - Token verify  │  │ - Allocation    │                         │ │
│  │  │ - Access control│  │ - Limits        │                         │ │
│  │  └─────────────────┘  └─────────────────┘                         │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                              │ FFI                                      │
│                              │ (Freedom From Interference)              │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    QM PARTITION (Quality Managed)                   │ │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐    │ │
│  │  │ gRPC Server     │  │ Configuration   │  │ Logging         │    │ │
│  │  │                 │  │ Management      │  │                 │    │ │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘    │ │
│  │                                                                    │ │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐    │ │
│  │  │ CLI Interface   │  │ Manifest        │  │ Event           │    │ │
│  │  │                 │  │ Parser          │  │ Handler         │    │ │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 4.3.2 Component Responsibilities

| Component | Partition | ASIL | Responsibilities |
|-----------|-----------|------|------------------|
| Core State Machine | Safety | D | Manage workload states, validate transitions |
| Watchdog/Heartbeat | Safety | D | Monitor timing, detect failures |
| Safe State Handler | Safety | D | Handle failures, graceful degradation |
| Authentication Module | Safety | D | Verify tokens, enforce access control |
| Resource Manager | Safety | C | Allocate resources, enforce limits |
| gRPC Server | QM | QM | Handle API communication |
| Configuration Management | QM | QM | Parse and manage configuration |
| CLI Interface | QM | QM | Command-line user interface |
| Logging | QM | QM | System logging and diagnostics |

### 4.4 Freedom From Interference (FFI)

#### 4.4.1 FFI Requirements

Freedom from interference ensures that QM components cannot negatively affect ASIL components.

| Interference Type | Mitigation | Implementation |
|-------------------|------------|----------------|
| Spatial interference | Memory isolation | Separate processes, address spaces |
| Temporal interference | Time partitioning | CPU quotas, scheduling priority |
| Communication interference | Input validation | Validate all inputs from QM components |
| Resource interference | Resource reservation | Pre-allocate resources for safety partition |

#### 4.4.2 FFI Analysis

| From Component | To Component | Interface | FFI Mechanism |
|----------------|--------------|-----------|---------------|
| gRPC Server (QM) | State Machine (D) | Messages | Input validation, message authentication |
| CLI (QM) | Authentication (D) | API calls | Timeout, input sanitization |
| Config Mgmt (QM) | Resource Mgr (C) | Config data | Schema validation, bounds checking |
| Logging (QM) | All (D) | Shared buffer | Separate buffer, write-only access |

### 4.5 Safety Analysis Methods

#### 4.5.1 Required Analyses for ASIL D

| Analysis Method | Purpose | ASIL D Applicability |
|-----------------|---------|---------------------|
| FMEA (Failure Mode and Effects Analysis) | Identify failure modes and effects | Highly recommended |
| FTA (Fault Tree Analysis) | Analyze causes of top-level hazards | Highly recommended |
| DFA (Dependent Failure Analysis) | Identify dependent failures | Mandatory |
| FMEDA (Failure Modes, Effects, and Diagnostic Analysis) | Quantitative hardware analysis | Recommended |

#### 4.5.2 Example FMEA for Ankaios Server

| Component | Failure Mode | Effect | Severity | Detection | Mitigation |
|-----------|--------------|--------|----------|-----------|------------|
| State Machine | State corruption | Incorrect workload state | High | CRC check | Redundant state storage |
| State Machine | Transition failure | Workload stuck | High | Timeout | Retry with fallback |
| Heartbeat | Missed heartbeat | False offline detection | Medium | Multiple misses required | Configurable threshold |
| Heartbeat | Delayed heartbeat | Late failure detection | High | Timestamp check | Bounded latency |
| Authentication | Token validation failure | Unauthorized access | Critical | Logging | Multiple validation |
| Resource Manager | Over-allocation | Resource exhaustion | High | Monitoring | Pre-reservation |

#### 4.5.3 Example FTA for Safety Goal SG-001

```
                    ┌─────────────────────────────────────┐
                    │ Safety-critical workload not started│
                    │ within specified time (SG-001)      │
                    └─────────────────────┬───────────────┘
                                          │
                              ┌───────────┴───────────┐
                              │         OR            │
                              └───────────┬───────────┘
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
        ┌───────────▼───────────┐  ┌──────▼──────┐  ┌──────────▼──────────┐
        │ Server fails to send  │  │ Agent fails │  │ Container runtime   │
        │ start command         │  │ to receive  │  │ fails to start      │
        └───────────┬───────────┘  └──────┬──────┘  └──────────┬──────────┘
                    │                     │                     │
        ┌───────────┴───┐         ┌───────┴───────┐    ┌───────┴───────┐
        │      OR       │         │      OR       │    │      OR       │
        └───────────┬───┘         └───────┬───────┘    └───────┬───────┘
              ┌─────┴─────┐         ┌─────┴─────┐        ┌─────┴─────┐
              │           │         │           │        │           │
    ┌─────────▼───┐ ┌─────▼─────┐ ┌─▼─────────┐ ┌▼─────┐ ┌▼─────────┐ ┌▼────────┐
    │Server crash │ │gRPC error │ │Network    │ │Agent │ │Image not │ │Resource │
    │             │ │           │ │failure    │ │crash │ │available │ │exhaust  │
    └─────────────┘ └───────────┘ └───────────┘ └──────┘ └──────────┘ └─────────┘
```

---

## 5. Software Development (ISO 26262 Part 6)

### 5.1 Overview

Part 6 is the most comprehensive part for software-intensive products like Ankaios. It covers all aspects of software development from requirements to verification.

### 5.2 Software Safety Requirements Specification

#### 5.2.1 Requirements Specification Methods for ASIL D

| Method | ASIL D Applicability |
|--------|---------------------|
| Natural language | Applicable |
| Informal notation | Applicable |
| Semi-formal notation | Highly recommended |
| Formal notation | Recommended |

#### 5.2.2 Requirements Properties

All software safety requirements shall be:

| Property | Description |
|----------|-------------|
| Unambiguous | Single interpretation |
| Comprehensible | Understandable by all stakeholders |
| Atomic | Single requirement per statement |
| Internally consistent | No contradictions |
| Feasible | Technically achievable |
| Verifiable | Can be tested or analyzed |
| Traceable | Linked to source and verification |

#### 5.2.3 Requirements Traceability

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Safety Goals    │────▶│ Functional      │────▶│ Technical       │
│ (HARA)          │     │ Safety Reqs     │     │ Safety Reqs     │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Test Cases      │◀────│ Software Unit   │◀────│ Software Safety │
│                 │     │ Design          │     │ Requirements    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

#### 5.2.4 Example Software Safety Requirements

| ID | Requirement | Source | ASIL | Verification Method |
|----|-------------|--------|------|---------------------|
| SSR-001 | The state machine shall validate each state transition against the allowed transition table | TSR-005 | D | Unit test, review |
| SSR-002 | The state machine shall reject invalid state transitions and log the attempt | TSR-005 | D | Unit test |
| SSR-003 | The heartbeat module shall send heartbeat messages every 100ms ±10ms | TSR-001 | D | Integration test |
| SSR-004 | The heartbeat module shall use monotonic clock for timing | TSR-001 | D | Review, unit test |
| SSR-005 | The watchdog shall trigger timeout after 300ms without heartbeat | TSR-002 | D | Integration test |
| SSR-006 | The agent shall maintain workload operation for 10s after server offline | TSR-003 | D | System test |
| SSR-007 | State data shall include CRC-32 calculated over all state fields | TSR-004 | D | Unit test |
| SSR-008 | State read shall verify CRC-32 and reject corrupted data | TSR-004 | D | Unit test |
| SSR-009 | Workload start confirmation shall be received within 100ms | TSR-006 | D | Integration test |
| SSR-010 | Failed workload restart shall be initiated within 200ms | TSR-007 | D | Integration test |

### 5.3 Software Architectural Design

#### 5.3.1 Architectural Design Principles for ASIL D

| Principle | ASIL D Applicability | Notes |
|-----------|---------------------|-------|
| Hierarchical structure | Highly recommended | Rust module system supports this |
| Restricted size of software components | Highly recommended | Keep modules focused |
| Restricted size of interfaces | Highly recommended | Minimize public API surface |
| Strong cohesion within components | Highly recommended | Single responsibility |
| Loose coupling between components | Highly recommended | Minimal dependencies |
| Appropriate scheduling | Highly recommended | Prioritize safety functions |
| Restricted use of interrupts | Highly recommended | Use async where possible |
| Appropriate spatial isolation | Highly recommended | Separate processes |

#### 5.3.2 Architectural Design Notations for ASIL D

| Notation | ASIL D Applicability |
|----------|---------------------|
| Natural language | Applicable |
| Informal notation | Applicable |
| Semi-formal notation | Highly recommended |
| Formal notation | Recommended |

#### 5.3.3 Current Ankaios Architecture Components

| Component | Rust Crate | Description | Safety Relevance |
|-----------|------------|-------------|------------------|
| Server | `server` | Central orchestration server | High |
| Agent | `agent` | Node-level workload manager | High |
| CLI | `ank` | Command-line interface | Low |
| Common | `common` | Shared types and utilities | High |
| gRPC | `grpc` | Communication layer | Medium |
| API | `ankaios_api` | API definitions | High |

#### 5.3.4 Architectural Safety Mechanisms

| Mechanism | Description | Implementation Approach |
|-----------|-------------|------------------------|
| Defensive programming | Validate all inputs | Rust type system, explicit validation |
| Error detection | Detect and handle errors | Rust Result type, error propagation |
| Error handling | Graceful degradation | Fallback states, recovery procedures |
| Plausibility checks | Validate data consistency | Range checks, state validation |
| Data integrity | Protect against corruption | CRC, redundant storage |
| Monitoring | Detect abnormal behavior | Watchdogs, heartbeats |

### 5.4 Software Unit Design and Implementation

#### 5.4.1 Design Principles for ASIL D

| Principle | ASIL D Applicability | Rust Support |
|-----------|---------------------|--------------|
| One entry and one exit point | Highly recommended | Early returns allowed with Result |
| No dynamic objects | Highly recommended | Avoid Box, Vec in safety paths |
| No recursion | Highly recommended | Use iteration |
| No dynamic memory allocation | Highly recommended | Use stack or pre-allocated memory |
| No multiple use of variable names | Highly recommended | Enforced by Rust compiler |
| Avoid global variables | Highly recommended | Minimize, use explicit state |

#### 5.4.2 Rust-Specific Considerations

##### 5.4.2.1 Advantages of Rust for Safety

| Feature | Safety Benefit |
|---------|---------------|
| Ownership system | Prevents memory corruption, use-after-free |
| Borrow checker | Prevents data races |
| No null pointers | Prevents null pointer dereferences |
| No undefined behavior (safe Rust) | Predictable execution |
| Strong type system | Catches errors at compile time |
| Pattern matching | Enforces exhaustive handling |
| Result/Option types | Explicit error handling |

##### 5.4.2.2 Challenges of Rust for Safety

| Challenge | Mitigation |
|-----------|------------|
| `unsafe` blocks | Minimize use, document justification, review |
| Third-party crates | Audit, minimize dependencies, vendor critical crates |
| Compiler complexity | Use qualified toolchain (Ferrocene) |
| Panic behavior | Configure no-panic in safety paths |
| Dynamic allocation | Avoid Box, Vec, HashMap in safety paths |

##### 5.4.2.3 `unsafe` Usage Guidelines

```rust
// GUIDELINE: Every unsafe block must have:
// 1. Safety comment explaining why it's safe
// 2. Minimal scope
// 3. Independent review

// Example:
fn read_hardware_register(addr: *const u32) -> u32 {
    // SAFETY: addr is a valid hardware register address
    // verified by the hardware abstraction layer.
    // This read has no side effects and is always safe.
    unsafe { *addr }
}
```

#### 5.4.3 Coding Guidelines for ASIL D

| Guideline | Description | Enforcement |
|-----------|-------------|-------------|
| Use safe Rust | Avoid `unsafe` except where necessary | Clippy, review |
| Explicit error handling | Use Result, no unwrap() in safety code | Clippy lint |
| No panics | Use Result instead of panic!() | Custom lint |
| Bounded loops | All loops must have bounds | Review |
| Input validation | Validate all external inputs | Review, unit test |
| Defensive programming | Check preconditions | Assertions, type system |
| Documentation | Document all public items | Clippy lint |
| Naming conventions | Follow Rust conventions | rustfmt |

#### 5.4.4 Example Safe Rust Patterns

```rust
// Pattern 1: Explicit error handling
fn process_workload_state(state: &[u8]) -> Result<WorkloadState, StateError> {
    // Validate input length
    if state.len() < MIN_STATE_SIZE {
        return Err(StateError::InvalidSize);
    }

    // Validate CRC
    let stored_crc = extract_crc(state)?;
    let calculated_crc = calculate_crc(&state[..state.len()-4]);
    if stored_crc != calculated_crc {
        return Err(StateError::CrcMismatch);
    }

    // Parse state
    parse_state(state)
}

// Pattern 2: Bounded iteration
fn retry_operation<F, T, E>(mut operation: F, max_retries: u32) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut attempts = 0u32;
    loop {
        match operation() {
            Ok(result) => return Ok(result),
            Err(e) if attempts < max_retries => {
                attempts = attempts.saturating_add(1);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

// Pattern 3: State machine with exhaustive matching
enum WorkloadState {
    Pending,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

fn handle_state_transition(
    current: WorkloadState,
    event: StateEvent,
) -> Result<WorkloadState, TransitionError> {
    match (current, event) {
        (WorkloadState::Pending, StateEvent::Start) => Ok(WorkloadState::Starting),
        (WorkloadState::Starting, StateEvent::Started) => Ok(WorkloadState::Running),
        (WorkloadState::Running, StateEvent::Stop) => Ok(WorkloadState::Stopping),
        (WorkloadState::Stopping, StateEvent::Stopped) => Ok(WorkloadState::Stopped),
        (WorkloadState::Starting, StateEvent::Failed) => Ok(WorkloadState::Failed),
        (WorkloadState::Running, StateEvent::Failed) => Ok(WorkloadState::Failed),
        // All other transitions are invalid
        (current, event) => Err(TransitionError::InvalidTransition { current, event }),
    }
}
```

### 5.5 Software Unit Verification

#### 5.5.1 Unit Verification Methods for ASIL D

| Method | ASIL D Applicability |
|--------|---------------------|
| Requirements-based testing | Mandatory |
| Interface testing | Mandatory |
| Fault injection testing | Recommended |
| Resource usage testing | Highly recommended |
| Back-to-back testing | Recommended |

#### 5.5.2 Structural Coverage for ASIL D

| Coverage Metric | ASIL D Applicability |
|-----------------|---------------------|
| Statement coverage | Mandatory |
| Branch coverage | Mandatory |
| MC/DC (Modified Condition/Decision Coverage) | Highly recommended |

#### 5.5.3 MC/DC Explanation

MC/DC requires that:
1. Every point of entry and exit has been invoked
2. Every decision has taken all possible outcomes
3. Each condition in a decision has been shown to independently affect the outcome

Example:
```rust
// Decision: if (a && b) || c
// Requires test cases showing:
// - a independently affects outcome
// - b independently affects outcome
// - c independently affects outcome
```

#### 5.5.4 Unit Test Requirements

| Requirement | Description |
|-------------|-------------|
| Test case traceability | Each test case linked to requirements |
| Test coverage measurement | Measure and report coverage metrics |
| Test independence | Test cases independent of each other |
| Test repeatability | Tests produce same results on re-run |
| Test documentation | Document test cases and results |

#### 5.5.5 Current Ankaios Test Infrastructure

Ankaios uses:
- Rust's built-in test framework
- cargo-nextest for test execution
- Robot Framework for system tests

Additional requirements for ASIL D:
- MC/DC coverage measurement tools
- Requirements-based test case generation
- Formal test documentation

### 5.6 Software Integration and Testing

#### 5.6.1 Integration Testing Methods for ASIL D

| Method | ASIL D Applicability |
|--------|---------------------|
| Requirements-based testing | Mandatory |
| Interface testing | Mandatory |
| Fault injection testing | Highly recommended |
| Resource usage testing | Highly recommended |

#### 5.6.2 Integration Test Strategy

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Integration Testing Levels                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Level 1: Unit Integration                                              │
│  ┌───────┐    ┌───────┐    ┌───────┐                                   │
│  │ Unit  │───▶│ Unit  │───▶│ Unit  │                                   │
│  │   A   │    │   B   │    │   C   │                                   │
│  └───────┘    └───────┘    └───────┘                                   │
│                                                                          │
│  Level 2: Component Integration                                         │
│  ┌─────────────────┐    ┌─────────────────┐                            │
│  │   State Machine │───▶│   Watchdog      │                            │
│  │   Component     │    │   Component     │                            │
│  └─────────────────┘    └─────────────────┘                            │
│                                                                          │
│  Level 3: Subsystem Integration                                         │
│  ┌─────────────────────────────────────┐                               │
│  │          Server Subsystem           │                               │
│  │  ┌─────────┐  ┌─────────┐          │                               │
│  │  │State    │  │Watchdog │          │                               │
│  │  │Machine  │  │         │          │                               │
│  │  └─────────┘  └─────────┘          │                               │
│  └─────────────────────────────────────┘                               │
│              │                                                          │
│              ▼                                                          │
│  Level 4: System Integration                                            │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │                    Complete Ankaios System                       │  │
│  │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │  │
│  │  │   Server    │───▶│    Agent    │───▶│   Runtime   │          │  │
│  │  └─────────────┘    └─────────────┘    └─────────────┘          │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 5.6.3 Integration Test Cases

| ID | Test Case | Level | Components | Verification |
|----|-----------|-------|------------|--------------|
| IT-001 | Server-Agent heartbeat exchange | 4 | Server, Agent | Timing within spec |
| IT-002 | Workload start command flow | 4 | Server, Agent, Runtime | State transitions correct |
| IT-003 | Server offline handling | 4 | Server, Agent | Agent continues operation |
| IT-004 | State synchronization | 4 | Server, Agent | State consistency |
| IT-005 | Authentication flow | 4 | Server, Agent | Access control enforced |
| IT-006 | Workload restart on failure | 4 | Agent, Runtime | Restart within timing |

### 5.7 Verification of Software Safety Requirements

#### 5.7.1 Verification Methods for ASIL D

| Method | ASIL D Applicability |
|--------|---------------------|
| Requirements-based testing | Mandatory |
| Fault injection testing | Highly recommended |
| Back-to-back testing | Recommended |
| Simulation | Recommended |
| Analysis | Highly recommended |
| Review | Mandatory |

#### 5.7.2 Verification Matrix Example

| Requirement ID | Test Cases | Review | Analysis | Status |
|----------------|------------|--------|----------|--------|
| SSR-001 | UT-001, UT-002 | REV-001 | - | Pending |
| SSR-002 | UT-003 | REV-001 | - | Pending |
| SSR-003 | IT-001 | REV-002 | ANA-001 | Pending |
| SSR-004 | UT-004 | REV-002 | - | Pending |
| SSR-005 | IT-001 | REV-002 | - | Pending |

### 5.8 Software Configuration Management

#### 5.8.1 Configuration Items

| Item Type | Examples |
|-----------|----------|
| Source code | All .rs files |
| Configuration | Cargo.toml, config files |
| Build scripts | build.rs, justfile |
| Test code | test modules, Robot files |
| Documentation | Requirements, design docs |
| Tools | Compiler, test tools |
| Work products | Reports, analysis results |

#### 5.8.2 Version Control Requirements

| Requirement | Implementation |
|-------------|----------------|
| Unique identification | Git commit hash |
| Version history | Git log |
| Branching strategy | Documented branching model |
| Baseline management | Git tags |
| Access control | GitHub permissions |
| Change tracking | Git diff, PRs |

---

## 6. Tool Qualification (ISO 26262 Part 8)

### 6.1 Overview

Tools used in safety-related development must be qualified to ensure they don't introduce errors. The qualification effort depends on the tool confidence level.

### 6.2 Tool Classification

#### 6.2.1 Tool Impact (TI)

| TI Level | Description | Example |
|----------|-------------|---------|
| TI1 | No direct impact on safety | Documentation tools |
| TI2 | Direct impact on safety | Compilers, code generators |

#### 6.2.2 Tool Error Detection (TD)

| TD Level | Description | Example |
|----------|-------------|---------|
| TD1 | High error detection | Testing tools with high coverage |
| TD2 | Medium error detection | Limited testing of tool output |
| TD3 | Low error detection | No verification of tool output |

#### 6.2.3 Tool Confidence Level (TCL)

| | TD1 | TD2 | TD3 |
|---|---|---|---|
| TI1 | TCL1 | TCL1 | TCL1 |
| TI2 | TCL1 | TCL2 | TCL3 |

### 6.3 Tool Qualification Methods for ASIL D

| TCL | Qualification Methods |
|-----|----------------------|
| TCL1 | No qualification required |
| TCL2 | Increased confidence from use OR tool development process OR validation |
| TCL3 | Development process AND validation of tool |

### 6.4 Ankaios Development Tools Analysis

| Tool | Category | TI | TD | TCL | Qualification Approach |
|------|----------|----|----|-----|----------------------|
| rustc (Rust compiler) | Compiler | TI2 | TD3 | TCL3 | Use Ferrocene OR extensive qualification |
| cargo | Build system | TI2 | TD3 | TCL3 | Confidence from use + validation |
| protoc | Code generator | TI2 | TD3 | TCL3 | Validation of generated code |
| cargo-nextest | Test runner | TI2 | TD1 | TCL1 | None required |
| clippy | Static analyzer | TI1 | - | TCL1 | None required |
| rustfmt | Formatter | TI1 | - | TCL1 | None required |
| grcov | Coverage tool | TI1 | - | TCL1 | None required |
| Robot Framework | System test | TI2 | TD1 | TCL1 | None required |
| Git | Version control | TI1 | - | TCL1 | None required |

### 6.5 Rust Compiler Qualification Options

#### 6.5.1 Option 1: Use Ferrocene

Ferrocene is an ISO 26262 qualified Rust toolchain.

| Aspect | Details |
|--------|---------|
| Qualification | ISO 26262 up to ASIL D |
| Coverage | rustc, standard library |
| Support | Commercial support available |
| Effort | Low - use pre-qualified toolchain |
| Cost | License fees |

#### 6.5.2 Option 2: Qualify rustc Through Use

| Activity | Description |
|----------|-------------|
| Historical data | Collect evidence of successful use |
| Test suite | Develop comprehensive test suite |
| Bug tracking | Track and analyze compiler bugs |
| Workarounds | Document known issues and workarounds |
| Effort | Very high |

#### 6.5.3 Option 3: Validate Tool Output

| Activity | Description |
|----------|-------------|
| Review generated code | Inspect compiler output |
| Back-to-back testing | Compare behavior across compiler versions |
| Formal verification | Formally verify critical code |
| Effort | Very high, ongoing |

### 6.6 Tool Qualification Documentation

| Document | Contents |
|----------|----------|
| Tool Qualification Plan | Scope, methods, criteria |
| Tool Qualification Report | Results, evidence, conclusion |
| Tool Manual | Usage instructions, known issues |
| Tool Configuration | Settings, versions, options |

---

## 7. Dependent Failures Analysis (ISO 26262 Part 9)

### 7.1 Overview

Part 9 addresses dependent failures that could compromise multiple elements and defeat safety mechanisms.

### 7.2 Types of Dependent Failures

| Type | Description | Example in Ankaios |
|------|-------------|-------------------|
| Common cause failure | Same root cause affects multiple components | Power supply failure affects server and agents |
| Cascading failure | Failure of one element causes failure of another | Server crash causes agent state corruption |
| Common mode failure | Same failure mode in different components | Buffer overflow in both server and agent |

### 7.3 Analysis Methods

#### 7.3.1 Dependent Failure Analysis (DFA)

| Step | Description |
|------|-------------|
| 1 | Identify safety mechanisms |
| 2 | Identify potential dependent failures |
| 3 | Evaluate coupling factors |
| 4 | Define measures to avoid dependent failures |

#### 7.3.2 Common Cause Failure Analysis

| Coupling Factor | Description | Mitigation |
|-----------------|-------------|------------|
| Hardware | Shared hardware resources | Redundant hardware |
| Software | Shared software components | Diversity |
| Environment | Shared environmental conditions | Environmental protection |
| Human | Common design/implementation errors | Independent teams, reviews |

### 7.4 Ankaios Dependent Failure Considerations

| Dependency | Risk | Mitigation |
|------------|------|------------|
| Server-Agent communication | Network failure affects all agents | Local fallback operation |
| Shared gRPC library | Bug affects server and agents | Defensive programming, diversity |
| Common state format | Corruption affects all components | CRC, validation |
| Rust compiler | Bug affects all Rust components | Qualified toolchain, testing |
| Linux OS | OS failure affects all components | Watchdog, health monitoring |

### 7.5 Safety Mechanism Independence

| Safety Mechanism | Independence Requirement | Implementation |
|------------------|------------------------|----------------|
| Heartbeat monitoring | Independent from main state machine | Separate module |
| CRC validation | Independent calculation | Different algorithm instance |
| Timeout handling | Independent timing source | Hardware timer |
| Redundant storage | Independent storage locations | Separate files/memory |

---

## 8. Documentation Requirements

### 8.1 Overview

ISO 26262 ASIL D requires comprehensive documentation. This section lists all required work products.

### 8.2 Safety Management Work Products (Part 2)

| Work Product | Description | ASIL D Requirement |
|--------------|-------------|-------------------|
| Safety plan | Plan for safety activities | Mandatory |
| Safety case | Argument for safety achievement | Mandatory |
| Confirmation review report | Review results | Mandatory |
| Functional safety audit report | Audit results | Mandatory |
| Functional safety assessment report | Assessment results | Mandatory |

### 8.3 Concept Phase Work Products (Part 3)

| Work Product | Description | ASIL D Requirement |
|--------------|-------------|-------------------|
| Item definition | Description of the item | Mandatory |
| Hazard analysis and risk assessment | HARA results | Mandatory |
| Functional safety concept | Safety requirements allocation | Mandatory |

### 8.4 System Design Work Products (Part 4)

| Work Product | Description | ASIL D Requirement |
|--------------|-------------|-------------------|
| Technical safety concept | Technical requirements | Mandatory |
| System design specification | System architecture | Mandatory |
| Hardware-software interface specification | HSI | Mandatory |
| Safety analysis report (FMEA/FTA) | Analysis results | Mandatory |
| Dependent failure analysis report | DFA results | Mandatory |

### 8.5 Software Development Work Products (Part 6)

| Work Product | Description | ASIL D Requirement |
|--------------|-------------|-------------------|
| Software safety requirements specification | Requirements | Mandatory |
| Software architectural design specification | Architecture | Mandatory |
| Software unit design specification | Unit design | Mandatory |
| Software unit implementation | Source code | Mandatory |
| Software unit verification report | Unit test results | Mandatory |
| Software integration and testing report | Integration results | Mandatory |
| Software verification report | Overall verification | Mandatory |

### 8.6 Tool Qualification Work Products (Part 8)

| Work Product | Description | ASIL D Requirement |
|--------------|-------------|-------------------|
| Tool qualification plan | Qualification approach | As needed per TCL |
| Tool qualification report | Qualification evidence | As needed per TCL |

### 8.7 Documentation Templates

#### 8.7.1 Software Safety Requirement Template

```
Requirement ID: SSR-XXX
Title: [Short descriptive title]
Description: [Detailed requirement text]
Rationale: [Why this requirement exists]
Source: [Parent requirement ID]
ASIL: [A/B/C/D]
Verification Method: [Test/Review/Analysis]
Verification Criteria: [Pass/fail criteria]
Status: [Draft/Reviewed/Approved]
```

#### 8.7.2 Test Case Template

```
Test Case ID: TC-XXX
Title: [Short descriptive title]
Requirement(s): [Linked requirements]
Preconditions: [Required setup]
Test Steps:
  1. [Step description]
  2. [Step description]
Expected Results: [Expected behavior]
Actual Results: [Observed behavior]
Pass/Fail: [Result]
Tester: [Name]
Date: [Date]
```

---

## 9. Practical Recommendations for Ankaios

### 9.1 Immediate Actions

#### 9.1.1 Gap Analysis

Conduct a comprehensive gap analysis comparing current Ankaios development practices against ISO 26262 Part 6 requirements.

| Area | Current State | Gap | Priority |
|------|---------------|-----|----------|
| Requirements traceability | Partial (requirement-tracing workflow) | Need safety requirements | High |
| Architecture documentation | README files | Need formal design docs | High |
| Unit test coverage | Good | Need MC/DC measurement | Medium |
| Code review | PR-based | Need independence documentation | Medium |
| Tool qualification | None | Need rustc qualification | High |
| Documentation | Technical docs | Need safety work products | High |

#### 9.1.2 `unsafe` Code Audit

Audit all `unsafe` blocks in Ankaios:

```bash
# Find all unsafe blocks
grep -rn "unsafe" --include="*.rs" .

# Count unsafe blocks per crate
for crate in agent server common grpc ankaios_api ank; do
  echo "$crate: $(grep -rn 'unsafe' $crate/src --include='*.rs' | wc -l)"
done
```

For each `unsafe` block:
1. Document justification
2. Verify safety invariants
3. Add safety comments
4. Review with independent reviewer

#### 9.1.3 Dependency Audit

Audit all third-party crates in Cargo.lock:

| Category | Action |
|----------|--------|
| Safety-critical path | Minimize dependencies, audit thoroughly |
| QM path | Standard security audit |
| Test dependencies | No qualification needed |

### 9.2 Architecture Partitioning

#### 9.2.1 Recommended Partition Strategy

Separate Ankaios into safety-critical and QM partitions:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    SAFETY-CRITICAL PARTITION (ASIL D)                    │
│                    Minimal, verified, deterministic                      │
│                                                                          │
│  Components:                                                             │
│  - Core state machine (workload state management)                       │
│  - Watchdog/heartbeat mechanism                                         │
│  - Safe state transition logic                                          │
│  - Authentication validation                                            │
│  - Resource allocation enforcement                                       │
│                                                                          │
│  Characteristics:                                                        │
│  - Minimal external dependencies                                         │
│  - No dynamic memory allocation                                          │
│  - No unsafe code (or fully justified)                                  │
│  - Complete MC/DC coverage                                               │
│  - Formal verification where practical                                   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              │ Freedom From Interference (FFI)
                              │ - Input validation at boundary
                              │ - Memory isolation
                              │ - Timing isolation
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                       QM PARTITION                                       │
│                    Full functionality, standard quality                  │
│                                                                          │
│  Components:                                                             │
│  - gRPC communication layer                                              │
│  - CLI (ank)                                                            │
│  - Configuration management                                              │
│  - Logging and diagnostics                                               │
│  - Non-safety workload management                                        │
│  - Manifest parsing                                                      │
│  - Event handling                                                        │
│                                                                          │
│  Characteristics:                                                        │
│  - Standard development practices                                        │
│  - Can use full Rust ecosystem                                          │
│  - Standard test coverage                                                │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 9.2.2 FFI Implementation

| FFI Mechanism | Implementation |
|---------------|----------------|
| Process isolation | Separate safety process |
| Memory protection | No shared memory, message passing |
| Input validation | Validate all messages from QM |
| Timeout handling | Bound response times from QM |
| Resource limits | Separate resource pools |

### 9.3 Ferrocene Adoption Considerations

#### 9.3.1 Benefits

| Benefit | Description |
|---------|-------------|
| Pre-qualified | ISO 26262 qualified up to ASIL D |
| Reduced effort | No need to qualify rustc yourself |
| Ongoing support | Maintained by Ferrous Systems |
| Standard library | Qualified standard library |

#### 9.3.2 Considerations

| Consideration | Description |
|---------------|-------------|
| Cost | License fees required |
| Version | May lag behind stable Rust |
| Platform | Check platform support |
| Integration | Integrate into build system |

#### 9.3.3 Adoption Steps

1. Contact Ferrous Systems for evaluation
2. Assess platform compatibility
3. Test Ankaios build with Ferrocene
4. Evaluate feature compatibility
5. Plan migration strategy

### 9.4 Verification Enhancement

#### 9.4.1 Coverage Tools

| Tool | Purpose | Action |
|------|---------|--------|
| grcov | Line/branch coverage | Currently used |
| llvm-cov | MC/DC coverage | Evaluate for adoption |
| cargo-mutants | Mutation testing | Evaluate for test quality |

#### 9.4.2 Static Analysis

| Tool | Purpose | Action |
|------|---------|--------|
| clippy | Lint checks | Currently used, expand rules |
| cargo-audit | Security vulnerabilities | Add to CI |
| cargo-deny | License/dependency checks | Add to CI |
| MIRAI | Abstract interpretation | Evaluate for safety code |

#### 9.4.3 Testing Strategy Enhancement

| Current | Enhancement |
|---------|-------------|
| Unit tests | Add requirements traceability |
| Integration tests | Add fault injection |
| System tests (Robot) | Add timing verification |
| - | Add MC/DC coverage measurement |
| - | Add back-to-back testing |

---

## 10. Estimated Effort and Timeline

### 10.1 Phase Overview

| Phase | Duration | Activities |
|-------|----------|------------|
| Phase 1: Planning | 2-3 months | Gap analysis, planning, TÜV engagement |
| Phase 2: Process | 3-6 months | Establish safety processes, CM, QM |
| Phase 3: Analysis | 3-4 months | HARA, FSC, TSC, architecture |
| Phase 4: Architecture | 6-12 months | Partitioning, FFI, refactoring |
| Phase 5: Documentation | 6-12 months | Create all work products (parallel) |
| Phase 6: Verification | 6-12 months | Testing, coverage, tool qualification |
| Phase 7: Assessment | 3-6 months | TÜV reviews, audits, certification |
| **Total** | **2-4 years** | Depending on scope and resources |

### 10.2 Detailed Phase Breakdown

#### Phase 1: Planning and Preparation (2-3 months)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Gap analysis | 4 weeks | Gap analysis report |
| Safety planning | 4 weeks | Safety plan |
| TÜV engagement | 2 weeks | Assessment agreement |
| Team training | 4 weeks | Training records |
| Tooling setup | 2 weeks | Tool infrastructure |

#### Phase 2: Process Establishment (3-6 months)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Safety management setup | 4 weeks | Organization structure |
| CM process | 4 weeks | CM plan |
| QM process | 4 weeks | QM plan |
| Review process | 2 weeks | Review procedures |
| Change management | 2 weeks | Change process |

#### Phase 3: Safety Analysis (3-4 months)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Item definition | 2 weeks | Item definition document |
| HARA | 4 weeks | HARA report |
| FSC development | 4 weeks | FSC document |
| TSC development | 4 weeks | TSC document |
| Safety analysis (FMEA/FTA) | 4 weeks | Analysis reports |

#### Phase 4: Architecture and Implementation (6-12 months)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Architecture design | 8 weeks | Architecture specification |
| Partitioning implementation | 12 weeks | Partitioned code |
| FFI implementation | 8 weeks | FFI mechanisms |
| Safety mechanisms | 8 weeks | Safety code |
| Integration | 4 weeks | Integrated system |

#### Phase 5: Documentation (6-12 months, parallel)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Requirements specification | 8 weeks | Requirements document |
| Design specification | 8 weeks | Design document |
| Test specification | 8 weeks | Test specification |
| Analysis documentation | 4 weeks | Analysis reports |
| Safety case | 8 weeks | Safety case |

#### Phase 6: Verification and Validation (6-12 months)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Unit testing | 12 weeks | Test results, coverage |
| Integration testing | 8 weeks | Integration test results |
| System testing | 8 weeks | System test results |
| Tool qualification | 8 weeks | Tool qualification reports |
| Verification review | 4 weeks | Verification report |

#### Phase 7: Assessment and Certification (3-6 months)

| Activity | Duration | Deliverables |
|----------|----------|--------------|
| Internal assessment | 4 weeks | Internal assessment report |
| TÜV interim review | 4 weeks | Review findings |
| Finding resolution | 8 weeks | Updated work products |
| Final assessment | 4 weeks | Assessment report |
| Certification | 4 weeks | Certificate |

### 10.3 Resource Requirements

#### Personnel

| Role | FTE | Duration |
|------|-----|----------|
| Safety Manager | 1 | Full project |
| Safety Engineers | 2 | Full project |
| Software Engineers | 4-6 | Phase 4-6 |
| Test Engineers | 2-3 | Phase 5-6 |
| Documentation Specialist | 1 | Phase 3-6 |
| QA Engineer | 1 | Full project |

#### External Resources

| Resource | Purpose |
|----------|---------|
| TÜV assessor | Assessment, certification |
| Safety consultant | Process guidance, reviews |
| Ferrocene license | Qualified toolchain (if adopted) |
| Training provider | ISO 26262 training |

### 10.4 Cost Estimates

| Category | Estimate Range | Notes |
|----------|----------------|-------|
| Personnel | High | Depends on team location |
| TÜV assessment | €100k - €300k | Depends on scope |
| Ferrocene license | Contact vendor | Annual fee |
| Training | €20k - €50k | Per person, depends on course |
| Tools | €10k - €50k | Coverage tools, analysis tools |
| External consultants | €50k - €200k | Optional, depends on gaps |

---

## 11. Key Questions to Answer First

Before beginning the certification journey, answer these critical questions:

### 11.1 Scope Questions

| Question | Options | Impact |
|----------|---------|--------|
| What is the safety scope? | Full system vs. specific functions | Determines effort |
| Which ASIL level is required? | A, B, C, or D | Determines rigor |
| Is partitioning acceptable? | Safety core + QM | Reduces scope |
| What workloads will be managed? | ADAS, infotainment, etc. | Determines ASIL |

### 11.2 Integration Questions

| Question | Options | Impact |
|----------|---------|--------|
| How does Ankaios fit in vehicle architecture? | Central orchestrator vs. domain-specific | Integration effort |
| What runtimes will be supported? | Podman, containerd, native | Runtime qualification |
| What is the target platform? | x86, ARM, specific HPC | Platform-specific concerns |
| What OS will be used? | Linux, safety OS | OS qualification |

### 11.3 Technical Questions

| Question | Options | Impact |
|----------|---------|--------|
| Can we use Ferrocene? | Yes/No | Tool qualification effort |
| Can we partition the architecture? | Yes/No | Architecture rework |
| Can we minimize unsafe code? | Yes/partially | Code rework |
| Can we reduce dependencies? | Yes/partially | Dependency management |

### 11.4 Organizational Questions

| Question | Options | Impact |
|----------|---------|--------|
| Do we have safety expertise? | Internal vs. external | Training/hiring |
| Which TÜV organization? | TÜV SÜD, TÜV Rheinland, etc. | Relationship building |
| What is the timeline? | 2 years, 3 years, 4 years | Resource planning |
| What is the budget? | Amount | Scope decisions |

### 11.5 Existing Evidence Questions

| Question | Current State | Reusability |
|----------|---------------|-------------|
| What testing exists? | Unit tests, system tests | High potential |
| What documentation exists? | README, design docs | Some potential |
| What reviews are performed? | PR reviews | Medium potential |
| What traceability exists? | Requirement tracing workflow | Good potential |

---

## 12. Appendices

### Appendix A: ISO 26262 Parts Overview

| Part | Title | Relevance to Ankaios |
|------|-------|---------------------|
| Part 1 | Vocabulary | Definitions |
| Part 2 | Management of functional safety | Safety organization |
| Part 3 | Concept phase | HARA, FSC |
| Part 4 | Product development: system level | System design |
| Part 5 | Product development: hardware level | Limited (mostly software) |
| Part 6 | Product development: software level | Primary focus |
| Part 7 | Production and operation | Post-development |
| Part 8 | Supporting processes | Tool qualification |
| Part 9 | ASIL-oriented and safety-oriented analyses | DFA |
| Part 10 | Guidelines | Implementation guidance |
| Part 11 | Guidelines on application of ISO 26262 to semiconductors | Limited relevance |
| Part 12 | Adaptation of ISO 26262 for motorcycles | Not applicable |

### Appendix B: ASIL D Requirements Summary

| Requirement Category | Key Requirements |
|---------------------|------------------|
| Documentation | Complete, reviewed, traced |
| Reviews | Independent (I2 or I3) |
| Testing | Requirements-based, MC/DC coverage |
| Analysis | FMEA, FTA, DFA mandatory |
| Tools | TCL3 tools require qualification |
| Verification | Multiple methods required |

### Appendix C: Glossary

| Term | Definition |
|------|------------|
| ASIL | Automotive Safety Integrity Level |
| DFA | Dependent Failure Analysis |
| FFI | Freedom From Interference |
| FMEA | Failure Mode and Effects Analysis |
| FSC | Functional Safety Concept |
| FTA | Fault Tree Analysis |
| HARA | Hazard Analysis and Risk Assessment |
| HSI | Hardware-Software Interface |
| MC/DC | Modified Condition/Decision Coverage |
| QM | Quality Management (non-safety) |
| TCL | Tool Confidence Level |
| TI | Tool Impact |
| TSC | Technical Safety Concept |

### Appendix D: References

| Document | Description |
|----------|-------------|
| ISO 26262:2018 | Road vehicles - Functional safety |
| IEC 61508 | Functional safety of E/E/PE systems |
| MISRA C | C coding guidelines |
| AUTOSAR | Automotive software architecture |
| Ferrocene | Qualified Rust toolchain |

### Appendix E: Useful Resources

| Resource | URL |
|----------|-----|
| Ferrocene | https://ferrocene.dev |
| TÜV SÜD | https://www.tuvsud.com |
| TÜV Rheinland | https://www.tuv.com |
| Rust Safety | https://rust-lang.github.io/unsafe-code-guidelines/ |
| Eclipse Ankaios | https://eclipse-ankaios.github.io/ankaios |

---

## Document Information

| Field | Value |
|-------|-------|
| Document Title | ISO 26262 ASIL D Certification Guide for Eclipse Ankaios |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Author | Generated with Bath Hex |
| Review Status | Pending |

---

## Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Bath Hex | Initial document creation |

---

*This document provides guidance for ISO 26262 ASIL D certification. It should be reviewed and adapted by qualified functional safety professionals before implementation.*
