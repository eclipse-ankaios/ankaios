# API library - SW Design

## About this document

This document describes the Software Design for the API library of Ankaios.

Ankaios is a workload orchestrator supporting a subset of the Kubernetes configurations and is targeted at the automotive use case.

The API library provides the definition of the external interface of Ankaios - the Control Interface.
The data structures defined here are used by Ankaios and the managed workloads to communicate.

## Context View

The following diagram shows a high level overview of the API library and its context:

![Context View](drawio/context_view.drawio.svg)

## Constraints, risks and decisions

### Design decisions

## Structural view

The API library provides two Protobuf files : one containing the base definition of the data structures used by Ankaios for the Control Interface and the one that defines the Control Interface itself:

![Context View](drawio/unit_overview.drawio.svg)

### Ank Base

#### Ank Base provides object definitions
`swdd~ank-base-provides-object-definitions~1`

Status: approved

The ank_base.proto file provides the definitions of the main Ankaios objects.

Rationale:
The Protobuf file is separated from the Control Interface to enable reusing it for implementations of the Communication Middleware.

Comment:
Ank Base objects are used to serialize YAML/JSON data for the output to the user, as the Ank Base objects allow empty fields required for filtering.

Tags:
AnkBase

Needs:
- impl

#### Agent supports restart policies
`swdd~api-supports-restart-policies~1`

Status: approved

Ankaios shall support the following restart policies for a workload:

* `NEVER`: The workload is never restarted. Once the container exits, it remains in the exited state.
* `ON_FAILURE`: If the workload exits with a non-zero exit code, it will be restarted.
* `ALWAYS`: The workload is restarted upon termination, regardless of the exit code.

Comment:
The default restart policy is `NEVER`.

Rationale:
In some cases, workloads must remain operational at all times, even if they fail or exit successfully.

Tags:
- AnkBase

Needs:
- impl

#### Ankaios workload execution state additional information
`swdd~api-workload-state-additional-information~1`

Status: approved

Ankaios shall provide a string with additional information for the workload execution state.

Rationale:
The additional information could be provided by the runtime and is helpful for debugging purposes.

Tags:
- AnkBase

Needs:
- impl
- utest

#### Ankaios workload execution state identification
`swdd~api-workload-state-identification~1`

Status: approved

Ankaios shall support workload execution state identification by the combination of:

- assigned agent name
- workload name
- runtime config hash

Rationale:
The workload name is not sufficient to uniquely identify a workload as it can be moved from one
agent to another or could get updated, which changes it's config hash.

Tags:
- AnkBase

Needs:
- impl

#### Workload add conditions for dependencies
`swdd~api-add-conditions-for-dependencies~1`

Status: approved

Ankaios shall support the following add conditions for a workload dependency:
* `running` - the workload is operational
* `succeeded` - the workload has successfully exited
* `failed` - the workload has exited with an error or could not be started

Rationale:
Some workloads may need another service to be running before they can be started, others may need preparatory tasks which have been successfully finished. Dependencies on failure of workloads allows the execution of mitigation or recording actions.

Tags:
- AnkBase

Needs:
- impl
- utest

#### Ankaios supported workload states
`swdd~api-workload-states-supported-states~1`

Status: approved

Ankaios shall support the following execution states with substates for a workload:

- agent disconnected
- pending
    * initial
    * starting
    * waiting to start
    * starting failed
- running
    * ok
- stopping
    * waiting to stop
    * stopping
    * requested at runtime
    * delete failed
- succeeded
    * ok
- failed
    * exec failed
    * unknown
    * lost
- not scheduled
- removed

Tags:
- AnkBase

Needs:
- impl
- utest

The Following diagram shows all Ankaios workload states and the possible transitions between them:

![Workload states](drawio/state_workload_execution_all_states_simple.drawio.svg)

### Control API

#### Control API provides definitions for the Control Interface
`swdd~control-api-provides-control-interface-definitions~1`

Status: approved

The control_api.proto file provides the definitions of the Ankaios Control Interface.

Tags:
ControlAPI

Needs:
- impl

### Spec Objects

The Ankaios Spec objects are providing objects definitions that can be validated upon conversion and used internally without further validation.
The general idea is to automatically generate all of the required objects using the `spec_macros` crate. Additionally there are some exceptions where the objects or some conversions need to be written manually.

#### API provides spec object definitions
`swdd~api-provides-spec-object-definitions~1`

Status: approved

Ankaios shall provide Spec objects definitions that include the minimum required fields for the internal execution.

Comment:
In contrast to the Ank Base objects, the Spec objects do not allow empty fields for required values.
As they are generated via macros, they do not require additional tests as the macros are already validated and verified.
The Spec objects are used to deserialize from YAML/JSON.

Tags:
- SpecObjects

Needs:
- impl

#### Provide conversions between Ank Base and Spec
`swdd~api-conversions-between-ank-base-and-spec~1`

Status: approved

Ankaios shall provide conversion functions in both directions between Ank Base and Spec objects.

Comment:
As the conversions in both directions between Spec and Ank Base are generated via macros, they do not require additional tests as the macros are already validated and verified.

Tags:
- AnkBase
- SpecObjects

Needs:
- impl

#### Workload state transitions
`swdd~api-workload-state-transitions~1`

status: approved

Upon transitioning from the state:
* `stopping` or
* `waiting_to_stop`

to:

* `running` or
* `succeeded` or
* `failed`

the workload execution state shall remain in `stopping` state.

Rationale:
This hysteresis is particularly necessary when the stopping operation is in progress, but the workload is still running and reports to be running. To prevent the state from flipping multiple times, the new value must depend on the old one and remain in the `stopping` state.

Tags:
- SpecObjects

Needs:
- impl
- utest

#### Workload states map allows managing workload execution states
`swdd~api-state-map-for-workload-execution-states~1`

