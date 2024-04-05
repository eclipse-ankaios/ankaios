# Interacting with Ankaios

Ankaios offers two ways of dynamically interacting with a running cluster - the `ank` CLI and the [control interface](./control-interface.md).

The `ank` CLI is targeted at integrators or [workload](./glossary.md#workload) developers that want to interact with the cluster during development or for a manual intervention. It is developed for ergonomics and not automation purposes. If required, an external application can connect to the interface used by the CLI, but this is not the standard way of automating a dynamic reconfiguration of the cluster during runtime.

The Ankaios [control interface](./control-interface.md) is provided to [workloads](./glossary.md#workload) managed by Ankaios and allows implementing the so-called "operator pattern". The [control interface](./control-interface.md) allows each workload to send messages to the agent managing it. After successful authorization, the Ankaios agent forwards the request to the Ankaios server and provides the response to the requesting workload. Through the control interface, a workload has the capability to obtain the complete state of the Ankaios cluster or administer the cluster by declaratively adjusting its state, thereby facilitating the addition or removal of other workloads.
