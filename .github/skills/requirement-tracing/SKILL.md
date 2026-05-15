---
name: requirement-tracing
description: Manage requirement tracing — write new or update existing requirements/design decisions, link implementations and tests to existing ones, and maintain tracing consistency. Applies automatically when implementing features or fixing bugs.
---

# Requirement Tracing Skill

This skill describes how to write, link, and maintain requirements and design decisions in the Ankaios project.

## Overview

Ankaios uses OpenFastTrace for bidirectional requirement tracing. Every feature must be traceable from its software design decision (swdd) through implementation and tests.

The tracing chain is:

```text
swdd (requirement/design decision)
  ← impl (source code)
  ← utest (unit test)
  ← itest (integration test)
  ← stest (system test)
```

## Requirement locations

Requirements and design decisions live in the SW design document of the crate they belong to:

| Crate       | File                                |
|-------------|-------------------------------------|
| server      | `server/doc/swdesign/README.md`     |
| agent       | `agent/doc/swdesign/README.md`      |
| common      | `common/doc/swdesign/README.md`     |
| ank (CLI)   | `ank/doc/swdesign/README.md`        |
| grpc        | `grpc/doc/swdesign/README.md`       |
| ank_schema  | `ank_schema/doc/swdesign/README.md` |
| ankaios_api | `ankaios_api/doc/swdesign/README.md`|

## Requirement format

Each requirement is a Markdown section with this structure:

```markdown
#### Human-readable title
`swdd~<crate-prefix>-<descriptive-name>~<version>`

Status: approved

[When <condition separated by and>,] <object> shall <do something>.

Comment:
<Optional additional context.>

Rationale:
<Optional explanation of why this requirement exists.>

Tags:
- <UnitName>

Needs:
- impl
- utest
```

### Wording rules

- Requirements **shall** use "shall" — never "should", "will", "is", etc.
- Only **one object** (subject) is allowed per requirement.
- Use the pattern: `[When <condition>,] <object> shall <action>.`
- When a requirement describes a sequence of actions by the same object, use one "shall" statement followed by an ordered list:

  ```markdown
  When X happens, the server shall do Y by performing the following actions in order:

  1. First action
  2. Second action
  3. Third action
  ```

- When a requirement describes multiple independent behaviors, split into separate requirements.
- Attributes of a configurable entity can be listed:

  ```markdown
  The server shall support configuring X with the following attributes:

  - `name` — description (required)
  - `prio` — description (optional, default `0`)
  ```
- Comment and Rationale sections are optional; omit them only when the reason is obvious and the rationale would not add new information. In all other cases, include a Rationale.
- Avoid term shadowing: do not reuse a term in a different requirement with a different meaning. For example, if "veto" means rejecting an entire state update, do not use "vetoed workloads" to describe individual workloads dropped from a list.

### ID naming conventions

- Prefix with the crate/component: `server-`, `agent-`, `cli-`, `grpc-`, `common-`, `ank-schema-`
- Use lowercase kebab-case for the descriptive part
- Version starts at `1` and increments when the requirement text changes materially

### Needs field

Specify which artifact types must cover this requirement:

| Artifact | When to include                                                        |
|----------|------------------------------------------------------------------------|
| `impl`   | Always (every requirement needs implementation)                        |
| `utest`  | When unit-testable logic exists                                        |
| `itest`  | When integration between components is tested using mocked components  |
| `stest`  | When end-to-end system behavior must be validated                      |

### Tags field

Tags reference the structural unit(s) (from the "Structural view" section of the swdesign doc) that implement this requirement. Use PascalCase unit names. Add tags to the Structural view in case new units are added with the implementation.

## Design decisions

Design decisions use the same format but add the following fields between Rationale or Comment and Needs:

```markdown

Assumptions:
<What was assumed to be true.>

Considered alternatives:
- **Alternative A**: <description and why rejected>
- **Alternative B**: <description and why rejected>

```

## Linking implementation to requirements

In source code, add a tracing comment **on the line(s) that implement** the requirement:

```rust
// [impl->swdd~server-loads-startup-state-file~3]
fn load_startup_state(path: &Path) -> Result<State> {
    // ...
}
```

Rules:

