# cargo-nextest

We use test runner [cargo-nextest](https://nexte.st/index.html) because of the following reasons:
1. It runs tests faster than `cargo test`.
2. It presents the test results concisely so you can see which tests passed and failed at a glance.
3. If debug logs are activated, it prints the debug logs only when a test has failed, so that it is clear the debug logs belong that failed test.

# Run unit tests

If you want to run all unit tests **without** traces, call in the root of the project:

```shell
cargo nextest run
```

Some unit tests can print trace logs.
If you want to see them, you have to set the `RUST_LOG` environment variable **before** running unit tests.

```shell
RUST_LOG=debug cargo nextest run
```

[Cargo-nextest](https://nexte.st/index.html) also allows to run only a subset of unit tests.
You have to set the "filter string" in the command:

```shell
cargo nextest run <filter string>
```

Where the `filter string` is part of unit test name. For example we have a unit test with the name:

```shell
test podman::workload::container_create_success
```

If you want to call only this test, you can call:

```shell
cargo nextest run workload::container_create_success
```

If you want to call all tests in `workload.rs`, you have to call:

```shell
cargo nextest run podman::workload
```

You can also call only tests in `workload.rs`, which have a name starting with `container`:

```shell
cargo nextest run podman::workload::container
```
