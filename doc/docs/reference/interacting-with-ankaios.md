# Interacting with Ankaios

Ankaios offers two ways of dynamically interacting with a running cluster - the `ank` CLI and the [control interface](./control-interface.md).

The `ank` CLI is targeted and integrators or [workload](./glossary.md#workload) developers that want to interact with the cluster during development or for a manual intervention. It is developed for ergonomics and not automation purposes. If required an external application can doc at the interface used by the CLI, but this is not the standard way of automating a dynamic reconfiguration of the cluster during runtime.

The Ankaios [control interface](./control-interface.md) is provided to [workloads](./glossary.md#workload) managed by Ankaios and allows implementing the so-called "operator pattern". The [control interface](./control-interface.md) allows each workload to send messages to the agent managing it. After successful authorization, the Ankaios agent forwards the request to the Ankaios server and provides the response to the requesting workload. Using the control interface a workload can retrieve the complete state of the Ankaios cluster or manage the cluster by declaratively updating the cluster state and with this adding or removing other workloads.