Status: approved

The WorkloadStatesMap shall represents the current execution states of the managed by Ankaios workloads by providing performant management of the states via the following functionalities:
* getting all workload states for an agent
* getting the workload state of a workload
* getting all workload states except the ones for a specific agent
* marking all states of an agent as agent disconnected
* adding an initial state for a list of workloads
* adding new states to the map
* keeping the map clean by deleting the entries for removed workloads
* removing states from the map

Tags:
- SpecObjects

Needs:
- impl
- utest

#### Workload delete conditions for dependencies
`swdd~api-delete-conditions-for-dependencies~1`

Status: approved

Ankaios shall support the following delete conditions for a workload dependency:
* `running` - the workload is operational
* `not pending nor running` - the workload is not running nor it is going to be started soon

Rationale:
Delete conditions are needed to be able to stop a workload on which others depend and for the update strategy `at least once` when the workload is shifted from one agent to another.

Comment:
The `DeleteCondition` objects is currently not automatically generated, mut manually written as it is not required at the external interface.

Tags:
- SpecObjects

Needs:
- impl
- utest

#### Provide deterministic object serialization
`swdd~api-object-serialization~1`

Status: approved

Ankaios shall provide a sorted serialization of unordered data structures.

Rationale:
Associative arrays using hash tables are typically used for fast access, but the data is stored unordered.
To provide a consistent view to the user, such data types shall be serialized into an ordered output.

Tags:
- AnkBase
- SpecObjects

Needs:
- impl
- utest

#### Provide a method that checks if the workload requires control interface
`swdd~api-workload-needs-control-interface~1`

Status: approved

Ankaios shall provide functionality for checking if the creation of a Control Interface for a workload is required
by verifying that allow rules for the Control Interface access are defined in the workload specification.

Tags:
- SpecObjects

Needs:
- impl
- utest

#### Naming of Workload execution instances
`swdd~api-workload-execution-instance-naming~1`

Status: approved

Ankaios shall provide functionality for retrieving the Workload execution instance name of the workload in the following naming schema:

    <Workload name>.<runtime config hash>.<Agent name>

Where the hash of the workload runtime config is calculated from the complete runtime config string provided in the workload specification.

Rationale:
A unique, consistent and reproducible naming that allows detecting changes in the workload configuration is needed in order to check if a workload specification differs from the workload execution instance. Such a configuration drift could occur during windows in which an Ankaios Agent was unresponsive or down.

Tags:
- SpecObjects

Needs:
- impl
- utest

#### Control Interface convention for workload names in logs access rules
`swdd~api-access-rules-logs-workload-names-convention~1`

Status: approved

Ankaios shall provide functionality for enforcing the validity of workload names in Control Interface access `LogRule`s:
- to contain at most one wildcard symbol "*"
- to be able to match a workload following the naming convention (e.g. not being to long, only containing valid characters)

Rationale:
This shall prevent users from providing rules which will never match any workload.
Otherwise, invalid deny rules could lead to workloads having more rights than expected.

Tags:
- SpecObjects

Needs:
- impl
- utest

#### Control Interface access rules filter mask conventions
`swdd~api-access-rules-filter-mask-convention~1`

Status: approved

Ankaios shall provide functionality for enforcing a non-empty filter mask for Control Interface access `StateRule`s.

Rationale:
An empty filter mask for an allow access rules might be misunderstood and expected to "allow nothing".

Tags:
- SpecObjects

Needs:
- impl
- utest
- stest

#### Workload naming convention
`swdd~api-workload-naming-convention~1`

Status: approved

Ankaios shall provide functionality for enforcing a workload name to:
* contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_"
* have a minimal length of 1 character
* have a maximal length of 63 characters

Rationale:
A consistent naming manner assures stability in usage and compatibility with Ankaios internal structure by ensuring proper function of the filtering.

Tags:
- SpecObjects

Needs:
- impl
- utest
- stest

#### Agent naming convention
`swdd~api-agent-naming-convention~1`

Status: approved

Ankaios shall provide functionality for enforcing an agent name to contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_".

Comment:
Supporting an empty agent name in a workload configuration allows for scenarios where a workload is not scheduled on an Ankaios agent.

Rationale:
A consistent naming manner assures stability in usage and compatibility with Ankaios internal structure by ensuring proper function of the filtering.

Tags:
- SpecObjects

Needs:
- impl
- utest
- stest

#### Config item key naming convention
`swdd~api-config-item-key-naming-convention~1`

Status: approved

Ankaios shall provide functionality for enforcing a config item key to contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_".

Rationale:
A consistent naming manner assures stability in usage and compatibility with Ankaios internal structure by ensuring proper function of the filtering.

Tags:
- SpecObjects

Needs:
- impl
- utest
- stest

#### Config aliases and referenced config keys naming convention
`swdd~api-config-aliases-and-config-reference-keys-naming-convention~1`

Status: approved

Ankaios shall provide functionality for enforcing a workload's config reference key value pairs to contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_".

Rationale:
A consistent naming manner assures stability in usage and compatibility with Ankaios internal structure by ensuring proper function of the filtering.

Tags:
- SpecObjects

Needs:
- impl
- utest
- stest

#### API version checks
`swdd~api-version-checks~1`

Status: approved

Ankaios shall provide functions to check the API version of a received `desiredState`.

Comment:
`desiredState`s can be received as manifests or at the Control Interface.
More then one version can be supported at the same time, where warning log messages are written in case a deprecated version is used.

Tags:
- SpecObjects

Needs:
- impl
- utest

## Data view

## Error management view

## Physical view

## References

## Glossary

* Protobuf - [Protocol Buffers](https://protobuf.dev/)

<!-- markdownlint-disable-file MD004 MD022 MD032 -->
