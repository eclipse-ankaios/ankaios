---
name: unit-test-development
description: "Write, improve, and review Ankaios Rust unit tests. Use when asked to add tests, write utests, improve test coverage, review unit tests, assess test quality, fix flaky tests, or design mocks, shims, fixtures, and test isolation."
---

# Unit Test Development

Use this skill to write or review focused, traceable, deterministic Rust unit tests in Ankaios.

Unit tests are encouraged for nontrivial behavior and optional for examples. Apply the same test-quality guidance when writing them.

## Establish the behavior under test

1. Read the production function and its nearest callers before reading broad surrounding code.
2. Read neighboring tests and reuse their fixtures, mocks, and naming conventions.
3. Identify observable behavior:
   - returned value or error
   - state transition
   - emitted message
   - required external interaction or ordering
4. Separate unit-testable behavior from behavior already covered more appropriately by integration or system tests.

Use the [requirement-tracing skill](../requirement-tracing/SKILL.md) for requirement lookup, test anchors, coverage semantics, and tracing validation. Example-only tests are exempt from requirement tracing.

## Write tests

### Structure and naming

- Put tests in a `#[cfg(test)] mod tests` near the bottom of the source module unless the crate has a more specific established pattern.
- Follow the neighboring test-module layout, including the standard test banner.
- Prefix unit-test names with `utest_` and describe behavior and condition, for example:

  ```rust
  fn utest_prepare_socket_path_replaces_stale_socket_file()
  ```

- Follow Arrange, Act, Assert with whitespace rather than comments.
- Keep one behavioral reason to fail per test. Multiple assertions are appropriate when they verify one outcome.
- Prefer small setup helpers when they remove repetition without hiding behavior relevant to the test.

### Assertions

- Assert the strongest stable observable result.
- Prefer exact values or error variants over only `is_ok()` or `is_err()` when the type permits it.
- For text errors, check the meaningful stable context and avoid matching incidental platform-specific wording.
- Verify that a failure short-circuits later operations when ordering matters.
- Assert mock call counts or order only when those interactions are part of the behavior or protect against side effects.
- Avoid reproducing the production algorithm in the expected value.

### Coverage selection

Cover the smallest useful set of equivalence classes:

- normal success
- boundary or absent input
- each materially different error source
- required short-circuit behavior
- entry-point wiring when helper tests alone would miss binding, conversion, or error mapping

Avoid redundant branch tests unless they protect a distinct failure mode.

## Choose test doubles

Use real values or temporary resources when practical. Otherwise, reuse an existing suitable mock, fake, or stub before creating a new double.

When a new double is needed, prefer a small fake or stub; reserve local shims for otherwise inaccessible OS or static APIs.

Prefer `tempfile` for filesystem tests where real filesystem semantics matter. Use Mockall expectations such as `.once()` and argument predicates when calls are behavioral requirements.

Keep a local shim narrow:

- mirror only the production API used by the module
- consume one-shot expectations so unexpected repeated calls fail
- fail clearly when an expectation is missing
- verify required expectations were consumed
- record call order only where order matters
- keep test-double implementation details after the behavior-focused tests

## Guarantee isolation

- Assume tests run concurrently and in an arbitrary order.
- Do not depend on state left by another test.
- Use unique temporary paths and RAII cleanup for real resources.
- For global Mockall contexts, follow the repository's `MockAllContextSync` pattern: `get_lock()` for synchronous tests and `get_lock_async().await` for asynchronous tests. These methods use independent mutexes; tests sharing a global mock must use the same synchronization mechanism, including mixed synchronous/asynchronous tests under `cargo test`.
- Prefer independent per-test state. Poison recovery alone does not restore shared mock state.

Thread-local shim state is suitable only when setup, execution, and verification remain on the same thread. It provides isolation between test threads, and nextest normally provides additional process isolation. Do not use thread-local expectations when:

- the code under test spawns a thread
- an async runtime may move execution between worker threads
- callbacks can run on another thread

For those cases, use explicitly owned state passed into the double, or synchronized shared state scoped by a per-test identifier.

Resetting thread-local state at the start of each synchronous test protects against worker-thread reuse. Prefer an RAII context when cleanup after a panic is also required.

## Handle asynchronous tests

- Use `#[tokio::test]` for async behavior.
- Avoid sleeps as synchronization. Use channels, notifications, timeouts, or observable state.
- Bound waits with `tokio::time::timeout` so failures terminate predictably.
- Do not hold a synchronous mutex guard across `.await`.
- Verify both expected messages and the absence of unintended messages where meaningful.

## Review tests

Review for defects and regression risk before style. Report findings first, ordered by severity, with file and line references.

Apply the guidance above, focusing on:

1. **Behavioral relevance**: Would a plausible production regression make this test fail?
2. **Coverage**: Are required entry points, error paths, and distinct failure modes tested at the appropriate layer?
3. **Assertion strength**: Could the wrong result satisfy the assertions?
4. **Isolation and doubles**: Can shared state leak, or can a double hide a regression?
5. **Failure quality**: Does the failure identify the defect without cascading?
6. **Maintainability**: Does setup or abstraction obscure the tested behavior?

Do not call a test weak solely because it does not assert implementation details. Do not recommend unit coverage when an existing integration test is the correct layer; state the remaining risk precisely.

If no findings remain, say so explicitly and mention residual risks such as untested platform behavior or integration boundaries.

## Validate changes

After a coherent change, run the narrowest relevant test and check formatting:

```shell
cargo test -p <package> <test-name-or-module-filter>
rustfmt --edition 2024 --check <touched-file.rs>
```

Broaden to `cargo test -p <package>` or `just utest` when the change or results warrant it. `just utest` uses the repository nextest configuration; tests must also remain isolated under `cargo test`.

For review-only requests, run tests when needed to verify a finding. Report checks run, test pass/fail counts when available, and relevant checks not run.
