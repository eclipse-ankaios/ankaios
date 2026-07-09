# Protocol Documentation

## Table of Contents

- [control_api.proto](#control_api-proto)

  - [ConnectionClosed](#control_api-ConnectionClosed)
  - [ControlInterfaceAccepted](#control_api-ControlInterfaceAccepted)
  - [FromAnkaios](#control_api-FromAnkaios)
  - [Hello](#control_api-Hello)
  - [ToAnkaios](#control_api-ToAnkaios)

- [ank_base.proto](#ank_base-proto)

  - [AccessRightsRule](#ank_base-AccessRightsRule)
  - [AgentAttributes](#ank_base-AgentAttributes)
  - [AgentMap](#ank_base-AgentMap)
  - [AgentMap.AgentsEntry](#ank_base-AgentMap-AgentsEntry)
  - [AgentStatus](#ank_base-AgentStatus)
  - [AlteredFields](#ank_base-AlteredFields)
  - [CompleteState](#ank_base-CompleteState)
  - [CompleteStateRequest](#ank_base-CompleteStateRequest)
  - [CompleteStateResponse](#ank_base-CompleteStateResponse)
  - [ConfigArray](#ank_base-ConfigArray)
  - [ConfigItem](#ank_base-ConfigItem)
  - [ConfigMap](#ank_base-ConfigMap)
  - [ConfigMap.ConfigsEntry](#ank_base-ConfigMap-ConfigsEntry)
  - [ConfigMappings](#ank_base-ConfigMappings)
  - [ConfigMappings.ConfigsEntry](#ank_base-ConfigMappings-ConfigsEntry)
  - [ConfigObject](#ank_base-ConfigObject)
  - [ConfigObject.FieldsEntry](#ank_base-ConfigObject-FieldsEntry)
  - [ControlInterfaceAccess](#ank_base-ControlInterfaceAccess)
  - [CpuUsage](#ank_base-CpuUsage)
  - [Dependencies](#ank_base-Dependencies)
  - [Dependencies.DependenciesEntry](#ank_base-Dependencies-DependenciesEntry)
  - [Error](#ank_base-Error)
  - [EventsCancelAccepted](#ank_base-EventsCancelAccepted)
  - [EventsCancelRequest](#ank_base-EventsCancelRequest)
  - [ExecutionState](#ank_base-ExecutionState)
  - [ExecutionsStatesForId](#ank_base-ExecutionsStatesForId)
  - [ExecutionsStatesForId.IdStateMapEntry](#ank_base-ExecutionsStatesForId-IdStateMapEntry)
  - [ExecutionsStatesOfWorkload](#ank_base-ExecutionsStatesOfWorkload)
  - [ExecutionsStatesOfWorkload.WlNameStateMapEntry](#ank_base-ExecutionsStatesOfWorkload-WlNameStateMapEntry)
  - [File](#ank_base-File)
  - [Files](#ank_base-Files)
  - [FreeMemory](#ank_base-FreeMemory)
  - [LogEntriesResponse](#ank_base-LogEntriesResponse)
  - [LogEntry](#ank_base-LogEntry)
  - [LogRule](#ank_base-LogRule)
  - [LogsCancelAccepted](#ank_base-LogsCancelAccepted)
  - [LogsCancelRequest](#ank_base-LogsCancelRequest)
  - [LogsRequest](#ank_base-LogsRequest)
  - [LogsRequestAccepted](#ank_base-LogsRequestAccepted)
  - [LogsStopResponse](#ank_base-LogsStopResponse)
  - [Request](#ank_base-Request)
  - [Response](#ank_base-Response)
  - [State](#ank_base-State)
  - [StateRule](#ank_base-StateRule)
  - [Tags](#ank_base-Tags)
  - [Tags.TagsEntry](#ank_base-Tags-TagsEntry)
  - [UpdateStateRequest](#ank_base-UpdateStateRequest)
  - [UpdateStateSuccess](#ank_base-UpdateStateSuccess)
  - [Workload](#ank_base-Workload)
  - [WorkloadInstanceName](#ank_base-WorkloadInstanceName)
  - [WorkloadMap](#ank_base-WorkloadMap)
  - [WorkloadMap.WorkloadsEntry](#ank_base-WorkloadMap-WorkloadsEntry)
  - [WorkloadState](#ank_base-WorkloadState)
  - [WorkloadStatesMap](#ank_base-WorkloadStatesMap)
  - [WorkloadStatesMap.AgentStateMapEntry](#ank_base-WorkloadStatesMap-AgentStateMapEntry)
  - [AddCondition](#ank_base-AddCondition)
  - [AgentDisconnected](#ank_base-AgentDisconnected)
  - [Failed](#ank_base-Failed)
  - [NotScheduled](#ank_base-NotScheduled)
  - [Pending](#ank_base-Pending)
  - [ReadWriteEnum](#ank_base-ReadWriteEnum)
  - [Removed](#ank_base-Removed)
  - [RestartPolicy](#ank_base-RestartPolicy)
  - [Running](#ank_base-Running)
  - [Stopping](#ank_base-Stopping)
  - [Succeeded](#ank_base-Succeeded)

- [Scalar Value Types](#scalar-value-types)

[Top](#top)

## control_api.proto

The Ankaios Control Interface is used in the communication between a workload and Ankaios

The protocol consists of the following top-level message types:

1. [ToAnkaios](#toankaios): workload -> ankaios
1. [FromAnkaios](#fromankaios): ankaios -> workload

### ConnectionClosed

This message informs the user of the Control Interface that the connection was closed by Ankaios. No more messages will be processed by Ankaios after this message is sent.

| Field  | Type              | Label | Description                                                |
| ------ | ----------------- | ----- | ---------------------------------------------------------- |
| reason | [string](#string) |       | A string containing the reason for closing the connection. |

### ControlInterfaceAccepted

A message indicating that the control interface connection is accepted. This message is sent in response to a hello message from the workload to Ankaios.

### FromAnkaios

Messages from the Ankaios server to e.g. the Ankaios agent.

| Field                    | Type                                                              | Label | Description                                                                               |
| ------------------------ | ----------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------- |
| response                 | [ank_base.Response](#ank_base-Response)                           |       | A message containing a response to a previous request.                                    |
| controlInterfaceAccepted | [ControlInterfaceAccepted](#control_api-ControlInterfaceAccepted) |       | A message indicating that the control interface connection is accepted.                   |
| connectionClosed         | [ConnectionClosed](#control_api-ConnectionClosed)                 |       | A message sent by Ankaios to inform a workload that the connection to Ankaios was closed. |

### Hello

This message is the first one that needs to be sent when a new connection to the Ankaios cluster is established. Without this message being sent all further request are rejected.

| Field           | Type              | Label | Description                                         |
| --------------- | ----------------- | ----- | --------------------------------------------------- |
| protocolVersion | [string](#string) |       | The protocol version used by the calling component. |

### ToAnkaios

Messages to the Ankaios server.

| Field   | Type                                  | Label | Description                                                                                                                         |
| ------- | ------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------------------- |
| hello   | [Hello](#control_api-Hello)           |       | The fist message sent when a connection is established. The message is needed to make sure the connected components are compatible. |
| request | [ank_base.Request](#ank_base-Request) |       | A request to Ankaios                                                                                                                |

[Top](#top)

## ank_base.proto

### AccessRightsRule

A message containing an allow or deny rule.

| Field     | Type                             | Label | Description                           |
| --------- | -------------------------------- | ----- | ------------------------------------- |
| stateRule | [StateRule](#ank_base-StateRule) |       | Rule for getting or setting the state |
| logRule   | [LogRule](#ank_base-LogRule)     |       | Rule for getting workload logs        |

### AgentAttributes

A message that contains attributes of the agent.

| Field  | Type                                 | Label | Description                          |
| ------ | ------------------------------------ | ----- | ------------------------------------ |
| status | [AgentStatus](#ank_base-AgentStatus) |       | The status information of the agent. |
| tags   | [Tags](#ank_base-Tags)               |       | A list of tag names.                 |

### AgentMap

A nested map that provides the names of the connected agents and their optional attributes. The first level allows searches by agent name.

| Field  | Type                                                   | Label    | Description |
| ------ | ------------------------------------------------------ | -------- | ----------- |
| agents | [AgentMap.AgentsEntry](#ank_base-AgentMap-AgentsEntry) | repeated |             |

### AgentMap.AgentsEntry

| Field | Type                                         | Label | Description |
| ----- | -------------------------------------------- | ----- | ----------- |
| key   | [string](#string)                            |       |             |
| value | [AgentAttributes](#ank_base-AgentAttributes) |       |             |

### AgentStatus

A message that contains resource availability information of the agent.

| Field       | Type                               | Label | Description                             |
| ----------- | ---------------------------------- | ----- | --------------------------------------- |
| cpu_usage   | [CpuUsage](#ank_base-CpuUsage)     |       | The cpu usage of the agent.             |
| free_memory | [FreeMemory](#ank_base-FreeMemory) |       | The amount of free memory of the agent. |

### AlteredFields

A message containing a list of fields that were added, updated, or removed in the state compared to the previous state.

| Field         | Type              | Label    | Description                                                               |
| ------------- | ----------------- | -------- | ------------------------------------------------------------------------- |
| addedFields   | [string](#string) | repeated | The fields that were added in the state compared to the previous state.   |
| updatedFields | [string](#string) | repeated | The fields that were updated in the state compared to the previous state. |
| removedFields | [string](#string) | repeated | The fields that were removed in the state compared to the previous state. |

### CompleteState

A message containing the complete state of the Ankaios system. This is a response to the [CompleteStateRequest](#completestaterequest) message.

| Field          | Type                                             | Label | Description                                                                                                                                                          |
| -------------- | ------------------------------------------------ | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| desiredState   | [State](#ank_base-State)                         |       | The state the user wants to reach.                                                                                                                                   |
| workloadStates | [WorkloadStatesMap](#ank_base-WorkloadStatesMap) |       | The current execution states of the workloads.                                                                                                                       |
| agents         | [AgentMap](#ank_base-AgentMap)                   |       | The agents currently connected to the Ankaios cluster.                                                                                                               |
| effectiveState | [State](#ank_base-State)                         |       | The rendered state with expanded templates after hooks mutations. Reflects the actual workload configurations sent to agents. Configs not filled as already applied. |

### CompleteStateRequest

A message containing a request for the complete/partial state of the Ankaios system. This is usually answered with a [CompleteState](#completestate) message.

| Field              | Type              | Label    | Description                                                                                                                                                                                            |
| ------------------ | ----------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| fieldMask          | [string](#string) | repeated | A list of symbolic field paths within the State message structure, e.g., 'desiredState.workloads.nginx'.                                                                                               |
| subscribeForEvents | [bool](#bool)     |          | If true, the server will send updates to the client whenever the state changes. The updates will contain only the fields specified in the fieldMask. If false, the server will send only one response. |

### CompleteStateResponse

A message containing the complete state of the Ankaios system along with information about what has changed. This is a response to the [CompleteStateRequest](#completestaterequest) message. If this is the first response to a CompleteStateRequest, the addedFields, updatedFields, and removedFields will all be empty.

| Field         | Type                                     | Label | Description                                                                                  |
| ------------- | ---------------------------------------- | ----- | -------------------------------------------------------------------------------------------- |
| completeState | [CompleteState](#ank_base-CompleteState) |       | The complete state of the Ankaios system.                                                    |
| alteredFields | [AlteredFields](#ank_base-AlteredFields) |       | The fields that were added, updated, or removed in the state compared to the previous state. |

### ConfigArray

A container for the array of config items.

| Field  | Type                               | Label    | Description                      |
| ------ | ---------------------------------- | -------- | -------------------------------- |
| values | [ConfigItem](#ank_base-ConfigItem) | repeated | An array of configuration items. |

### ConfigItem

A configuration mapping that can be a simple string, a mapping or an array of other configurations items.

The keys cannot be empty nor be longer than 63 symbols and can contain only regular characters, digits and the symbols "-" and "\_".

| Field  | Type                                   | Label | Description |
| ------ | -------------------------------------- | ----- | ----------- |
| String | [string](#string)                      |       |             |
| array  | [ConfigArray](#ank_base-ConfigArray)   |       |             |
| object | [ConfigObject](#ank_base-ConfigObject) |       |             |

### ConfigMap

Configuration values which can be referenced in workload configurations by mapping them the configs field of the workload definition.

| Field   | Type                                                       | Label    | Description                                                  |
| ------- | ---------------------------------------------------------- | -------- | ------------------------------------------------------------ |
| configs | [ConfigMap.ConfigsEntry](#ank_base-ConfigMap-ConfigsEntry) | repeated | This is a workaround for proto not supporting optional maps. |

### ConfigMap.ConfigsEntry

| Field | Type                               | Label | Description |
| ----- | ---------------------------------- | ----- | ----------- |
| key   | [string](#string)                  |       |             |
| value | [ConfigItem](#ank_base-ConfigItem) |       |             |

### ConfigMappings

This is a workaround for proto not supporting optional maps.

| Field   | Type                                                                 | Label    | Description                                                             |
| ------- | -------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------- |
| configs | [ConfigMappings.ConfigsEntry](#ank_base-ConfigMappings-ConfigsEntry) | repeated | A message containing the configuration mappings assigned to a workload. |

The keys and the values cannot be empty nor be longer than 63 symbols and can contain only regular characters, digits and the symbols "-" and "\_". |

### ConfigMappings.ConfigsEntry

| Field | Type              | Label | Description |
| ----- | ----------------- | ----- | ----------- |
| key   | [string](#string) |       |             |
| value | [string](#string) |       |             |

### ConfigObject

A mapping of configuration items.

The keys cannot be empty nor be longer than 63 symbols and can contain only regular characters, digits and the symbols "-" and "\_".

| Field  | Type                                                           | Label    | Description                       |
| ------ | -------------------------------------------------------------- | -------- | --------------------------------- |
| fields | [ConfigObject.FieldsEntry](#ank_base-ConfigObject-FieldsEntry) | repeated | A mapping of configuration items. |

### ConfigObject.FieldsEntry

| Field | Type                               | Label | Description |
| ----- | ---------------------------------- | ----- | ----------- |
| key   | [string](#string)                  |       |             |
| value | [ConfigItem](#ank_base-ConfigItem) |       |             |

### ControlInterfaceAccess

A message containing the parts of the control interface the workload as authorized to access. By default, all access is denied. Only if a matching allow rule is found, and no matching deny rules is found, the access is allowed.

| Field      | Type                                           | Label    | Description              |
| ---------- | ---------------------------------------------- | -------- | ------------------------ |
| allowRules | [AccessRightsRule](#ank_base-AccessRightsRule) | repeated | Rules allow the access   |
| denyRules  | [AccessRightsRule](#ank_base-AccessRightsRule) | repeated | Rules denying the access |

### CpuUsage

A message containing the CPU usage information of the agent.

| Field     | Type              | Label | Description                                                                                                                              |
| --------- | ----------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| cpu_usage | [uint32](#uint32) |       | expressed in percent, the formula for calculating: cpu_usage = (new_work_time - old_work_time) / (new_total_time - old_total_time) * 100 |

### Dependencies

This is a workaround for proto not supporting optional maps.

| Field        | Type                                                                       | Label    | Description                                          |
| ------------ | -------------------------------------------------------------------------- | -------- | ---------------------------------------------------- |
| dependencies | [Dependencies.DependenciesEntry](#ank_base-Dependencies-DependenciesEntry) | repeated | A mapping from a workload name to an expected state. |

### Dependencies.DependenciesEntry

| Field | Type                                   | Label | Description |
| ----- | -------------------------------------- | ----- | ----------- |
| key   | [string](#string)                      |       |             |
| value | [AddCondition](#ank_base-AddCondition) |       |             |

### Error

Indicated an error in the communication with Ankaios.

| Field   | Type              | Label | Description                                    |
| ------- | ----------------- | ----- | ---------------------------------------------- |
| message | [string](#string) |       | A message specifying the reason for the error. |

### EventsCancelAccepted

A message indicating that the request for canceling the event campaign was accepted. Please note that the actual stopping of the event campaign could take longer so other event notifications could be delivered also after receiving this message.

### EventsCancelRequest

A message stopping the event campaign previously started.

### ExecutionState

A message containing information about the detailed state of a workload in the Ankaios system.

| Field             | Type                                             | Label    | Description                                                                                                                                |
| ----------------- | ------------------------------------------------ | -------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| additionalInfo    | [string](#string)                                | optional | The additional info contains more detailed information from the runtime regarding the execution state.                                     |
| agentDisconnected | [AgentDisconnected](#ank_base-AgentDisconnected) |          | The exact state of the workload cannot be determined, e.g., because of a broken connection to the responsible agent.                       |
| pending           | [Pending](#ank_base-Pending)                     |          | The workload is going to be started eventually.                                                                                            |
| running           | [Running](#ank_base-Running)                     |          | The workload is operational.                                                                                                               |
| stopping          | [Stopping](#ank_base-Stopping)                   |          | The workload is scheduled for stopping.                                                                                                    |
| succeeded         | [Succeeded](#ank_base-Succeeded)                 |          | The workload has successfully finished its operation.                                                                                      |
| failed            | [Failed](#ank_base-Failed)                       |          | The workload has failed or is in a degraded state.                                                                                         |
| notScheduled      | [NotScheduled](#ank_base-NotScheduled)           |          | The workload is not scheduled to run at any agent. This is signalized with an empty agent in the workload specification.                   |
| removed           | [Removed](#ank_base-Removed)                     |          | The workload was removed from Ankaios. This state is used only internally in Ankaios. The outside world removed states are just not there. |

### ExecutionsStatesForId

A map providing the execution state of a specific workload for a given id. This level is needed as a workload could be running more than once on one agent in different versions.

| Field      | Type                                                                                     | Label    | Description |
| ---------- | ---------------------------------------------------------------------------------------- | -------- | ----------- |
| idStateMap | [ExecutionsStatesForId.IdStateMapEntry](#ank_base-ExecutionsStatesForId-IdStateMapEntry) | repeated |             |

### ExecutionsStatesForId.IdStateMapEntry

| Field | Type                                       | Label | Description |
| ----- | ------------------------------------------ | ----- | ----------- |
| key   | [string](#string)                          |       |             |
| value | [ExecutionState](#ank_base-ExecutionState) |       |             |

### ExecutionsStatesOfWorkload

A map providing the execution state of a workload for a given name.

| Field          | Type                                                                                                       | Label    | Description |
| -------------- | ---------------------------------------------------------------------------------------------------------- | -------- | ----------- |
| wlNameStateMap | [ExecutionsStatesOfWorkload.WlNameStateMapEntry](#ank_base-ExecutionsStatesOfWorkload-WlNameStateMapEntry) | repeated |             |

### ExecutionsStatesOfWorkload.WlNameStateMapEntry

| Field | Type                                                     | Label | Description |
| ----- | -------------------------------------------------------- | ----- | ----------- |
| key   | [string](#string)                                        |       |             |
| value | [ExecutionsStatesForId](#ank_base-ExecutionsStatesForId) |       |             |

### File

A message describing a file with a mount point and file content.

| Field      | Type              | Label | Description                                             |
| ---------- | ----------------- | ----- | ------------------------------------------------------- |
| mountPoint | [string](#string) |       | The path where the file is mounted inside the workload. |
| data       | [string](#string) |       | The content of the file.                                |
| binaryData | [string](#string) |       | The base64 encoded content of the file.                 |

### Files

This is a workaround for proto not supporting optional arrays.

| Field | Type                   | Label    | Description                                 |
| ----- | ---------------------- | -------- | ------------------------------------------- |
| files | [File](#ank_base-File) | repeated | A vector with files assigned to a workload. |

### FreeMemory

A message containing the amount of free memory of the agent.

| Field       | Type              | Label | Description        |
| ----------- | ----------------- | ----- | ------------------ |
| free_memory | [uint64](#uint64) |       | expressed in bytes |

### LogEntriesResponse

A message containing the requested logs.

| Field      | Type                           | Label    | Description                          |
| ---------- | ------------------------------ | -------- | ------------------------------------ |
| logEntries | [LogEntry](#ank_base-LogEntry) | repeated | The logs of the requested workloads. |

### LogEntry

A message containing a single log entry.

| Field        | Type                                                   | Label | Description                                             |
| ------------ | ------------------------------------------------------ | ----- | ------------------------------------------------------- |
| workloadName | [WorkloadInstanceName](#ank_base-WorkloadInstanceName) |       | The name of the workloads for which logs are requested. |
| message      | [string](#string)                                      |       | The log message.                                        |

### LogRule

Message containing a rule for getting workload logs.

| Field         | Type              | Label    | Description                                     |
| ------------- | ----------------- | -------- | ----------------------------------------------- |
| workloadNames | [string](#string) | repeated | The names of the workloads the rule applies to. |

The item cannot not be empty and must represent a valid workload name with at most one wildcard "\*" matching zero or multiple characters. The length must be at most 64 characters (accounting the wildcard). |

### LogsCancelAccepted

A message indicating that the request for canceling the log collection was accepted. Please note that the actual stopping of the log collection campaign could take longer so other log response messages could be delivered also after receiving this message.

### LogsCancelRequest

A message stopping the streaming of logs requested via a request with the follow flag.

### LogsRequest

A message requesting workload logs.

| Field         | Type                                                   | Label    | Description                                                                                                |
| ------------- | ------------------------------------------------------ | -------- | ---------------------------------------------------------------------------------------------------------- |
| workloadNames | [WorkloadInstanceName](#ank_base-WorkloadInstanceName) | repeated | The names of the workloads for which logs are requested.                                                   |
| follow        | [bool](#bool)                                          | optional | If true, the server will stream the logs to the client until a LogsCancelRequest with the same Id is sent. |
| tail          | [int32](#int32)                                        | optional | The number of lines to show from the end of the logs. Default is -1, which means all logs.                 |
| since         | [string](#string)                                      | optional | Only return logs after a specific TIMESTAMP. The TIMESTAMP is a string in RFC3339 format.                  |
| until         | [string](#string)                                      | optional | Only return logs before a specific TIMESTAMP. The TIMESTAMP is a string in RFC3339 format.                 |

### LogsRequestAccepted

A message indicating that the logs were successfully requested and for which workloads.

| Field         | Type                                                   | Label    | Description                                                                               |
| ------------- | ------------------------------------------------------ | -------- | ----------------------------------------------------------------------------------------- |
| workloadNames | [WorkloadInstanceName](#ank_base-WorkloadInstanceName) | repeated | The instance names of the workloads for which the logs request was successfully accepted. |

### LogsStopResponse

This message is sent when no more logs can be sampled from a workload.

| Field        | Type                                                   | Label | Description                                                       |
| ------------ | ------------------------------------------------------ | ----- | ----------------------------------------------------------------- |
| workloadName | [WorkloadInstanceName](#ank_base-WorkloadInstanceName) |       | The name of the workload for which the log streaming was stopped. |

### Request

A message containing a request to the Ankaios server to update the state or to request the complete state of the Ankaios system.

| Field                | Type                                                   | Label | Description                                                                                                    |
| -------------------- | ------------------------------------------------------ | ----- | -------------------------------------------------------------------------------------------------------------- |
| requestId            | [string](#string)                                      |       | The unique request Id for this request.                                                                        |
| updateStateRequest   | [UpdateStateRequest](#ank_base-UpdateStateRequest)     |       | A message to Ankaios server to update the state of one or more agent(s).                                       |
| completeStateRequest | [CompleteStateRequest](#ank_base-CompleteStateRequest) |       | A message to Ankaios server to request the complete state by the given request id and the optional field mask. |
| logsRequest          | [LogsRequest](#ank_base-LogsRequest)                   |       | A message to Ankaios server to request workload logs.                                                          |
| logsCancelRequest    | [LogsCancelRequest](#ank_base-LogsCancelRequest)       |       | A message to Ankaios server to stop the request for workload logs.                                             |
| eventsCancelRequest  | [EventsCancelRequest](#ank_base-EventsCancelRequest)   |       | A message to Ankaios server to stop a specific event campaign.                                                 |

### Response

A message containing a response from the Ankaios server to a particular request. The response content depends on the request content previously sent to the Ankaios server.

| Field                 | Type                                                     | Label | Description                                                                                                                                                                 |
| --------------------- | -------------------------------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| requestId             | [string](#string)                                        |       | The Id corresponding to the one of the request for this response.                                                                                                           |
| error                 | [Error](#ank_base-Error)                                 |       |                                                                                                                                                                             |
| completeStateResponse | [CompleteStateResponse](#ank_base-CompleteStateResponse) |       | A message containing the complete state (or a filtered part) on the Ankaios cluster.                                                                                        |
| UpdateStateSuccess    | [UpdateStateSuccess](#ank_base-UpdateStateSuccess)       |       | A message confirming the state update and providing information on the changed workloads.                                                                                   |
| logsRequestAccepted   | [LogsRequestAccepted](#ank_base-LogsRequestAccepted)     |       | A message containing the workload names for which the logs were successfully requested.                                                                                     |
| logEntriesResponse    | [LogEntriesResponse](#ank_base-LogEntriesResponse)       |       | A message containing workload logs.                                                                                                                                         |
| logsStopResponse      | [LogsStopResponse](#ank_base-LogsStopResponse)           |       | A message containing the workload instance name indicating the stop of the log streaming.                                                                                   |
| logsCancelAccepted    | [LogsCancelAccepted](#ank_base-LogsCancelAccepted)       |       | A message indicating that the request for canceling the log collection was accepted. Please note that the actual stopping of the log collection campaign could take longer. |
| eventsCancelAccepted  | [EventsCancelAccepted](#ank_base-EventsCancelAccepted)   |       | A message indicating that the request for canceling the event campaign was accepted. Please note that the actual stopping of the event campaign could take longer.          |

### State

A message containing the state information.

| Field      | Type                                 | Label | Description                                                              |
| ---------- | ------------------------------------ | ----- | ------------------------------------------------------------------------ |
| apiVersion | [string](#string)                    |       | The current version of the API.                                          |
| workloads  | [WorkloadMap](#ank_base-WorkloadMap) |       | A mapping from workload names to workload configurations.                |
| configs    | [ConfigMap](#ank_base-ConfigMap)     |       | Configuration values which can be referenced in workload configurations. |

### StateRule

Message containing a rule for getting or setting the state

| Field       | Type                                     | Label    | Description                                                          |
| ----------- | ---------------------------------------- | -------- | -------------------------------------------------------------------- |
| operation   | [ReadWriteEnum](#ank_base-ReadWriteEnum) |          | Defines which actions are allowed                                    |
| filterMasks | [string](#string)                        | repeated | Paths defining the parts of the complete state that can be accessed. |

Each item must not be empty and be composed of a wildcard "*" or regular characters, digits and the symbols "-" and "\_" separated with points ".", e.g., desiredState.workloads.*.agent |

### Tags

This is a workaround for proto not supporting optional maps.

| Field | Type                                       | Label    | Description                        |
| ----- | ------------------------------------------ | -------- | ---------------------------------- |
| tags  | [Tags.TagsEntry](#ank_base-Tags-TagsEntry) | repeated | Tags describing custom properties. |

### Tags.TagsEntry

| Field | Type              | Label | Description |
| ----- | ----------------- | ----- | ----------- |
| key   | [string](#string) |       |             |
| value | [string](#string) |       |             |

### UpdateStateRequest

A message containing a request to update the state of the Ankaios system. The new state is provided as state object. To specify which part(s) of the new state object should be updated a list of update mask (same as field mask) paths needs to be provided.

| Field      | Type                                     | Label    | Description                                                                                                                            |
| ---------- | ---------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| newState   | [CompleteState](#ank_base-CompleteState) |          | The new state of the Ankaios system.                                                                                                   |
| updateMask | [string](#string)                        | repeated | A list of symbolic field paths within the state message structure, e.g., 'desiredState.workloads.nginx' to specify what to be updated. |

### UpdateStateSuccess

A message from the server containing the ids of the workloads that have been started and stopped in response to a previously sent UpdateStateRequest.

| Field            | Type              | Label    | Description                                                |
| ---------------- | ----------------- | -------- | ---------------------------------------------------------- |
| addedWorkloads   | [string](#string) | repeated | Workload instance names of workloads which will be started |
| deletedWorkloads | [string](#string) | repeated | Workload instance names of workloads which will be stopped |

### Workload

A message containing the configuration of a workload.

| Field | Type              | Label    | Description                   |
| ----- | ----------------- | -------- | ----------------------------- |
| agent | [string](#string) | optional | The name of the owning agent. |

Agent names shall not be longer then 63 symbols and can contain only regular characters, digits and the symbols "-" and "\_". An empty agent name indicates that the workload is not scheduled.

Agent names can also be templates referencing configuration values, e.g., "{{agent_name}}" or "{{agent_name_prefix}}-agent1" | | restartPolicy | [RestartPolicy](#ank_base-RestartPolicy) | optional | An enum value that defines the condition under which a workload is restarted. | | dependencies | [Dependencies](#ank_base-Dependencies) | | A map of workload names and expected states to enable a synchronized start of the workload. | | tags | [Tags](#ank_base-Tags) | | A map of tag names. | | runtime | [string](#string) | optional | The name of the runtime e.g. podman. | | runtimeConfig | [string](#string) | optional | The configuration information specific to the runtime. | | controlInterfaceAccess | [ControlInterfaceAccess](#ank_base-ControlInterfaceAccess) | | The authorization rules for accessing the control interface. | | configs | [ConfigMappings](#ank_base-ConfigMappings) | | A mapping containing the configurations assigned to the workload. | | files | [Files](#ank_base-Files) | | A list of files assigned to the workload. |

### WorkloadInstanceName

A message describing the unique instance name of a workload

| Field        | Type              | Label | Description                          |
| ------------ | ----------------- | ----- | ------------------------------------ |
| workloadName | [string](#string) |       | The name of the workload.            |
| agentName    | [string](#string) |       | The name of the owning Agent.        |
| id           | [string](#string) |       | A unique identifier of the workload. |

### WorkloadMap

A mapping from a workload name to a workload configuration.

Workload names shall not be empty nor be longer then 63 symbols and can contain only regular characters, digits and the symbols "-" and "\_".

| Field     | Type                                                               | Label    | Description                                                  |
| --------- | ------------------------------------------------------------------ | -------- | ------------------------------------------------------------ |
| workloads | [WorkloadMap.WorkloadsEntry](#ank_base-WorkloadMap-WorkloadsEntry) | repeated | This is a workaround for proto not supporting optional maps. |

### WorkloadMap.WorkloadsEntry

| Field | Type                           | Label | Description |
| ----- | ------------------------------ | ----- | ----------- |
| key   | [string](#string)              |       |             |
| value | [Workload](#ank_base-Workload) |       |             |

### WorkloadState

A message containing the information about the workload state.

| Field          | Type                                                   | Label | Description                   |
| -------------- | ------------------------------------------------------ | ----- | ----------------------------- |
| instanceName   | [WorkloadInstanceName](#ank_base-WorkloadInstanceName) |       |                               |
| executionState | [ExecutionState](#ank_base-ExecutionState)             |       | The workload execution state. |

### WorkloadStatesMap

A nested map that provides the execution state of a workload in a structured way. The first level allows searches by agent.

| Field         | Type                                                                                   | Label    | Description |
| ------------- | -------------------------------------------------------------------------------------- | -------- | ----------- |
| agentStateMap | [WorkloadStatesMap.AgentStateMapEntry](#ank_base-WorkloadStatesMap-AgentStateMapEntry) | repeated |             |

### WorkloadStatesMap.AgentStateMapEntry

| Field | Type                                                               | Label | Description |
| ----- | ------------------------------------------------------------------ | ----- | ----------- |
| key   | [string](#string)                                                  |       |             |
| value | [ExecutionsStatesOfWorkload](#ank_base-ExecutionsStatesOfWorkload) |       |             |

### AddCondition

An enum type describing the expected workload state. Used for dependency management.

| Name               | Number | Description                                                    |
| ------------------ | ------ | -------------------------------------------------------------- |
| ADD_COND_RUNNING   | 0      | The workload is operational.                                   |
| ADD_COND_SUCCEEDED | 1      | The workload has successfully exited.                          |
| ADD_COND_FAILED    | 2      | The workload has exited with an error or could not be started. |

### AgentDisconnected

The exact state of the workload cannot be determined, e.g., because of a broken connection to the responsible agent.

| Name               | Number | Description |
| ------------------ | ------ | ----------- |
| AGENT_DISCONNECTED | 0      |             |

### Failed

The workload has failed or is in a degraded state.

| Name               | Number | Description                                                                                                                    |
| ------------------ | ------ | ------------------------------------------------------------------------------------------------------------------------------ |
| FAILED_EXEC_FAILED | 0      | The workload has failed during operation                                                                                       |
| FAILED_UNKNOWN     | 1      | The workload is in an unsupported by Ankaios runtime state. The workload was possibly altered outside of Ankaios.              |
| FAILED_LOST        | 2      | The workload cannot be found anymore. The workload was possibly altered outside of Ankaios or was auto-removed by the runtime. |

### NotScheduled

The workload is not scheduled to run at any agent. This is signalized with an empty agent in the workload specification.

| Name          | Number | Description |
| ------------- | ------ | ----------- |
| NOT_SCHEDULED | 0      |             |

### Pending

The workload is going to be started eventually.

| Name                     | Number | Description                                                                    |
| ------------------------ | ------ | ------------------------------------------------------------------------------ |
| PENDING_INITIAL          | 0      | The workload specification has not yet being scheduled                         |
| PENDING_WAITING_TO_START | 1      | The start of the workload will be triggered once all its dependencies are met. |
| PENDING_STARTING         | 2      | Starting the workload was scheduled at the corresponding runtime.              |
| PENDING_STARTING_FAILED  | 8      | The starting of the workload by the runtime failed.                            |

### ReadWriteEnum

An enum type describing if reads and/or write actions are allowed.

| Name          | Number | Description          |
| ------------- | ------ | -------------------- |
| RW_NOTHING    | 0      | Allow nothing        |
| RW_READ       | 1      | Allow read           |
| RW_WRITE      | 2      | Allow write          |
| RW_READ_WRITE | 5      | Allow read and write |

### Removed

The workload was removed from Ankaios. This state is used only internally in Ankaios. The outside world removed states are just not there.

| Name    | Number | Description |
| ------- | ------ | ----------- |
| REMOVED | 0      |             |

### RestartPolicy

An enum type describing the restart behavior of a workload.

| Name       | Number | Description                                                                               |
| ---------- | ------ | ----------------------------------------------------------------------------------------- |
| NEVER      | 0      | The workload is never restarted. Once the workload exits, it remains in the exited state. |
| ON_FAILURE | 1      | If the workload exits with a non-zero exit code, it will be restarted.                    |
| ALWAYS     | 2      | The workload is restarted upon termination, regardless of the exit code.                  |

### Running

The workload is operational.

| Name       | Number | Description                  |
| ---------- | ------ | ---------------------------- |
| RUNNING_OK | 0      | The workload is operational. |

### Stopping

The workload is scheduled for stopping.

| Name                          | Number | Description                                                                                                                                 |
| ----------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| STOPPING                      | 0      | The workload is being stopped.                                                                                                              |
| STOPPING_WAITING_TO_STOP      | 1      | The deletion of the workload will be triggered once neither 'pending' nor 'running' workload depending on it exists.                        |
| STOPPING_REQUESTED_AT_RUNTIME | 2      | This is an Ankaios generated state returned when the stopping was explicitly triggered by the user and the request was sent to the runtime. |
| STOPPING_DELETE_FAILED        | 8      | The deletion of the workload by the runtime failed.                                                                                         |

### Succeeded

The workload has successfully finished operation.

| Name         | Number | Description                                       |
| ------------ | ------ | ------------------------------------------------- |
| SUCCEEDED_OK | 0      | The workload has successfully finished operation. |

## Scalar Value Types

| .proto Type | Notes                                                                                                                                           | C++    | Java       | Python      | Go      | C#         | PHP            | Ruby                           |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------ | ---------- | ----------- | ------- | ---------- | -------------- | ------------------------------ |
| double      |                                                                                                                                                 | double | double     | float       | float64 | double     | float          | Float                          |
| float       |                                                                                                                                                 | float  | float      | float       | float32 | float      | float          | Float                          |
| int32       | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint32 instead. | int32  | int        | int         | int32   | int        | integer        | Bignum or Fixnum (as required) |
| int64       | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint64 instead. | int64  | long       | int/long    | int64   | long       | integer/string | Bignum                         |
| uint32      | Uses variable-length encoding.                                                                                                                  | uint32 | int        | int/long    | uint32  | uint       | integer        | Bignum or Fixnum (as required) |
| uint64      | Uses variable-length encoding.                                                                                                                  | uint64 | long       | int/long    | uint64  | ulong      | integer/string | Bignum or Fixnum (as required) |
| sint32      | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int32s.                            | int32  | int        | int         | int32   | int        | integer        | Bignum or Fixnum (as required) |
| sint64      | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int64s.                            | int64  | long       | int/long    | int64   | long       | integer/string | Bignum                         |
| fixed32     | Always four bytes. More efficient than uint32 if values are often greater than 2^28.                                                            | uint32 | int        | int         | uint32  | uint       | integer        | Bignum or Fixnum (as required) |
| fixed64     | Always eight bytes. More efficient than uint64 if values are often greater than 2^56.                                                           | uint64 | long       | int/long    | uint64  | ulong      | integer/string | Bignum                         |
| sfixed32    | Always four bytes.                                                                                                                              | int32  | int        | int         | int32   | int        | integer        | Bignum or Fixnum (as required) |
| sfixed64    | Always eight bytes.                                                                                                                             | int64  | long       | int/long    | int64   | long       | integer/string | Bignum                         |
| bool        |                                                                                                                                                 | bool   | boolean    | boolean     | bool    | bool       | boolean        | TrueClass/FalseClass           |
| string      | A string must always contain UTF-8 encoded or 7-bit ASCII text.                                                                                 | string | String     | str/unicode | string  | string     | string         | String (UTF-8)                 |
| bytes       | May contain any arbitrary sequence of bytes.                                                                                                    | string | ByteString | str         | []byte  | ByteString | string         | String (ASCII-8BIT)            |