- Place the comment directly above the line(s) that satisfy the requirement
- If the whole code in a `.rs` file implements the requirement, place the tracing anchor after the `use` statements
- One comment per requirement per logical block (don't repeat on every line)
- Multiple requirements can be traced from the same code block using separate comments:

  ```rust
  // [impl->swdd~server-validates-desired-state-api-version~1]
  // [impl->swdd~server-fails-on-invalid-startup-state~1]
  ```

## Linking unit tests to requirements

In test functions, add a tracing comment directly above the test:

```rust
// [utest->swdd~server-loads-startup-state-file~3]
#[test]
fn test_loads_startup_state() {

    let result = load_startup_state(&path);
    assert!(result.is_ok());
}
```

If a test covers multiple requirements, add multiple comments.

## Linking system tests to requirements

In Robot Framework `.robot` files, add a tracing comment above or inside the test case:

```robot
# [stest->swdd~server-loads-startup-state-file~3]
*** Test Cases ***
Server loads startup state from file
    [Documentation]    Verify server loads state from YAML file
    ...
```

## When to write new requirements

Write a **new requirement** when:

- Implementing new user-visible behavior or a new feature
- Adding a new constraint or validation rule
- The behavior is not already described by an existing requirement

Write a **new design decision** when:

- Making a non-obvious architectural choice
- Choosing between multiple viable alternatives
- The decision has long-term implications or trade-offs

## When to link to existing requirements

Link to an **existing requirement without changes** when:

- The implementation satisfies the requirement as written
- Adding new code that covers an already-documented behavior

**Update an existing requirement** (bump version) when:

- The implementation changes the behavior described by the requirement
- The requirement text no longer accurately describes what the code does
- When bumping version: update the version in the ID AND update all existing `[impl->...]`, `[utest->...]`, `[stest->...]` references to the new version

## Version bumping

When updating a requirement:

1. Increment the version number in the ID: `swdd~name~1` → `swdd~name~2`
2. Update all tracing comments in code/tests that reference the old version

**Important:** Only bump the version when the requirement was already published on the `main` branch. If you are working on a feature branch and the requirement was introduced or last changed on that same branch (i.e., it has not been merged to `main` yet), update the requirement text in place **without** incrementing the version number. Version bumps are reserved for changes to requirements that are already part of the released baseline.
3. Update the requirement text to match the new behavior
4. Make sure that the traced implementation and tests are updated to satisfy the new version of the requirement

## Validation

Ankaios uses [OpenFastTrace (OFT)](https://github.com/itsallcode/openfasttrace) to verify that every requirement is covered by the required artifact types (`impl`, `utest`, etc.). OFT scans the `src/`, `doc/` and `tests/` directories, parses requirement IDs from Markdown and tracing comments from source code, and checks coverage.

### Quick check via just

```bash
just trace-requirements
```

This runs `oft trace` and generates an HTML report at `build/req/req_tracing_report.html`.

### Running OFT directly

```bash
oft trace <directories...> -a <artifact-types> [options]
```

Key options:

- `-a swdd,impl,utest,itest,stest` — artifact types to include (always use this set)
- `-v all` — show all requirements, not just problems (default only shows uncovered ones)
- `-o html -f report.html` — generate an HTML report instead of plain text

Example showing all requirements:

```bash
oft trace $(find . -type d \( -name 'src' -o -name 'doc' -o -name 'tests' \) -not -path './doc') -a swdd,impl,utest,itest,stest -v all
```

### Comparing against main

```bash
just compare-requirements
```

This compares the tracing report of the current branch against main and prints newly uncovered requirements.

## VS Code snippets

Use the existing code snippets for quick insertion:

- Type `impl` → expands to `// [impl->$1]`
- Type `utest` → expands to `// [utest->$1]`
- Type `req` → expands to a full requirement template

## Workflow summary

When implementing a feature:

1. **Check** if a matching requirement already exists in the relevant `doc/swdesign/README.md`
2. **If yes**: add `// [impl->swdd~requirement-name~version]` tracing comments to implementation code
3. **If no**: write a new requirement in the appropriate swdesign document, then trace it
4. **For tests**: add `// [utest->swdd~...]` or `# [stest->swdd~...]` comments
5. **Verify** with `just trace-requirements` that coverage is complete
