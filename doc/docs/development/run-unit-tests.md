# Run unit tests

If you want to run all unit tests **without** traces, call in the root of the project:

```shell
cargo nextest
```

Some unit tests can print trace logs.
If you want to see them, you have to set the `RUST_LOG` environment variable **before** running unit tests.
The tests in the agent are multithreaded.
Tests are not stated one-by-one, but tests are started in parallel in its own threads.
This makes difficult to read trace logs, because it is hard to find which trace belongs to which test.
If you want to have traces sorted by unit tests, call this (recommended):

```shell
cargo nextest -- --show-output
```

Rust also allows to run only a subset of unit tests.
You have to se the "filter string" in the command:

```shell
cargo nextest <filter string> [-- --show-output]
```

Where the `filter string` is part of unit test name. For example we have a unit test with the name:

```shell
test podman::workload::container_create_success
```

If you want to call only this test, you can call:

```shell
cargo nextest workload::container_create_success [-- --show-output]
```

If you want to call all tests in `workload.rs`, you have to call:

```shell
cargo nextest podman::workload [-- --show-output]
```

You can also call only tests in `workload.rs`, which have a name starting with `container`:

```shell
cargo nextest podman::workload::container [-- --show-output]
```
