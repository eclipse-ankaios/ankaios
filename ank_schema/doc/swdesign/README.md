# Ankaios Schema - SW Design

## About this document

This document describes the Software Design for the Ankaios Schema crate.

Ankaios is a workload orchestrator supporting a subset of the Kubernetes configurations and is targeted at the automotive use case.

The Ankaios Schema crate provides the JSON Schema definition for the Ankaios manifest format as well as functionality for creating and validating the schema.

## Context View

The Ankaios Schema crate is used by Ankaios components that need to validate manifests (e.g. the CLI and/or the server).

## Constraints, risks and decisions

### Design decisions

## Structural view

### Schema provider

The schema provider generates the JSON Schema in Draft 7 format from the Ankaios API types.

### Manifest validator

The manifest validator exposes a public function to validate a given manifest against the Ankaios JSON Schema.

## Behavioral view

This chapter defines the runtime behavior of the Ankaios Schema crate. The following chapters describe the requirements towards the crate.

### Schema generation

#### Ankaios Schema crate provides the Ankaios schema in JSON Schema Draft 7 format
`swdd~ank-schema-provides-schema~1`

Status: approved

The Ankaios Schema crate shall provide the Ankaios manifest schema as a JSON Schema in Draft 7 format, derived from the `StateSpec` type of the Ankaios API.

Rationale:
JSON Schema Draft 7 is widely supported and provides sufficient expressiveness for describing the Ankaios manifest structure including field patterns, required fields, and enum values.

Tags:
- SchemaProvider

Needs:
- impl

### Manifest validation

#### Ankaios Schema crate provides a manifest validation method
`swdd~ank-schema-provides-manifest-validation~1`

Status: approved

The Ankaios Schema crate shall provide a public function that:
* validates a given manifest represented as a `serde_json::Value` against the Ankaios JSON Schema
* returns a descriptive error listing all violations when the validation fails.

Rationale:
Centralizing the validation logic in a dedicated crate avoids code duplication and ensures all Ankaios components apply the same validation rules.

Tags:
- ManifestValidator

Needs:
- impl
- utest

#### Ankaios Schema crate scopes validation errors to the relevant schema branch
`swdd~ank-schema-scopes-validation-errors~1`

Status: approved

When validating a manifest, the manifest validator shall report only the validation errors of the schema branch selected by the actual data by performing the following actions:

1. rewrite each internally tagged enum `oneOf` into a discriminator check plus one conditional branch per variant, so that only the variant matching the discriminator value contributes errors
2. rewrite each nullable `anyOf`/`oneOf` wrapper into a conditional that accepts `null` without emitting a "not of type null" error when the concrete branch fails
3. collect the resulting errors using the validator's per-error iteration.

Comment:
The per-error iteration is used instead of the structured evaluation output because the latter does not reliably report `required` violations when the same schema object also declares `properties`, which would otherwise hide the actual cause of a validation failure.

Rationale:
Internally tagged enums and optional fields generate `oneOf`/`anyOf` constructs whose default error reporting lists violations of every branch, producing misleading messages that reference unrelated variants (e.g. a `LogRule` error for a mistyped `StateRule` field).

Tags:
- ManifestValidator

Needs:
- impl
- utest

## Data view

## Error management view

## Physical view

## References

## Glossary

<!-- markdownlint-disable-file MD004 MD022 MD032 -->
