# Internal derive macros for automatically generating <>Internal objects

This create provides the functionality to automatically derive <Orig>Internal objects from an Orig custom type

## Usage

Todo

### Debugging

Debugging procedural macros is not very easy, but still achievable although different compared with debugging production code or tests.

There is a [good article on debugging from ferrous systems](https://ferrous-systems.com/blog/testing-proc-macros/) about testing proc macros in general.
In the following, the basics are summarized for easier entry in the topic:

Todo

#### Using cargo expand

[Cargo expand](https://github.com/dtolnay/cargo-expand) is a nice way to see the output of the macros without having to add `println`s in the code.
The only problem is the sheer amount of produced output as also the derives for `Clone`, `Debug`, serde, etc. are in the output.

**Note** that you need to be in the `api` crate where the macros are used to be able to run `cargo expand`.

To get a focused results, one has to get a bit creative, e.g., to get the derived struct for the internal workload `WorkloadInternal` use the following command:

```bash
cargo expand | grep "struct WorkloadInternal {" --before-context 5 --after-context 20
```

Which would output 5 lines before the searched string and 20 lines after.

The same way the `from` implementation can be viewed too:

```bash
cargo expand | grep " From<WorkloadInternal>.*for Workload {" --before-context 5 --after-context 20
```
