# Hazard Analysis and Risk Assessment (HARA)

## Eclipse Ankaios Workload Orchestrator

| Document Information | |
|---------------------|---|
| Document ID | ANKAIOS-HARA-001 |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Related Item Definition | ANKAIOS-ID-001 |
| Author | Safety Engineering Team |

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [HARA Methodology](#2-hara-methodology)
3. [Operational Situations](#3-operational-situations)
4. [Hazard Identification](#4-hazard-identification)
5. [Risk Assessment](#5-risk-assessment)
6. [ASIL Determination](#6-asil-determination)
7. [Safety Goals](#7-safety-goals)
8. [Safe States](#8-safe-states)
9. [Traceability](#9-traceability)
10. [References](#10-references)

---

## 1. Introduction

### 1.1 Purpose

This Hazard Analysis and Risk Assessment (HARA) document identifies potential hazards associated with Eclipse Ankaios workload orchestrator malfunctions, assesses their risks, determines appropriate ASIL levels, and defines safety goals to mitigate identified risks.

### 1.2 Scope

This HARA covers:
- All functional behaviors of the Ankaios system
- All identified operational situations
- Hazards arising from malfunctioning behavior
- Safety goals to prevent or mitigate hazardous events

### 1.3 Assumptions

| ID | Assumption |
|----|------------|
| A-HARA-001 | Ankaios manages safety-critical ADAS workloads (ASIL B-D) |
| A-HARA-002 | Vehicle is operational on public roads |
| A-HARA-003 | Multiple occupants may be present |
| A-HARA-004 | Other road users may be affected |
| A-HARA-005 | Managed workloads include vehicle control functions |

---

## 2. HARA Methodology

### 2.1 Process Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         HARA PROCESS FLOW                                │
│                                                                          │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌──────────┐ │
│  │ Operational │───▶│   Hazard    │───▶│    Risk     │───▶│  Safety  │ │
│  │ Situations  │    │Identification│   │ Assessment  │    │  Goals   │ │
│  └─────────────┘    └─────────────┘    └─────────────┘    └──────────┘ │
│                                               │                         │
│                                               ▼                         │
│                                        ┌─────────────┐                  │
│                                        │    ASIL     │                  │
│                                        │Determination│                  │
│                                        └─────────────┘                  │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Severity Classification (S)

| Level | Description | Example |
|-------|-------------|---------|
| S0 | No injuries | Comfort functions fail |
| S1 | Light to moderate injuries | Minor collision at low speed |
| S2 | Severe to life-threatening injuries (survival probable) | Collision with injuries |
| S3 | Life-threatening to fatal injuries (survival uncertain) | High-speed collision |

### 2.3 Exposure Classification (E)

| Level | Description | Probability |
|-------|-------------|-------------|
| E0 | Incredibly rare | < 1% of operating time |
| E1 | Very low probability | 1-2% of operating time |
| E2 | Low probability | 2-10% of operating time |
| E3 | Medium probability | 10-50% of operating time |
| E4 | High probability | > 50% of operating time |

### 2.4 Controllability Classification (C)

| Level | Description | Driver Control |
|-------|-------------|----------------|
| C0 | Controllable in general | > 99% of drivers |
| C1 | Simply controllable | > 99% of drivers |
| C2 | Normally controllable | > 90% of drivers |
| C3 | Difficult to control or uncontrollable | < 90% of drivers |

### 2.5 ASIL Determination Matrix

|     | C1 | C2 | C3 |
|-----|----|----|----|
| **S1, E1** | QM | QM | QM |
| **S1, E2** | QM | QM | QM |
| **S1, E3** | QM | QM | A |
| **S1, E4** | QM | A | B |
| **S2, E1** | QM | QM | QM |
| **S2, E2** | QM | QM | A |
| **S2, E3** | QM | A | B |
| **S2, E4** | A | B | C |
| **S3, E1** | QM | QM | A |
| **S3, E2** | QM | A | B |
| **S3, E3** | A | B | C |
| **S3, E4** | B | C | D |

---

## 3. Operational Situations

### 3.1 Vehicle Operating Modes

| ID | Operating Mode | Description | Frequency |
|----|----------------|-------------|-----------|
| OM-001 | Parked | Vehicle stationary, ignition off | Low |
| OM-002 | Ignition On | Vehicle powered, stationary | Medium |
| OM-003 | Urban Driving | Low speed (< 50 km/h) | High |
| OM-004 | Rural Driving | Medium speed (50-100 km/h) | Medium |
| OM-005 | Highway Driving | High speed (> 100 km/h) | Medium |
| OM-006 | Emergency Maneuver | Collision avoidance active | Low |
| OM-007 | Autonomous Driving | ADAS in control | Medium |

### 3.2 Environmental Conditions

| ID | Condition | Description |
|----|-----------|-------------|
| EC-001 | Normal | Dry road, good visibility |
| EC-002 | Rain | Wet road, reduced visibility |
| EC-003 | Snow/Ice | Slippery road |
| EC-004 | Night | Reduced visibility |
| EC-005 | Heavy Traffic | High vehicle density |

### 3.3 Operational Situation Combinations

| OS ID | Operating Mode | Environment | Exposure |
|-------|----------------|-------------|----------|
| OS-001 | Highway Driving | Normal | E4 |
| OS-002 | Highway Driving | Rain | E3 |
| OS-003 | Highway Driving | Night | E3 |
| OS-004 | Urban Driving | Normal | E4 |
| OS-005 | Urban Driving | Heavy Traffic | E4 |
| OS-006 | Rural Driving | Normal | E3 |
| OS-007 | Autonomous Driving | Any | E3 |
| OS-008 | Emergency Maneuver | Any | E2 |
| OS-009 | Ignition On | Any | E4 |
| OS-010 | Parked | Any | E1 |

---

## 4. Hazard Identification

### 4.1 Malfunctioning Behaviors

| MB ID | Malfunctioning Behavior | Affected Function |
|-------|------------------------|-------------------|
| MB-001 | Failure to start safety-critical workload | F-001 Workload Orchestration |
| MB-002 | Unintended termination of running workload | F-001 Workload Orchestration |
| MB-003 | Incorrect workload state reporting | F-002 State Management |
| MB-004 | Delayed workload startup | F-003 Workload Scheduling |
| MB-005 | Incorrect dependency resolution | F-007 Inter-Workload Dependencies |
| MB-006 | Failure to restart crashed workload | F-005 Failure Recovery |
| MB-007 | Delayed failure detection | F-004 Health Monitoring |
| MB-008 | Incorrect workload configuration | F-006 Configuration Management |
| MB-009 | Unauthorized workload modification | F-009 Access Control |
| MB-010 | Communication loss without safe handling | F-010 Secure Communication |
| MB-011 | Resource exhaustion blocking workloads | F-008 Resource Monitoring |
| MB-012 | Starting wrong workload version | F-001 Workload Orchestration |
| MB-013 | Circular dependency deadlock | F-007 Inter-Workload Dependencies |
| MB-014 | State corruption affecting multiple workloads | F-002 State Management |
| MB-015 | Spurious workload restart | F-005 Failure Recovery |

### 4.2 Hazardous Events Identification

| HE ID | Hazardous Event | Malfunctioning Behavior | Operational Situation |
|-------|-----------------|------------------------|----------------------|
| HE-001 | Loss of ADAS functionality during highway driving | MB-001, MB-002 | OS-001 |
| HE-002 | Loss of ADAS during rain conditions | MB-001, MB-002 | OS-002 |
| HE-003 | Loss of ADAS during emergency maneuver | MB-001, MB-002 | OS-008 |
| HE-004 | Delayed AEB activation | MB-004, MB-007 | OS-008 |
| HE-005 | Incorrect lane keeping during highway driving | MB-003, MB-008 | OS-001 |
| HE-006 | Loss of vehicle control functions | MB-002, MB-006 | OS-001 |
| HE-007 | Unauthorized ADAS parameter change | MB-009 | OS-007 |
| HE-008 | Multiple ADAS failures due to cascade | MB-005, MB-013, MB-014 | OS-001 |
| HE-009 | Degraded ADAS due to resource starvation | MB-011 | OS-007 |
| HE-010 | Wrong software version running | MB-012 | OS-001 |
| HE-011 | ADAS not starting after vehicle start | MB-001, MB-004 | OS-009 |
| HE-012 | False positive workload restart causing disruption | MB-015 | OS-007 |
| HE-013 | Loss of ADAS in urban environment | MB-002, MB-006 | OS-004 |
| HE-014 | Communication failure causing workload isolation | MB-010 | OS-001 |
| HE-015 | Silent failure of monitoring workload | MB-003, MB-007 | OS-007 |

---

## 5. Risk Assessment

### 5.1 Hazardous Event Analysis

#### HE-001: Loss of ADAS functionality during highway driving

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-001: Highway Driving, Normal | Most common ADAS usage scenario |
| Severity | S3 | Loss of ADAS at high speed can lead to fatal collision |
| Exposure | E4 | Highway driving with ADAS is frequent |
| Controllability | C3 | Driver may not be prepared to take over |
| **ASIL** | **D** | Per ASIL matrix (S3, E4, C3) |

#### HE-002: Loss of ADAS during rain conditions

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-002: Highway Driving, Rain | Challenging conditions |
| Severity | S3 | Reduced stopping distance, high risk |
| Exposure | E3 | Rain conditions are medium frequency |
| Controllability | C3 | Difficult control in rain |
| **ASIL** | **C** | Per ASIL matrix (S3, E3, C3) |

#### HE-003: Loss of ADAS during emergency maneuver

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-008: Emergency Maneuver | Critical situation |
| Severity | S3 | Emergency = high collision risk |
| Exposure | E2 | Emergency situations are rare |
| Controllability | C3 | Already in emergency, minimal control margin |
| **ASIL** | **B** | Per ASIL matrix (S3, E2, C3) |

#### HE-004: Delayed AEB activation

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-008: Emergency Maneuver | Time-critical situation |
| Severity | S3 | Delayed braking = collision |
| Exposure | E2 | Emergency situations rare |
| Controllability | C3 | Driver relying on AEB |
| **ASIL** | **B** | Per ASIL matrix (S3, E2, C3) |

#### HE-005: Incorrect lane keeping during highway driving

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-001: Highway Driving | High speed environment |
| Severity | S3 | Lane departure at high speed |
| Exposure | E4 | Common usage scenario |
| Controllability | C2 | Most drivers can correct lane |
| **ASIL** | **C** | Per ASIL matrix (S3, E4, C2) |

#### HE-006: Loss of vehicle control functions

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-001: Highway Driving | High speed |
| Severity | S3 | Complete loss of control |
| Exposure | E4 | Common driving scenario |
| Controllability | C3 | Uncontrollable without functions |
| **ASIL** | **D** | Per ASIL matrix (S3, E4, C3) |

#### HE-007: Unauthorized ADAS parameter change

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-007: Autonomous Driving | ADAS in control |
| Severity | S3 | Incorrect parameters = wrong behavior |
| Exposure | E3 | Autonomous driving medium frequency |
| Controllability | C3 | Driver not monitoring closely |
| **ASIL** | **C** | Per ASIL matrix (S3, E3, C3) |

#### HE-008: Multiple ADAS failures due to cascade

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-001: Highway Driving | Multiple system failure |
| Severity | S3 | Complete ADAS loss |
| Exposure | E4 | Common driving, cascade possible |
| Controllability | C3 | Multiple failures overwhelming |
| **ASIL** | **D** | Per ASIL matrix (S3, E4, C3) |

#### HE-009: Degraded ADAS due to resource starvation

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-007: Autonomous Driving | ADAS active |
| Severity | S2 | Degraded but not lost |
| Exposure | E3 | Resource issues medium frequency |
| Controllability | C2 | Gradual degradation controllable |
| **ASIL** | **A** | Per ASIL matrix (S2, E3, C2) |

#### HE-010: Wrong software version running

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-001: Highway Driving | Any driving scenario |
| Severity | S3 | Unknown behavior with wrong version |
| Exposure | E2 | Version mismatch is rare |
| Controllability | C2 | Behavior may be acceptable |
| **ASIL** | **A** | Per ASIL matrix (S3, E2, C2) |

#### HE-011: ADAS not starting after vehicle start

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-009: Ignition On | Startup scenario |
| Severity | S2 | Driver can choose not to drive |
| Exposure | E4 | Every startup |
| Controllability | C1 | Don't start driving |
| **ASIL** | **A** | Per ASIL matrix (S2, E4, C1) |

#### HE-012: False positive workload restart causing disruption

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-007: Autonomous Driving | ADAS active |
| Severity | S2 | Temporary disruption |
| Exposure | E3 | Medium frequency |
| Controllability | C2 | Driver can take over |
| **ASIL** | **A** | Per ASIL matrix (S2, E3, C2) |

#### HE-013: Loss of ADAS in urban environment

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-004: Urban Driving | Lower speeds |
| Severity | S2 | Lower speed = less severe |
| Exposure | E4 | Common scenario |
| Controllability | C2 | More reaction time |
| **ASIL** | **B** | Per ASIL matrix (S2, E4, C2) |

#### HE-014: Communication failure causing workload isolation

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-001: Highway Driving | High speed |
| Severity | S3 | Isolated workloads may fail |
| Exposure | E3 | Network failures medium |
| Controllability | C2 | Graceful degradation possible |
| **ASIL** | **B** | Per ASIL matrix (S3, E3, C2) |

#### HE-015: Silent failure of monitoring workload

| Parameter | Value | Justification |
|-----------|-------|---------------|
| Operational Situation | OS-007: Autonomous Driving | Monitoring critical |
| Severity | S3 | Undetected failure dangerous |
| Exposure | E3 | Medium frequency |
| Controllability | C3 | No indication to driver |
| **ASIL** | **C** | Per ASIL matrix (S3, E3, C3) |

### 5.2 Risk Assessment Summary Table

| HE ID | Hazardous Event | S | E | C | ASIL |
|-------|-----------------|---|---|---|------|
| HE-001 | Loss of ADAS during highway driving | S3 | E4 | C3 | **D** |
| HE-002 | Loss of ADAS during rain | S3 | E3 | C3 | C |
| HE-003 | Loss of ADAS during emergency | S3 | E2 | C3 | B |
| HE-004 | Delayed AEB activation | S3 | E2 | C3 | B |
| HE-005 | Incorrect lane keeping | S3 | E4 | C2 | C |
| HE-006 | Loss of vehicle control functions | S3 | E4 | C3 | **D** |
| HE-007 | Unauthorized ADAS parameter change | S3 | E3 | C3 | C |
| HE-008 | Multiple ADAS failures (cascade) | S3 | E4 | C3 | **D** |
| HE-009 | Degraded ADAS (resource starvation) | S2 | E3 | C2 | A |
| HE-010 | Wrong software version | S3 | E2 | C2 | A |
| HE-011 | ADAS not starting | S2 | E4 | C1 | A |
| HE-012 | False positive restart | S2 | E3 | C2 | A |
| HE-013 | Loss of ADAS in urban | S2 | E4 | C2 | B |
| HE-014 | Communication failure isolation | S3 | E3 | C2 | B |
| HE-015 | Silent monitoring failure | S3 | E3 | C3 | C |

---

## 6. ASIL Determination

### 6.1 ASIL Summary by Function

| Function | Highest ASIL | Driving Hazards |
|----------|--------------|-----------------|
| F-001 Workload Orchestration | **D** | HE-001, HE-006, HE-008 |
| F-002 State Management | **D** | HE-008 |
| F-003 Workload Scheduling | B | HE-004 |
| F-004 Health Monitoring | C | HE-015 |
| F-005 Failure Recovery | **D** | HE-006 |
| F-006 Configuration Management | C | HE-005 |
| F-007 Inter-Workload Dependencies | **D** | HE-008 |
| F-008 Resource Monitoring | A | HE-009 |
| F-009 Access Control | C | HE-007 |
| F-010 Secure Communication | B | HE-014 |

### 6.2 ASIL Decomposition Consideration

For ASIL D requirements, decomposition may be considered:

| Original | Decomposed To | Method |
|----------|---------------|--------|
| ASIL D | ASIL B(D) + ASIL B(D) | Redundancy |
| ASIL D | ASIL C(D) + ASIL A(D) | Diverse implementation |

Note: (D) suffix indicates ASIL D-level independence requirements apply.

---

## 7. Safety Goals

### 7.1 Safety Goal Definitions

#### SG-001: Ensure Safety-Critical Workload Availability

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-001 |
| Description | Ankaios shall ensure that safety-critical workloads are started and maintained in running state within specified time constraints |
| ASIL | D |
| Safe State | Workload running or safe degradation mode active |
| Fault Tolerant Time | 100ms (workload start), 500ms (failure detection) |
| Related Hazards | HE-001, HE-003, HE-006, HE-011 |

#### SG-002: Prevent Unintended Workload Termination

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-002 |
| Description | Ankaios shall not terminate safety-critical workloads without explicit authorized command or detected workload failure |
| ASIL | D |
| Safe State | Workload continues running |
| Fault Tolerant Time | N/A (prevention) |
| Related Hazards | HE-001, HE-002, HE-006 |

#### SG-003: Maintain State Integrity

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-003 |
| Description | Ankaios shall maintain consistent and correct workload state information |
| ASIL | D |
| Safe State | State validated or error reported |
| Fault Tolerant Time | 100ms (detection) |
| Related Hazards | HE-005, HE-008, HE-015 |

#### SG-004: Ensure Timely Failure Recovery

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-004 |
| Description | Ankaios shall detect workload failures and initiate recovery within specified time |
| ASIL | D |
| Safe State | Recovery initiated or safe degradation |
| Fault Tolerant Time | 500ms (detection), 200ms (recovery initiation) |
| Related Hazards | HE-004, HE-006, HE-007 |

#### SG-005: Prevent Cascade Failures

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-005 |
| Description | Ankaios shall prevent single point failures from cascading to multiple safety-critical workloads |
| ASIL | D |
| Safe State | Failure contained, other workloads unaffected |
| Fault Tolerant Time | N/A (prevention) |
| Related Hazards | HE-008, HE-013, HE-014 |

#### SG-006: Enforce Dependency Ordering

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-006 |
| Description | Ankaios shall start and stop workloads according to specified dependencies without deadlock |
| ASIL | C |
| Safe State | Dependencies satisfied or error reported |
| Fault Tolerant Time | 1000ms (deadlock detection) |
| Related Hazards | HE-004, HE-008 |

#### SG-007: Ensure Authorized Access Only

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-007 |
| Description | Ankaios shall only execute workload operations from authenticated and authorized sources |
| ASIL | C |
| Safe State | Unauthorized operations rejected |
| Fault Tolerant Time | N/A (prevention) |
| Related Hazards | HE-007, HE-010 |

#### SG-008: Maintain Communication Integrity

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-008 |
| Description | Ankaios shall detect communication failures and maintain workload operation during temporary network interruptions |
| ASIL | B |
| Safe State | Local operation continues, reconnection attempted |
| Fault Tolerant Time | 300ms (detection), 10s (graceful degradation) |
| Related Hazards | HE-014 |

#### SG-009: Ensure Resource Availability

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-009 |
| Description | Ankaios shall ensure sufficient resources are available for safety-critical workloads |
| ASIL | A |
| Safe State | Resources reserved or warning issued |
| Fault Tolerant Time | 2s (detection) |
| Related Hazards | HE-009 |

#### SG-010: Prevent Spurious Restarts

| Attribute | Value |
|-----------|-------|
| Safety Goal ID | SG-010 |
| Description | Ankaios shall not unnecessarily restart workloads that are functioning correctly |
| ASIL | A |
| Safe State | Workload continues uninterrupted |
| Fault Tolerant Time | N/A (prevention) |
| Related Hazards | HE-012 |

### 7.2 Safety Goals Summary Table

| SG ID | Safety Goal | ASIL | Primary Hazards |
|-------|-------------|------|-----------------|
| SG-001 | Ensure safety-critical workload availability | D | HE-001, HE-003, HE-006, HE-011 |
| SG-002 | Prevent unintended workload termination | D | HE-001, HE-002, HE-006 |
| SG-003 | Maintain state integrity | D | HE-005, HE-008, HE-015 |
| SG-004 | Ensure timely failure recovery | D | HE-004, HE-006, HE-007 |
| SG-005 | Prevent cascade failures | D | HE-008, HE-013, HE-014 |
| SG-006 | Enforce dependency ordering | C | HE-004, HE-008 |
| SG-007 | Ensure authorized access only | C | HE-007, HE-010 |
| SG-008 | Maintain communication integrity | B | HE-014 |
| SG-009 | Ensure resource availability | A | HE-009 |
| SG-010 | Prevent spurious restarts | A | HE-012 |

### 7.3 Safety Goal Relationships

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     SAFETY GOAL HIERARCHY                                │
│                                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                        ASIL D GOALS                                │  │
│  │                                                                    │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐          │  │
│  │  │ SG-001   │  │ SG-002   │  │ SG-003   │  │ SG-004   │          │  │
│  │  │Workload  │  │Prevent   │  │State     │  │Failure   │          │  │
│  │  │Availabil.│  │Terminat. │  │Integrity │  │Recovery  │          │  │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘          │  │
│  │       │             │             │             │                 │  │
│  │       └─────────────┴──────┬──────┴─────────────┘                 │  │
│  │                            │                                      │  │
│  │                     ┌──────▼──────┐                               │  │
│  │                     │   SG-005    │                               │  │
│  │                     │  Prevent    │                               │  │
│  │                     │  Cascade    │                               │  │
│  │                     └─────────────┘                               │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                    SUPPORTING GOALS (ASIL A-C)                     │  │
│  │                                                                    │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐          │  │
│  │  │ SG-006   │  │ SG-007   │  │ SG-008   │  │ SG-009   │          │  │
│  │  │Dependency│  │ Access   │  │ Commun.  │  │ Resource │          │  │
│  │  │ (ASIL C) │  │ (ASIL C) │  │ (ASIL B) │  │ (ASIL A) │          │  │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘          │  │
│  │                                                                    │  │
│  │                     ┌──────────┐                                  │  │
│  │                     │ SG-010   │                                  │  │
│  │                     │ Spurious │                                  │  │
│  │                     │ (ASIL A) │                                  │  │
│  │                     └──────────┘                                  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 8. Safe States

### 8.1 Safe State Definitions

| SS ID | Safe State | Description | Entry Conditions |
|-------|------------|-------------|------------------|
| SS-001 | Workload Running | Safety-critical workload operational | Normal operation |
| SS-002 | Degraded Operation | Reduced functionality with warning | Partial failure |
| SS-003 | Graceful Degradation | Controlled reduction of features | Communication loss |
| SS-004 | Manual Takeover | Driver assumes control | ADAS failure notification |
| SS-005 | Safe Stop | Vehicle brought to safe standstill | Multiple failures |
| SS-006 | Startup Blocked | Prevent driving without ADAS | Startup failure |

### 8.2 Safe State Transitions

```
                    ┌─────────────────────┐
                    │    SS-001           │
                    │ Workload Running    │
                    │ (Normal Operation)  │
                    └──────────┬──────────┘
                               │
           ┌───────────────────┼───────────────────┐
           │                   │                   │
           ▼                   ▼                   ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│    SS-002       │  │    SS-003       │  │    SS-006       │
│   Degraded      │  │   Graceful      │  │   Startup       │
│   Operation     │  │   Degradation   │  │   Blocked       │
└────────┬────────┘  └────────┬────────┘  └─────────────────┘
         │                    │
         └─────────┬──────────┘
                   │
                   ▼
         ┌─────────────────┐
         │    SS-004       │
         │ Manual Takeover │
         └────────┬────────┘
                  │
                  ▼
         ┌─────────────────┐
         │    SS-005       │
         │   Safe Stop     │
         └─────────────────┘
```

### 8.3 Fault Tolerant Time Intervals

| Transition | From | To | FTTI |
|------------|------|-----|------|
| Failure detection | SS-001 | SS-002/SS-003 | 500ms |
| Degradation notification | SS-002 | SS-004 | 2s |
| Communication timeout | SS-001 | SS-003 | 300ms |
| Recovery timeout | SS-002 | SS-004 | 10s |
| Multiple failure | SS-002 | SS-005 | 5s |
| Startup timeout | - | SS-006 | 30s |

---

## 9. Traceability

### 9.1 Hazard to Safety Goal Traceability

| Hazard ID | Safety Goal ID(s) |
|-----------|-------------------|
| HE-001 | SG-001, SG-002 |
| HE-002 | SG-002 |
| HE-003 | SG-001 |
| HE-004 | SG-004, SG-006 |
| HE-005 | SG-003 |
| HE-006 | SG-001, SG-002, SG-004 |
| HE-007 | SG-004, SG-007 |
| HE-008 | SG-003, SG-005, SG-006 |
| HE-009 | SG-009 |
| HE-010 | SG-007 |
| HE-011 | SG-001 |
| HE-012 | SG-010 |
| HE-013 | SG-005 |
| HE-014 | SG-005, SG-008 |
| HE-015 | SG-003 |

### 9.2 Safety Goal to Function Traceability

| Safety Goal ID | Function ID(s) |
|----------------|----------------|
| SG-001 | F-001, F-003, F-005 |
| SG-002 | F-001, F-009 |
| SG-003 | F-002, F-004 |
| SG-004 | F-004, F-005 |
| SG-005 | F-002, F-007 |
| SG-006 | F-003, F-007 |
| SG-007 | F-009, F-010 |
| SG-008 | F-010 |
| SG-009 | F-008 |
| SG-010 | F-004, F-005 |

### 9.3 Forward Traceability to FSR

| Safety Goal ID | Functional Safety Requirement ID(s) |
|----------------|-------------------------------------|
| SG-001 | FSR-001 through FSR-005 |
| SG-002 | FSR-006 through FSR-009 |
| SG-003 | FSR-010 through FSR-014 |
| SG-004 | FSR-015 through FSR-019 |
| SG-005 | FSR-020 through FSR-024 |
| SG-006 | FSR-025 through FSR-028 |
| SG-007 | FSR-029 through FSR-032 |
| SG-008 | FSR-033 through FSR-036 |
| SG-009 | FSR-037 through FSR-039 |
| SG-010 | FSR-040 through FSR-042 |

---

## 10. References

### 10.1 Input Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-ID-001 | Item Definition |
| ISO 26262-3:2018 | Concept Phase |

### 10.2 Output Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-FSR-001 | Functional Safety Requirements |
| ANKAIOS-FSC-001 | Functional Safety Concept |

---

## Appendix A: Hazard Log

| HE ID | Status | Last Review | Reviewer | Notes |
|-------|--------|-------------|----------|-------|
| HE-001 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-002 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-003 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-004 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-005 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-006 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-007 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-008 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-009 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-010 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-011 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-012 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-013 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-014 | Open | 2026-08-15 | Safety Team | Initial assessment |
| HE-015 | Open | 2026-08-15 | Safety Team | Initial assessment |

---

## Appendix B: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Safety Team | Initial release |

---

*Document approved for ISO 26262 compliance activities.*
