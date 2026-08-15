# Cybersecurity Requirements

## Eclipse Ankaios Workload Orchestrator

| Document Information | |
|---------------------|---|
| Document ID | ANKAIOS-CSR-001 |
| Version | 1.0 |
| Date | 2026-08-15 |
| Status | Initial Draft |
| Standard Reference | ISO/SAE 21434:2021, UN R155 |
| Author | Security Engineering Team |

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Threat Analysis and Risk Assessment](#2-threat-analysis-and-risk-assessment)
3. [Security Goals](#3-security-goals)
4. [Authentication Requirements](#4-authentication-requirements)
5. [Authorization Requirements](#5-authorization-requirements)
6. [Communication Security Requirements](#6-communication-security-requirements)
7. [Data Protection Requirements](#7-data-protection-requirements)
8. [Input Validation Requirements](#8-input-validation-requirements)
9. [Secure Development Requirements](#9-secure-development-requirements)
10. [Monitoring and Incident Response](#10-monitoring-and-incident-response)
11. [Update and Patch Management](#11-update-and-patch-management)
12. [Traceability](#12-traceability)
13. [References](#13-references)

---

## 1. Introduction

### 1.1 Purpose

This document specifies cybersecurity requirements for Eclipse Ankaios in accordance with ISO/SAE 21434:2021 and UN R155 regulations. These requirements ensure the security of the workload orchestration system against cyber threats.

### 1.2 Scope

These requirements cover:
- Authentication and authorization
- Communication security
- Data protection
- Secure development practices
- Vulnerability management
- Incident response

### 1.3 Relationship to Safety

Cybersecurity and functional safety are interconnected. Security breaches can lead to safety violations. Requirements are tagged with safety impact level.

| Safety Impact | Description |
|---------------|-------------|
| High | Security breach directly affects safety goals |
| Medium | Security breach may indirectly affect safety |
| Low | Limited safety impact |

### 1.4 Requirements Notation

| Notation | Meaning |
|----------|---------|
| **shall** | Mandatory requirement |
| **should** | Recommended requirement |
| **may** | Optional requirement |
| [CAL X] | Cybersecurity Assurance Level |
| {STRIDE} | Threat category reference |

---

## 2. Threat Analysis and Risk Assessment

### 2.1 Asset Identification

| Asset ID | Asset | Description | Criticality |
|----------|-------|-------------|-------------|
| A-001 | Server State | Desired/actual workload state | Critical |
| A-002 | Server Process | Ankaios server process | Critical |
| A-003 | Agent Process | Ankaios agent process | Critical |
| A-004 | Communication Channel | Server-agent gRPC channel | High |
| A-005 | Certificates | TLS certificates and keys | Critical |
| A-006 | Configuration | Workload manifests and configs | High |
| A-007 | Control Interface | Workload communication pipes | High |
| A-008 | Container Runtime | Podman/Containerd | High |
| A-009 | Workload Images | Container images | Medium |
| A-010 | Logs | Audit and operational logs | Medium |

### 2.2 Threat Identification (STRIDE)

#### 2.2.1 Spoofing

| Threat ID | Threat | Target Asset | Attack Vector |
|-----------|--------|--------------|---------------|
| T-S-001 | Agent impersonation | A-004 | Compromised agent key |
| T-S-002 | Server impersonation | A-004 | MITM attack |
| T-S-003 | Workload identity spoofing | A-007 | Control interface abuse |
| T-S-004 | CLI impersonation | A-004 | Stolen credentials |

#### 2.2.2 Tampering

| Threat ID | Threat | Target Asset | Attack Vector |
|-----------|--------|--------------|---------------|
| T-T-001 | State corruption | A-001 | Malicious state update |
| T-T-002 | Configuration tampering | A-006 | File modification |
| T-T-003 | Command injection | A-008 | Malicious runtime config |
| T-T-004 | Image tampering | A-009 | Supply chain attack |
| T-T-005 | Log tampering | A-010 | Log modification |

#### 2.2.3 Repudiation

| Threat ID | Threat | Target Asset | Attack Vector |
|-----------|--------|--------------|---------------|
| T-R-001 | Unauthorized action denial | A-010 | Missing audit trail |
| T-R-002 | State change denial | A-001 | Missing change tracking |
| T-R-003 | Access denial | A-007 | Missing access logs |

#### 2.2.4 Information Disclosure

| Threat ID | Threat | Target Asset | Attack Vector |
|-----------|--------|--------------|---------------|
| T-I-001 | Communication eavesdropping | A-004 | Unencrypted traffic |
| T-I-002 | Key exposure | A-005 | Key leakage |
| T-I-003 | Configuration exposure | A-006 | Unauthorized access |
| T-I-004 | State information leakage | A-001 | Unauthorized queries |

#### 2.2.5 Denial of Service

| Threat ID | Threat | Target Asset | Attack Vector |
|-----------|--------|--------------|---------------|
| T-D-001 | Server flooding | A-002 | Connection exhaustion |
| T-D-002 | Agent starvation | A-003 | Resource exhaustion |
| T-D-003 | Message flooding | A-004 | Message storm |
| T-D-004 | Control interface DoS | A-007 | Pipe flooding |
| T-D-005 | Runtime exhaustion | A-008 | Container bomb |

#### 2.2.6 Elevation of Privilege

| Threat ID | Threat | Target Asset | Attack Vector |
|-----------|--------|--------------|---------------|
| T-E-001 | Workload escape | A-003, A-008 | Container escape |
| T-E-002 | Control interface privilege | A-007 | Authorization bypass |
| T-E-003 | Server privilege gain | A-002 | Server compromise |
| T-E-004 | Agent privilege gain | A-003 | Agent compromise |

### 2.3 Risk Assessment

| Threat ID | Likelihood | Impact | Risk Level | Safety Impact |
|-----------|------------|--------|------------|---------------|
| T-S-001 | Medium | Critical | High | High |
| T-S-002 | Low | Critical | Medium | High |
| T-S-003 | Medium | High | High | High |
| T-S-004 | Low | High | Medium | Medium |
| T-T-001 | Medium | Critical | High | High |
| T-T-002 | Medium | High | High | High |
| T-T-003 | Medium | Critical | High | High |
| T-T-004 | Low | Critical | Medium | High |
| T-T-005 | Low | Medium | Low | Low |
| T-R-001 | Medium | Medium | Medium | Low |
| T-R-002 | Medium | Medium | Medium | Medium |
| T-R-003 | Medium | Medium | Medium | Low |
| T-I-001 | Low | High | Medium | Medium |
| T-I-002 | Low | Critical | Medium | High |
| T-I-003 | Medium | Medium | Medium | Medium |
| T-I-004 | Medium | Medium | Medium | Medium |
| T-D-001 | Medium | High | High | High |
| T-D-002 | Medium | High | High | High |
| T-D-003 | Medium | Medium | Medium | Medium |
| T-D-004 | Medium | Medium | Medium | Medium |
| T-D-005 | Low | High | Medium | High |
| T-E-001 | Low | Critical | Medium | High |
| T-E-002 | Medium | High | High | High |
| T-E-003 | Low | Critical | Medium | High |
| T-E-004 | Low | Critical | Medium | High |

---

## 3. Security Goals

### 3.1 Security Goal Definitions

| SG ID | Security Goal | Description | CAL |
|-------|---------------|-------------|-----|
| SEC-SG-001 | Authentic Communication | Ensure all communication originates from authenticated sources | CAL 3 |
| SEC-SG-002 | Authorization Enforcement | Ensure all operations are properly authorized | CAL 3 |
| SEC-SG-003 | Communication Confidentiality | Protect communication content from disclosure | CAL 2 |
| SEC-SG-004 | Data Integrity | Protect data from unauthorized modification | CAL 3 |
| SEC-SG-005 | Availability | Ensure system availability under attack | CAL 2 |
| SEC-SG-006 | Audit Trail | Maintain complete audit trail of security events | CAL 2 |
| SEC-SG-007 | Secure Configuration | Ensure secure default configuration | CAL 2 |
| SEC-SG-008 | Secure Updates | Ensure secure software update process | CAL 3 |

---

## 4. Authentication Requirements

### CSR-AUTH-001: Mutual TLS Authentication

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-001 |
| Title | Mutual TLS Authentication |
| Description | All server-agent connections **shall** use mutual TLS authentication with X.509 certificates. |
| CAL | 3 |
| Threats Mitigated | T-S-001, T-S-002 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-AUTH-002: Certificate Key Requirements

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-002 |
| Title | Certificate Key Requirements |
| Description | Certificates **shall** use minimum RSA 2048-bit or ECDSA P-256 keys. RSA keys **should** be 4096-bit for new deployments. |
| CAL | 3 |
| Threats Mitigated | T-S-001, T-S-002, T-I-002 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-AUTH-003: Certificate Chain Validation

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-003 |
| Title | Certificate Chain Validation |
| Description | The system **shall** validate complete certificate chain to a trusted root CA. Self-signed certificates **shall** be rejected in production mode. |
| CAL | 3 |
| Threats Mitigated | T-S-001, T-S-002 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-AUTH-004: Certificate Expiration

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-004 |
| Title | Certificate Expiration |
| Description | The system **shall** reject expired certificates. The system **should** warn when certificates will expire within 30 days. |
| CAL | 2 |
| Threats Mitigated | T-S-001 |
| Safety Impact | Medium |
| Verification | Security Test |

### CSR-AUTH-005: Insecure Mode Warning

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-005 |
| Title | Insecure Mode Warning |
| Description | When running in insecure mode (--insecure), the system **shall** log warning at startup and every 5 minutes. |
| CAL | 2 |
| Threats Mitigated | T-S-001, T-I-001 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-AUTH-006: Agent Identity Verification

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-006 |
| Title | Agent Identity Verification |
| Description | The server **shall** verify agent identity matches the agent name provided in AgentHello message against certificate CN or SAN. |
| CAL | 3 |
| Threats Mitigated | T-S-001 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-AUTH-007: Workload Identity

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTH-007 |
| Title | Workload Identity |
| Description | Each workload **shall** be identified by a unique instance ID derived from workload name and execution ID. |
| CAL | 2 |
| Threats Mitigated | T-S-003 |
| Safety Impact | Medium |
| Verification | Unit Test |

---

## 5. Authorization Requirements

### CSR-AUTHZ-001: Default Deny Policy

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-001 |
| Title | Default Deny Policy |
| Description | All control interface requests **shall** be denied by default unless explicitly allowed by authorization rules. |
| CAL | 3 |
| Threats Mitigated | T-E-002 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-AUTHZ-002: Deny Rule Priority

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-002 |
| Title | Deny Rule Priority |
| Description | Deny rules **shall** be evaluated before allow rules. If any deny rule matches, the request **shall** be denied. |
| CAL | 3 |
| Threats Mitigated | T-E-002 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-AUTHZ-003: Filter Mask Validation

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-003 |
| Title | Filter Mask Validation |
| Description | Authorization filter masks **shall** be validated at configuration load. Invalid masks **shall** be rejected. |
| CAL | 2 |
| Threats Mitigated | T-E-002, T-T-001 |
| Safety Impact | Medium |
| Verification | Unit Test |

### CSR-AUTHZ-004: Operation Type Enforcement

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-004 |
| Title | Operation Type Enforcement |
| Description | Authorization **shall** distinguish between read (RW_READ), write (RW_WRITE), and read-write (RW_READ_WRITE) operations. |
| CAL | 3 |
| Threats Mitigated | T-T-001, T-E-002 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-AUTHZ-005: Log Access Authorization

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-005 |
| Title | Log Access Authorization |
| Description | Workload log access **shall** require explicit log rule authorization with workload name pattern. |
| CAL | 2 |
| Threats Mitigated | T-I-004 |
| Safety Impact | Low |
| Verification | Unit Test |

### CSR-AUTHZ-006: Server Authorization

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-006 |
| Title | Server Authorization |
| Description | Only authenticated server connections **shall** be authorized to send UpdateWorkload commands to agents. |
| CAL | 3 |
| Threats Mitigated | T-S-002, T-T-001 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-AUTHZ-007: CLI Authorization

| Attribute | Value |
|-----------|-------|
| ID | CSR-AUTHZ-007 |
| Title | CLI Authorization |
| Description | CLI connections **shall** be authenticated via mTLS. CLI certificate **should** have limited validity period. |
| CAL | 2 |
| Threats Mitigated | T-S-004 |
| Safety Impact | Medium |
| Verification | Security Test |

---

## 6. Communication Security Requirements

### CSR-COMM-001: TLS Version

| Attribute | Value |
|-----------|-------|
| ID | CSR-COMM-001 |
| Title | TLS Version |
| Description | The system **shall** support TLS 1.3. TLS 1.2 **may** be supported with approved cipher suites. TLS 1.1 and below **shall** be disabled. |
| CAL | 3 |
| Threats Mitigated | T-I-001 |
| Safety Impact | Medium |
| Verification | Security Test |

### CSR-COMM-002: Cipher Suite Restrictions

| Attribute | Value |
|-----------|-------|
| ID | CSR-COMM-002 |
| Title | Cipher Suite Restrictions |
| Description | Only AEAD cipher suites **shall** be permitted. CBC mode ciphers **shall** be disabled. |
| CAL | 3 |
| Threats Mitigated | T-I-001 |
| Safety Impact | Medium |
| Verification | Security Test |

**Approved Cipher Suites (TLS 1.3):**
- TLS_AES_256_GCM_SHA384
- TLS_AES_128_GCM_SHA256
- TLS_CHACHA20_POLY1305_SHA256

**Approved Cipher Suites (TLS 1.2):**
- TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
- TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
- TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
- TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256

### CSR-COMM-003: Forward Secrecy

| Attribute | Value |
|-----------|-------|
| ID | CSR-COMM-003 |
| Title | Forward Secrecy |
| Description | All cipher suites **shall** provide forward secrecy (ECDHE key exchange). |
| CAL | 2 |
| Threats Mitigated | T-I-001, T-I-002 |
| Safety Impact | Medium |
| Verification | Security Test |

### CSR-COMM-004: Control Interface Security

| Attribute | Value |
|-----------|-------|
| ID | CSR-COMM-004 |
| Title | Control Interface Security |
| Description | Control interface pipes **shall** be created with permissions 0600 (owner read/write only). |
| CAL | 2 |
| Threats Mitigated | T-S-003, T-I-003 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-COMM-005: Message Integrity

| Attribute | Value |
|-----------|-------|
| ID | CSR-COMM-005 |
| Title | Message Integrity |
| Description | All messages **shall** use protobuf serialization with implicit framing to ensure integrity. |
| CAL | 2 |
| Threats Mitigated | T-T-001 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-COMM-006: Replay Prevention

| Attribute | Value |
|-----------|-------|
| ID | CSR-COMM-006 |
| Title | Replay Prevention |
| Description | Request messages **shall** include unique request ID. Duplicate request IDs **should** be detected and logged. |
| CAL | 2 |
| Threats Mitigated | T-T-001 |
| Safety Impact | Medium |
| Verification | Unit Test |

---

## 7. Data Protection Requirements

### CSR-DATA-001: Key Storage Security

| Attribute | Value |
|-----------|-------|
| ID | CSR-DATA-001 |
| Title | Key Storage Security |
| Description | Private keys **shall** be stored with file permissions 0600. Private keys **shall not** be logged or transmitted. |
| CAL | 3 |
| Threats Mitigated | T-I-002 |
| Safety Impact | High |
| Verification | Review, Security Test |

### CSR-DATA-002: Configuration File Security

| Attribute | Value |
|-----------|-------|
| ID | CSR-DATA-002 |
| Title | Configuration File Security |
| Description | Configuration files containing secrets **shall** have permissions 0600 or 0640 with restricted group. |
| CAL | 2 |
| Threats Mitigated | T-I-003, T-T-002 |
| Safety Impact | Medium |
| Verification | Review |

### CSR-DATA-003: Secret Handling

| Attribute | Value |
|-----------|-------|
| ID | CSR-DATA-003 |
| Title | Secret Handling |
| Description | Secrets in configuration **shall** be marked as sensitive. Sensitive values **shall** be masked in logs. |
| CAL | 2 |
| Threats Mitigated | T-I-003 |
| Safety Impact | Medium |
| Verification | Review |

### CSR-DATA-004: State Data Protection

| Attribute | Value |
|-----------|-------|
| ID | CSR-DATA-004 |
| Title | State Data Protection |
| Description | State data **shall** be validated before processing. Invalid state data **shall** be rejected. |
| CAL | 3 |
| Threats Mitigated | T-T-001 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-DATA-005: Log Sanitization

| Attribute | Value |
|-----------|-------|
| ID | CSR-DATA-005 |
| Title | Log Sanitization |
| Description | Logs **shall not** contain: private keys, passwords, tokens, or full certificate contents. |
| CAL | 2 |
| Threats Mitigated | T-I-002, T-I-003 |
| Safety Impact | Medium |
| Verification | Review |

---

## 8. Input Validation Requirements

### CSR-INPUT-001: API Version Validation

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-001 |
| Title | API Version Validation |
| Description | The system **shall** validate API version on all manifest inputs. Unsupported versions **shall** be rejected. |
| CAL | 2 |
| Threats Mitigated | T-T-001, T-T-002 |
| Safety Impact | Medium |
| Verification | Unit Test |

### CSR-INPUT-002: Field Name Validation

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-002 |
| Title | Field Name Validation |
| Description | Field names **shall** be validated: max 63 chars, allowed characters [a-zA-Z0-9_-], no leading hyphen. |
| CAL | 2 |
| Threats Mitigated | T-T-001, T-T-003 |
| Safety Impact | Medium |
| Verification | Unit Test |

### CSR-INPUT-003: RuntimeConfig Validation

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-003 |
| Title | RuntimeConfig Validation |
| Description | RuntimeConfig **shall** be validated for: maximum length (1MB), no null bytes, valid UTF-8 encoding. |
| CAL | 3 |
| Threats Mitigated | T-T-003 |
| Safety Impact | High |
| Verification | Unit Test |

### CSR-INPUT-004: Template Injection Prevention

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-004 |
| Title | Template Injection Prevention |
| Description | Config template variables **shall** be escaped to prevent template injection. Direct execution of template content **shall** be prohibited. |
| CAL | 3 |
| Threats Mitigated | T-T-003, T-E-001 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-INPUT-005: Path Traversal Prevention

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-005 |
| Title | Path Traversal Prevention |
| Description | File paths in workload specs **shall** be validated to prevent path traversal (../ sequences). |
| CAL | 3 |
| Threats Mitigated | T-T-002, T-I-003 |
| Safety Impact | High |
| Verification | Security Test |

### CSR-INPUT-006: Message Size Limits

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-006 |
| Title | Message Size Limits |
| Description | gRPC messages **shall** have maximum size of 4MB. Messages exceeding limit **shall** be rejected. |
| CAL | 2 |
| Threats Mitigated | T-D-003 |
| Safety Impact | Medium |
| Verification | Unit Test |

### CSR-INPUT-007: Dependency Cycle Prevention

| Attribute | Value |
|-----------|-------|
| ID | CSR-INPUT-007 |
| Title | Dependency Cycle Prevention |
| Description | Workload dependency graphs **shall** be validated for cycles before acceptance. Cyclic dependencies **shall** be rejected. |
| CAL | 2 |
| Threats Mitigated | T-D-002 |
| Safety Impact | High |
| Verification | Unit Test |

---

## 9. Secure Development Requirements

### CSR-DEV-001: No Unsafe Rust in Safety Code

| Attribute | Value |
|-----------|-------|
| ID | CSR-DEV-001 |
| Title | No Unsafe Rust in Safety Code |
| Description | Safety-critical code paths **shall** not use unsafe Rust without documented justification and review. |
| CAL | 3 |
| Threats Mitigated | T-E-001, T-E-003, T-E-004 |
| Safety Impact | High |
| Verification | Code Review |

### CSR-DEV-002: Dependency Auditing

| Attribute | Value |
|-----------|-------|
| ID | CSR-DEV-002 |
| Title | Dependency Auditing |
| Description | All third-party dependencies **shall** be audited using cargo-audit. Critical vulnerabilities **shall** block release. |
| CAL | 2 |
| Threats Mitigated | T-T-004 |
| Safety Impact | High |
| Verification | CI Pipeline |

### CSR-DEV-003: Static Analysis

| Attribute | Value |
|-----------|-------|
| ID | CSR-DEV-003 |
| Title | Static Analysis |
| Description | Code **shall** pass clippy analysis with security-related lints enabled. |
| CAL | 2 |
| Threats Mitigated | Multiple |
| Safety Impact | Medium |
| Verification | CI Pipeline |

### CSR-DEV-004: Panic Prevention

| Attribute | Value |
|-----------|-------|
| ID | CSR-DEV-004 |
| Title | Panic Prevention |
| Description | Safety-critical code **shall** handle all errors explicitly. Use of unwrap() and expect() **shall** be justified. |
| CAL | 3 |
| Threats Mitigated | T-D-001, T-D-002 |
| Safety Impact | High |
| Verification | Code Review |

### CSR-DEV-005: Fuzzing

| Attribute | Value |
|-----------|-------|
| ID | CSR-DEV-005 |
| Title | Fuzzing |
| Description | Input parsing code **should** be fuzz tested. Critical parsers (protobuf, YAML) **shall** be fuzz tested. |
| CAL | 2 |
| Threats Mitigated | T-T-001, T-D-001 |
| Safety Impact | Medium |
| Verification | Test Report |

---

## 10. Monitoring and Incident Response

### CSR-MON-001: Security Event Logging

| Attribute | Value |
|-----------|-------|
| ID | CSR-MON-001 |
| Title | Security Event Logging |
| Description | The system **shall** log all security-relevant events: authentication, authorization decisions, configuration changes. |
| CAL | 2 |
| Threats Mitigated | T-R-001, T-R-002, T-R-003 |
| Safety Impact | Low |
| Verification | Review |

### CSR-MON-002: Authentication Failure Logging

| Attribute | Value |
|-----------|-------|
| ID | CSR-MON-002 |
| Title | Authentication Failure Logging |
| Description | Failed authentication attempts **shall** be logged with: timestamp, source address, failure reason. |
| CAL | 2 |
| Threats Mitigated | T-S-001, T-R-001 |
| Safety Impact | Medium |
| Verification | Unit Test |

### CSR-MON-003: Authorization Failure Logging

| Attribute | Value |
|-----------|-------|
| ID | CSR-MON-003 |
| Title | Authorization Failure Logging |
| Description | Denied authorization requests **shall** be logged with: requester, resource, operation, rule matched. |
| CAL | 2 |
| Threats Mitigated | T-E-002, T-R-001 |
| Safety Impact | Medium |
| Verification | Unit Test |

### CSR-MON-004: Rate Limiting

| Attribute | Value |
|-----------|-------|
| ID | CSR-MON-004 |
| Title | Rate Limiting |
| Description | The server **should** implement rate limiting: max 100 connections per second per source IP. |
| CAL | 2 |
| Threats Mitigated | T-D-001 |
| Safety Impact | High |
| Verification | Integration Test |

### CSR-MON-005: Connection Limit

| Attribute | Value |
|-----------|-------|
| ID | CSR-MON-005 |
| Title | Connection Limit |
| Description | The server **shall** limit concurrent connections to 1000. Excess connections **shall** be rejected. |
| CAL | 2 |
| Threats Mitigated | T-D-001 |
| Safety Impact | High |
| Verification | Integration Test |

### CSR-MON-006: Anomaly Detection

| Attribute | Value |
|-----------|-------|
| ID | CSR-MON-006 |
| Title | Anomaly Detection |
| Description | The system **should** detect and log anomalous patterns: rapid reconnection, unusual state changes, high error rates. |
| CAL | 1 |
| Threats Mitigated | Multiple |
| Safety Impact | Medium |
| Verification | Review |

---

## 11. Update and Patch Management

### CSR-UPDATE-001: Secure Update Channel

| Attribute | Value |
|-----------|-------|
| ID | CSR-UPDATE-001 |
| Title | Secure Update Channel |
| Description | Software updates **shall** be distributed via authenticated channels (signed packages, HTTPS). |
| CAL | 3 |
| Threats Mitigated | T-T-004 |
| Safety Impact | High |
| Verification | Review |

### CSR-UPDATE-002: Update Signature Verification

| Attribute | Value |
|-----------|-------|
| ID | CSR-UPDATE-002 |
| Title | Update Signature Verification |
| Description | Binary releases **shall** be signed. Signature verification **shall** be performed before installation. |
| CAL | 3 |
| Threats Mitigated | T-T-004 |
| Safety Impact | High |
| Verification | Review |

### CSR-UPDATE-003: Rollback Capability

| Attribute | Value |
|-----------|-------|
| ID | CSR-UPDATE-003 |
| Title | Rollback Capability |
| Description | The system **shall** support rollback to previous version. Rollback **shall** complete within 5 minutes. |
| CAL | 2 |
| Threats Mitigated | T-D-001 |
| Safety Impact | High |
| Verification | Integration Test |

### CSR-UPDATE-004: Vulnerability Disclosure

| Attribute | Value |
|-----------|-------|
| ID | CSR-UPDATE-004 |
| Title | Vulnerability Disclosure |
| Description | Security vulnerabilities **shall** be tracked and disclosed according to Eclipse Foundation policy. |
| CAL | 2 |
| Threats Mitigated | Multiple |
| Safety Impact | Medium |
| Verification | Process Review |

### CSR-UPDATE-005: Container Image Security

| Attribute | Value |
|-----------|-------|
| ID | CSR-UPDATE-005 |
| Title | Container Image Security |
| Description | Container images **should** be scanned for vulnerabilities before deployment. Critical vulnerabilities **should** block deployment. |
| CAL | 2 |
| Threats Mitigated | T-T-004 |
| Safety Impact | High |
| Verification | CI Pipeline |

---

## 12. Traceability

### 12.1 Threat to Requirement Traceability

| Threat ID | CSR IDs |
|-----------|---------|
| T-S-001 | CSR-AUTH-001, CSR-AUTH-002, CSR-AUTH-003, CSR-AUTH-004, CSR-AUTH-006, CSR-MON-002 |
| T-S-002 | CSR-AUTH-001, CSR-AUTH-002, CSR-AUTH-003, CSR-AUTHZ-006 |
| T-S-003 | CSR-AUTH-007, CSR-COMM-004 |
| T-S-004 | CSR-AUTHZ-007 |
| T-T-001 | CSR-AUTHZ-003, CSR-AUTHZ-004, CSR-AUTHZ-006, CSR-COMM-005, CSR-COMM-006, CSR-DATA-004, CSR-INPUT-001, CSR-INPUT-002, CSR-DEV-005 |
| T-T-002 | CSR-DATA-002, CSR-INPUT-001, CSR-INPUT-002, CSR-INPUT-005 |
| T-T-003 | CSR-INPUT-002, CSR-INPUT-003, CSR-INPUT-004 |
| T-T-004 | CSR-DEV-002, CSR-UPDATE-001, CSR-UPDATE-002, CSR-UPDATE-005 |
| T-T-005 | N/A (logging integrity out of scope) |
| T-R-001 | CSR-MON-001, CSR-MON-002, CSR-MON-003 |
| T-R-002 | CSR-MON-001 |
| T-R-003 | CSR-MON-001, CSR-MON-003 |
| T-I-001 | CSR-AUTH-005, CSR-COMM-001, CSR-COMM-002, CSR-COMM-003 |
| T-I-002 | CSR-AUTH-002, CSR-COMM-003, CSR-DATA-001, CSR-DATA-005 |
| T-I-003 | CSR-COMM-004, CSR-DATA-002, CSR-DATA-003, CSR-DATA-005, CSR-INPUT-005 |
| T-I-004 | CSR-AUTHZ-005 |
| T-D-001 | CSR-MON-004, CSR-MON-005, CSR-DEV-004, CSR-DEV-005, CSR-UPDATE-003 |
| T-D-002 | CSR-DEV-004, CSR-INPUT-007 |
| T-D-003 | CSR-INPUT-006 |
| T-D-004 | N/A (implementation detail) |
| T-D-005 | N/A (runtime limitation) |
| T-E-001 | CSR-INPUT-004, CSR-DEV-001 |
| T-E-002 | CSR-AUTHZ-001, CSR-AUTHZ-002, CSR-AUTHZ-004, CSR-MON-003 |
| T-E-003 | CSR-DEV-001 |
| T-E-004 | CSR-DEV-001 |

### 12.2 Security Goal to Requirement Traceability

| Security Goal | CSR IDs |
|---------------|---------|
| SEC-SG-001 | CSR-AUTH-001 through CSR-AUTH-007 |
| SEC-SG-002 | CSR-AUTHZ-001 through CSR-AUTHZ-007 |
| SEC-SG-003 | CSR-COMM-001 through CSR-COMM-006 |
| SEC-SG-004 | CSR-DATA-001 through CSR-DATA-005, CSR-INPUT-001 through CSR-INPUT-007 |
| SEC-SG-005 | CSR-MON-004, CSR-MON-005, CSR-DEV-004 |
| SEC-SG-006 | CSR-MON-001 through CSR-MON-006 |
| SEC-SG-007 | CSR-DATA-002, CSR-AUTH-005 |
| SEC-SG-008 | CSR-UPDATE-001 through CSR-UPDATE-005 |

### 12.3 Requirement Summary

| Category | Count |
|----------|-------|
| Authentication (AUTH) | 7 |
| Authorization (AUTHZ) | 7 |
| Communication (COMM) | 6 |
| Data Protection (DATA) | 5 |
| Input Validation (INPUT) | 7 |
| Development (DEV) | 5 |
| Monitoring (MON) | 6 |
| Update (UPDATE) | 5 |
| **Total** | **48** |

---

## 13. References

### 13.1 Standards

| Standard | Title |
|----------|-------|
| ISO/SAE 21434:2021 | Road vehicles - Cybersecurity engineering |
| UN R155 | Cybersecurity and cybersecurity management system |
| UN R156 | Software update and software update management system |
| NIST SP 800-53 | Security and Privacy Controls |
| OWASP ASVS | Application Security Verification Standard |

### 13.2 Related Documents

| Document ID | Title |
|-------------|-------|
| ANKAIOS-ID-001 | Item Definition |
| ANKAIOS-HARA-001 | Hazard Analysis and Risk Assessment |
| ANKAIOS-FSR-001 | Functional Safety Requirements |
| ANKAIOS-TSR-001 | Technical Safety Requirements |

---

## Appendix A: Cybersecurity Assurance Levels

| CAL | Description | Rigor |
|-----|-------------|-------|
| CAL 1 | Minimal cybersecurity assurance | Basic controls |
| CAL 2 | Moderate cybersecurity assurance | Standard controls |
| CAL 3 | High cybersecurity assurance | Rigorous controls |
| CAL 4 | Maximum cybersecurity assurance | Comprehensive controls |

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| CAL | Cybersecurity Assurance Level |
| mTLS | Mutual Transport Layer Security |
| STRIDE | Spoofing, Tampering, Repudiation, Information Disclosure, Denial of Service, Elevation of Privilege |
| AEAD | Authenticated Encryption with Associated Data |
| CN | Common Name (certificate field) |
| SAN | Subject Alternative Name (certificate field) |

---

## Appendix C: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-08-15 | Security Team | Initial release |

---

*Document approved for ISO/SAE 21434 compliance activities.*
