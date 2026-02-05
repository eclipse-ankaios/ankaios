# Common library - SW Design

## About this document

This document describes the Software Design for the Common library of Ankaios.

Ankaios is a workload orchestrator supporting a subset of the Kubernetes configurations and is targeted at the automotive use case.

The Common library is a collection of units used in other components.
The goal is to avoid code duplication especially when it is about definitions of interfaces and basic structures (don't repeat yourself).

## Context View

The following diagram shows a high level overview of the Common library and its context:

![Context View](drawio/context_view.drawio.svg)

The diagram does not show all dependencies between the Common library and other components of Ankaios as anybody can use the Common library.
On the other hand the Common library is not allowed to use other component of Ankaios.

## Constraints, risks and decisions

### Design decisions

#### The Common library dependencies
`swdd~common-library-dependencies~1`

The Common library shall not use any other component of Ankaios.

Rationale: Other components are allowed to use the Common library.
Allowing dependencies in other direction would cause a cyclic dependency.

## Structural view

The Common library is a collection of independent units (structures, interfaces) used by other components of Ankaios.
For this reason it is useless to draw a structural diagram for this library.

### FromServerChannel

Simplifies sending and receiving `FromServer` messages. Internally uses a multi-producer, single-consumer channel from Tokio.

#### Provide `FromServerChannel`
`swdd~from-server-channel~1`

Status: approved

The Common library shall provide an asynchronous communication channel that supports sending and receiving the `FromServer` message.

Rationale: The communication channels are especially needed in order to abstract the Communication Middleware.

Tags:
- FromServerChannel

Needs:
- impl
- utest

### ToServerChannel

Simplifies sending and receiving `ToServer` messages. Internally uses a multi-producer, single-consumer channel from Tokio.

#### Provide `ToServerChannel`
`swdd~to-server-channel~1`

Status: approved

The Common library shall provide an asynchronous communication channel that supports sending and receiving the `ToServer` message.

Rationale: The communication channels are especially needed in order to abstract the Communication Middleware.

Tags:
- ToServerChannel

Needs:
- impl
- utest

### Objects

Definitions of objects which are needed in all other components of Ankaios.
These objects especially include objects which needs to be sent through for the `FromServerChannel` and `ToServerChannel`.

#### WorkloadStatesMap

The WorkloadStatesMap is a container that holds workload execution states and allows searching through them in an efficient way.

#### AgentMap

The AgentMap is an associative data structure that stores the names of the agents connected to the server as keys and the corresponding agent attributes as values.

### Common interface definitions

This includes definition of interfaces, which are used in other libraries and executables of Ankaios.

#### Provide common interface definitions
`swdd~common-interface-definitions~1`

Status: approved

The Common library shall provide interface used by Ankaios' libraries and executables.

Rationale: This prevents code duplication in accordance to the DRY principle.

Tags:
- CommonInterfaces

Needs:
- impl

### Common Helpers

Different helper methods used by other components of Ankaios. For example regarding error handling or testing.

#### Provide common helper methods
`swdd~common-helper-methods~1`

Status: approved

The Common library shall provide helper methods used by Ankaios' libraries and executables.

Rationale: This prevents code duplication in accordance to the DRY principle.

Tags:
- CommonHelpers

Needs:
- impl

#### Provide common version checking functionality
`swdd~common-version-checking~2`

Status: approved

The Common library shall provide a common release version checking functionality that fails if a provided version differs from the current major one.

Tags:
- CommonHelpers

Needs:
- impl
- utest

#### Provide common config handling
`swdd~common-config-handling~1`

Status: approved

The Common library shall provide a common function for handling config files that supports user-provided and default paths with fallback to default configuration values.

Rationale: This ensures a unified and consistent config handling across Ankaios components.

Tags:
- CommonHelpers

Needs:
- impl
- utest

### State manipulation

Provides methods for accessing or updating parts of objects, as used by field masks.

#### State manipulation uses period separated paths
`swdd~common-state-manipulation-path~1`

Status: approved

The state manipulation methods of the Common library shall use paths separated by the '.' symbols.

Tags:
- CommonStateManipulation

Needs:
- impl
- utest

#### State manipulation allows to set values
`swdd~common-state-manipulation-set~1`

Status: approved

The Common library shall provide a method to set the value of an object at a certain path.

Tags:
- CommonStateManipulation

Needs:
- impl
- utest

#### State manipulation set operation adds missing objects
`swdd~common-state-manipulation-set-add-missing-objects~1`

Status: approved

When setting the value of an object at a certain path, the Common library shall add missing intermediate objects as empty objects followed by the newly added value.

Tags:
- CommonStateManipulation

Needs:
- impl
- utest

#### State manipulation allows to remove values
`swdd~common-state-manipulation-remove~1`

Status: approved

The Common library shall provide a method to remove the value of an object at a certain path.

Tags:
- CommonStateManipulation

Needs:
- impl
- utest

#### State manipulation allows to get values
`swdd~common-state-manipulation-get~1`

Status: approved

The Common library shall provide a method to get the value of an object at a certain path.

Tags:
- CommonStateManipulation

Needs:
- impl
- utest

#### State manipulation provides functionality to expand wildcards
`swdd~common-state-manipulation-expand-wildcards~1`

Status: approved

The Common library shall provide a method for expanding paths containing wildcards ('*') as segments, using an existing object.

Comment:
The result is a list of all paths valid for the given object, resulting from replacing wildcard segments with any possible segment.

Tags:
- CommonStateManipulation

Needs:
- impl
- utest

## Data view

## Error management view

## Physical view

## References

## Glossary

<!-- markdownlint-disable-file MD004 MD022 MD032 -->
