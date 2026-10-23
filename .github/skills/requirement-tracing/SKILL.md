---
name: requirement-tracing
description: Manage requirement tracing — write new or update existing requirements/design decisions, link implementations and tests to existing ones, and maintain tracing consistency. Applies automatically when implementing product features or fixing product bugs; examples are exempt.
---

# Requirement Tracing

Ankaios uses OpenFastTrace (OFT) to trace product implementations and tests directly to software requirements/design decisions (`swdd`):

```text
impl  --\
utest ---\
itest ----> swdd
stest ---/
```

`Needs` selects the required coverage types; not every requirement needs every test layer. Code under `examples/` is exempt from requirement tracing.

## Workflow

1. Search the relevant SW design document and existing anchors for the behavior being changed.
2. Reuse a requirement when it already describes the intended behavior, including bug fixes that restore that behavior. Update it when its specified behavior changes; add one when no existing requirement covers a new behavior, constraint, or validation rule.
3. Record a design decision for a non-obvious architectural choice with significant alternatives or long-term trade-offs.
4. Trace the implementing blocks and relevant unit, integration, and system tests using the anchor rules below.
5. Review semantic coverage, then validate the tracing report and comparison against main.

## Requirement locations

Requirements and design decisions live in the SW design document of the crate they belong to:

| Crate       | SW design document                                         |
| ----------- | ---------------------------------------------------------- |
| server      | [server](../../../server/doc/swdesign/README.md)           |
| agent       | [agent](../../../agent/doc/swdesign/README.md)             |
| common      | [common](../../../common/doc/swdesign/README.md)           |
| ank (CLI)   | [ank](../../../ank/doc/swdesign/README.md)                 |
| grpc        | [grpc](../../../grpc/doc/swdesign/README.md)               |
| ank_schema  | [ank_schema](../../../ank_schema/doc/swdesign/README.md)   |
| ankaios_api | [ankaios_api](../../../ankaios_api/doc/swdesign/README.md) |

## Requirement format

Use the project's [requirement template](../../../doc/docs/development/requirement-template.md):

```markdown
#### Human-readable title
`swdd~<crate-prefix>-<descriptive-name>~<version>`

Status: approved

[When <condition>,] <object> shall <action>.

Rationale:
<Why this requirement exists; omit if it adds no information.>

Tags:
- <UnitName>

Needs:
- impl
- utest
```

### Wording and scope

- Use "shall" and one subject per requirement.
- Split independent behaviors into separate requirements. For an ordered sequence by one subject, use one "shall" statement followed by a numbered list.
- List configurable attributes with their required/optional status and defaults.
- Add `Comment:` for useful context and `Rationale:` unless the reason is obvious and adds no information.
- Use terms consistently across requirements; do not give an established term a different meaning.

### ID naming conventions

- Follow the component's existing prefix: `server-`, `agent-`, `cli-`, `grpc-`, `common-`, `ank-schema-`, or `api-`.
- Use lowercase kebab-case for the descriptive part.
- Start at version `1`; follow the versioning rules below for updates.

### Needs field

Specify which artifact types must cover this requirement:

| Artifact | When to include                                                        |
|----------|------------------------------------------------------------------------|
| `impl`   | Always (every requirement needs implementation)                        |
| `utest`  | When unit-testable logic exists                                        |
| `itest`  | When integration between components is tested using mocked components  |
| `stest`  | When end-to-end system behavior must be validated                      |

### Tags field

Use the implementing unit names from the document's "Structural view", following its naming conventions. Add new units there when the implementation introduces them.

## Design decisions

Use the same format, adding assumptions and considered alternatives. Follow the surrounding document's field order:

```markdown
Assumptions:
<What was assumed to be true.>

Considered alternatives:
- **Alternative A**: <description and why rejected>
- **Alternative B**: <description and why rejected>
```

## Anchor rules

Place implementation anchors directly above the implementing block. For a whole Rust file, place them after the `use` statements:

```rust
// [impl->swdd~server-loads-startup-state-file~3]
fn load_startup_state(path: &Path) -> Result<State> {
    // ...
}
```

Place unit and integration test anchors directly above the test:

```rust
// [utest->swdd~server-loads-startup-state-file~3]
#[test]
fn utest_loads_startup_state() {
    // Test body omitted.
}

// [itest->swdd~server-loads-startup-state-file~3]
#[test]
fn itest_loads_startup_state() {
    // Test body omitted.
}
```

In Robot Framework, place the system test anchor immediately above or inside the test case:

```robot
*** Test Cases ***
# [stest->swdd~server-loads-startup-state-file~3]
Server loads startup state from file
    [Documentation]    Verify server loads state from YAML file
    ...
```

- Use one anchor per requirement per logical block, not per line. Use separate anchors when a block covers multiple requirements.
- Each traced block or test must genuinely contribute. Individual artifacts may cover part of a requirement; collectively, they must satisfy its complete behavior and declared `Needs`.
- For test design and assertion quality, use the [unit-test-development skill](../unit-test-development/SKILL.md).
- Optional insertion helpers are available in the [VS Code tracing snippets](../../../.vscode/tracing.code-snippets).

## Versioning

For a material change to a requirement already on `main`, increment its version once relative to that baseline, e.g. `swdd~name~1` to `swdd~name~2`. New requirements and further edits to an unpublished revision keep their version. The baseline is main, not a release.

Update every anchor targeting the changed requirement (`impl`, `utest`, `itest`, and `stest`) to the new version, and ensure its implementation and tests satisfy the revised behavior.

## Semantic review

Before accepting or tracing a requirement, check:

- The heading and ID summarize the complete normative behavior, not just one supported option.
- When extending a mechanism, existing options remain documented, either in one general requirement or in separate requirements for independent behaviors.
- Traced implementations and tests cover every specified option and match `Needs`. Add anchors to existing tests when established behavior becomes part of a broader requirement.
- The rationale explains the whole requirement, not just one option.

OFT checks links and coverage types, not semantic agreement between headings, IDs, bodies, rationales, code, and tests. Perform this review even when tracing reports no issues.

## Validation

1. Run `just trace-requirements` and inspect `build/req/req_tracing_report.html` for unresolved links and missing coverage.
2. Run `just compare-requirements` and inspect its output for newly uncovered items relative to main. This fetches `origin/main` but uses the local `main` branch for the baseline; ensure it represents the intended baseline.
3. Resolve tracing issues caused by the change and report any remaining gaps.

Neither command's exit status alone proves complete coverage: the recipes suppress OFT failures, and the comparison script prints findings without failing. The comparison only reports new uncovered items or items that changed from covered to uncovered; it is not a full tracing check.

See the [project tracing guide](../../../doc/docs/development/requirement-tracing.md) or `oft help` for direct OFT usage.
