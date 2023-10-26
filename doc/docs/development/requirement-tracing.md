# Requirement tracing

## Introduction

The Eclipse Ankaios project provides requirement tracing using the [OpenFastTrace](https://github.com/itsallcode/openfasttrace) requirement tracing suite. The dev container already includes the required tooling. To generate a requirement tracing report call:

```shell
tools/generate_oft_html_report.sh
```

Afterwards the HTML report is available under `build/req/req_tracing_report.html` and shows the current coverage state.

For details on the OpenFastTrace tool, please consult [OFT's user documentation](https://github.com/itsallcode/openfasttrace/blob/main/doc/user_guide.md) or execute `oft help`.

## Adding requirements

Eclipse Ankaios traces requirements between 

* Design (`**/doc/README.md`)
* Implementations (`**/src/**`)
* Tests (`**/src/**`, `tests/**`)

So for new features 

* New requirements need to be added in the design or existing requirements need to be modified (type `swdd`)
* Mark the parts in the source code that actually implement the design requirement (type `impl`)
* Mark the tests that test the design and implementation (type `utest`, `itest` or `stest`)
