# Events

Workloads can subscribe for changes on the state of the cluster.
This is done by sending a [CompleteStateRequest](./_ankaios.proto.md#completestaterequest) with the `subscribeForEvents` flag set to true.
The `fieldMask` in the request can be used to specify for which fields updates should be sent.

To see an example of how to use events, see the [Tutorial: Registering for events](../usage/tutorial-events.md).

## Authorization

The same rules as for a normal `CompleteStateRequest` apply.
A `StateRule` with a `filterMask` matching the requested `fieldMask` must be present for the workload.
See the section on [Authorization in Control Interface](./control-interface.md#authorization) for more details.

## Workflow

1. The workload chooses a unique ID and subscribes by sending a `CompleteStateRequest` with `subscribeForEvents` set to true using this ID.
2. Ankaios responds with an initial `CompleteStateResponse` using the same ID containing the current state.
   As this is the complete current state, the `alteredFields` field is not set and the state contains all requested fields.
   This response also indicates that the subscription was accepted.
3. Whenever the requested fields change, ankaios sends a `CompleteStateResponse` using the same ID with the `alteredFields` field set to the changed fields.
   The state in this response only contains the changed fields.
4. The workload can unsubscribe by sending a `EventsCancelRequest` using the same ID as the initial request.
5. Ankaios responds with a `EventsCancelAccepted` using the same ID confirming the unsubscription.

## Wildcards

As the default `CompleteStateRequest`, the subscription to events supports wildcards in the `fieldMask`.
Each element of a `fieldMask` (the part between two "." symbols) can be replaced by a `*`.
This is useful for matching dynamic fields, like the IDs of workloads, or names of agents and workloads.
E.g. to subscribe to all state changes of all workloads use the field mask `workloadStates.*.*.*.state`.

## Custom Events

It is also possible to use the `configs` field of [State](./_ankaios.proto.md#state) to send and receive custom events.
The sender sets a special config with the desired event data, and receivers can subscribe to changes of this config.

The authorization rules allow to exactly determine who is allowed to send and receive these custom events.
