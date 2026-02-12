# Unit verification

This page defines which tools and processes are used in in this project for the purposes of software unit verification. The unit verification process is performed during implementation phase and is as automated as possible, one exception is the code review which cannot be done automatically. Automated unit test runs are executed by the [CI build system](https://github.com/eclipse-ankaios/ankaios/actions) as well as during the regular releasing process.

## Verification tools and procedures

Ankaios development follows the guidelines specified in the [Rust coding guidelines](rust-coding-guidelines.md).

### Code review

Code reviews are part of the implementation process and performed before code is merged to the main branch. Contributors create pull requests and request a review s.t. the process can be started. The review is performed by at least one committer who has good knowledge of the area under review.
When all applicable review criteria and checklists are passed and reviewer(s) have accepted the change, code can be merged to the main branch.

### Verification by unit test

#### Test focus and goal

The objective of the unit test is to confirm the correct internal behavior of a software unit according to the design aspects documented in the SW design.
A unit test will test the unit in the target environment by triggering unit methods/functions and verifying the behavior. Stubbed interfaces/mocking techniques can be used to meet the code coverage requirements. This means that unit tests shall be written according to the detailed requirements. Requirement source is SW design.

##### Unit test case naming convention

By introducing a naming convention for unit test cases a harmonized test code-base can be achieved. This simplifies reading and understanding the intention of the unit test case. Please see the naming convention defined [in Rust coding guidelines](rust-coding-guidelines.md).

##### Unit test organization

The unit tests shall be written in the same file as the source code like suggested in the [Rust Language Book](https://doc.rust-lang.org/book/ch11-03-test-organization.html) and shall be prefixed with `utest_`.

###### Example for unit tests in source file in Rust

At the end of the file e.g. `my_module/src/my_component.rs`:

```rust
...
fn my_algorithm(input: i32) -> Vec<u8> {
    ...
}

async fn my_async_function(input: i32) -> Vec<u8> {
    ...
}
...
#[cfg(test)]
mod tests {
    ...
    #[test]
    fn utest_my_algorithm_returns_empty_array_when_input_is_0_or_negative() {
        ...
    }

    #[tokio::test]
    async fn utest_my_async_function_returns_empty_array_when_input_is_0_or_negative() {
        ...
    }
}
```

#### Test Execution and Reports

Unit test cases are executed manually by the developer during implementation phase and later automatically in CI builds. Unit test and coverage reports are generated and stored automatically by the CI build system.
If unit test case fails before code is merged to main branch (merge verification), the merge is not allowed until the issue is fixed. If unit test case fails after the code is merged to main branch, it is reported via email and fixed via internal Jira ticket reported by the developer.

Regression testing is done by the [CI build system](https://github.com/eclipse-ankaios/ankaios/actions).

#### Goals and Metrics

The following table show how test coverage is currently shown in the coverage report:

| Goal         | Metric | Red   | Yellow | Green |
|--------------|--------|-------|--------|-------|
| Code coverage|        | <80%  | >80%   | 100%  |

Currently there is no proper way of explicitly excluding parts of the code from the test coverage report in order to get to an easily observable value of 100%. The explicitly excluded code would have a corresponding comment stating the reason for excluding it.
As this is not possible, we would initially **target at least 80% line coverage in each file**.
