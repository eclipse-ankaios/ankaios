# Eclipse Ankaios Safety Documentation

This directory contains ISO 26262 and ISO/SAE 21434 safety and security documentation for the Eclipse Ankaios workload orchestrator.

## Document Overview

| Document | ID | Description |
|----------|-----|-------------|
| [01_Item_Definition.md](01_Item_Definition.md) | ANKAIOS-ID-001 | Defines Ankaios as a safety-related item including boundaries, interfaces, and operating conditions |
| [02_HARA_Hazard_Analysis.md](02_HARA_Hazard_Analysis.md) | ANKAIOS-HARA-001 | Hazard Analysis and Risk Assessment identifying 15 hazardous events and 10 safety goals |
| [03_FMEA_Analysis.md](03_FMEA_Analysis.md) | ANKAIOS-FMEA-001 | Failure Mode and Effects Analysis covering 106 failure modes across all components |
| [04_Functional_Safety_Requirements.md](04_Functional_Safety_Requirements.md) | ANKAIOS-FSR-001 | 42 Functional Safety Requirements derived from safety goals |
| [05_Technical_Safety_Requirements.md](05_Technical_Safety_Requirements.md) | ANKAIOS-TSR-001 | 49 Technical Safety Requirements for implementation |
| [06_Cybersecurity_Requirements.md](06_Cybersecurity_Requirements.md) | ANKAIOS-CSR-001 | 48 Cybersecurity Requirements per ISO/SAE 21434 |

## Document Relationships

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       SAFETY DOCUMENTATION HIERARCHY                     │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                        01_Item_Definition                            ││
│  │                     System scope and boundaries                      ││
│  └───────────────────────────────┬─────────────────────────────────────┘│
│                                  │                                       │
│                                  ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                    02_HARA_Hazard_Analysis                           ││
│  │            Hazards → Risk Assessment → Safety Goals                  ││
│  └───────────────────────────────┬─────────────────────────────────────┘│
│                                  │                                       │
│            ┌─────────────────────┴─────────────────────┐                │
│            │                                           │                │
│            ▼                                           ▼                │
│  ┌─────────────────────────┐              ┌─────────────────────────┐  │
│  │   03_FMEA_Analysis      │              │ 04_FSR_Requirements     │  │
│  │  Component failure      │              │ Functional safety       │  │
│  │  modes and effects      │              │ requirements            │  │
│  └─────────────────────────┘              └───────────┬─────────────┘  │
│                                                       │                 │
│                                                       ▼                 │
│                                           ┌─────────────────────────┐  │
│                                           │ 05_TSR_Requirements     │  │
│                                           │ Technical safety        │  │
│                                           │ requirements            │  │
│                                           └─────────────────────────┘  │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                   06_Cybersecurity_Requirements                      ││
│  │              Security threats → Security requirements                ││
│  │                    (Parallel to safety analysis)                     ││
│  └─────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────┘
```

## Key Statistics

### Safety Analysis
- **Hazardous Events**: 15 identified
- **Safety Goals**: 10 defined (5 ASIL D, 2 ASIL C, 2 ASIL B, 1 ASIL A)
- **FMEA Failure Modes**: 106 analyzed
- **Critical RPN (>100)**: 18 failure modes requiring immediate action

### Requirements
- **Functional Safety Requirements**: 42 (24 ASIL D, 8 ASIL C, 4 ASIL B, 6 ASIL A)
- **Technical Safety Requirements**: 49
- **Cybersecurity Requirements**: 48 (14 CAL 3, 28 CAL 2, 6 CAL 1)

### ASIL Distribution
| ASIL | Hazards | FSRs | TSRs |
|------|---------|------|------|
| D | 3 | 24 | 30 |
| C | 5 | 8 | 8 |
| B | 4 | 4 | 5 |
| A | 3 | 6 | 6 |

## Standards Compliance

### ISO 26262:2018 (Functional Safety)
- Part 2: Management of functional safety
- Part 3: Concept phase (Item Definition, HARA)
- Part 4: Product development: system level (TSC, FMEA)
- Part 6: Product development: software level (FSR, TSR)
- Part 9: ASIL-oriented and safety-oriented analyses (FMEA, FTA)

### ISO/SAE 21434:2021 (Cybersecurity)
- Threat analysis and risk assessment (TARA)
- Cybersecurity goals and requirements
- Cybersecurity assurance levels (CAL)

### UN Regulations
- UN R155: Cybersecurity
- UN R156: Software update management

## Usage Notes

1. **Document IDs**: Each document has a unique ID (e.g., ANKAIOS-HARA-001) for traceability
2. **Version Control**: All documents are version 1.0 (Initial Draft)
3. **Status**: All documents require review and approval
4. **Traceability**: Requirements include forward and backward traceability

## Next Steps

1. **Review and Approval**: Submit documents for safety review
2. **TÜV Consultation**: Engage assessment body for preliminary review
3. **Gap Analysis**: Compare current implementation against requirements
4. **Implementation**: Develop safety mechanisms per TSRs
5. **Verification**: Create test cases for each requirement

## Related Documentation

See also:
- [ISO26262_ASIL_D_Certification_Guide.md](../ISO26262_ASIL_D_Certification_Guide.md) - Comprehensive certification guide

## Document Version History

| Date | Version | Changes |
|------|---------|---------|
| 2026-08-15 | 1.0 | Initial documentation set |

---

*This documentation set supports ISO 26262 and ISO/SAE 21434 compliance activities for Eclipse Ankaios.*
