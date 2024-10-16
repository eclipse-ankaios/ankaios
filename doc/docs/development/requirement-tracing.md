# Requirement tracing

## Introduction

The Eclipse Ankaios project provides requirement tracing using the [OpenFastTrace](https://github.com/itsallcode/openfasttrace) requirement tracing suite. The dev container already includes the required tooling. To generate a requirement tracing report call:

```shell
just trace-requirements
```

Afterwards the HTML report is available under `build/req/req_tracing_report.html` and shows the current coverage state.

For details on the OpenFastTrace tool, please consult [OFT's user documentation](https://github.com/itsallcode/openfasttrace/blob/main/doc/user_guide.md) or execute `oft help`.

## Adding requirements

Eclipse Ankaios traces requirements between

* Design (`**/doc/README.md`)
* Implementations (`**/src/**`)
* Tests (`**/src/**`, `tests/**`)

Thus, for new features:

* New requirements need to be added in the design or existing requirements need to be modified (type `swdd`)
* Mark the parts in the source code that actually implement the design requirement using the type `impl`, e.g., `// [impl->swdd~this-is-a-requirement~1]`
* Mark the tests that check the implementation of the design with one of the types `utest`, `itest` or `stest` depending on the type of the test, e.g., `// [utest->swdd~this-is-a-requirement~1]` for a unit test

The format of a requirement is described in the next section [Requirement description template](requirement-template.md).
