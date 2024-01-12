# API

Ankaios offers an API to alter the current state.
The API is constructed with message data structures described in the [protocol documentation](./_ankaios.proto.md#protocol-documentation).
Ankaios provides a [gRPC](https://grpc.io/docs/what-is-grpc/introduction/) API which can be used during development. The provided ank CLI uses this API, but the API can also be used directly. Ankaios also provides the [control interface](./control-interface.md) API to the managed [workloads](./glossary.md#workload) which allows [workloads](./glossary.md#workload) to alter the current/stored state.
