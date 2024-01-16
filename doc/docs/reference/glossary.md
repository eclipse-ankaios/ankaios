# Glossary

This glossary is intended to be a comprehensive, uniform list of Ankaios terminology. It consists of technical terms specific to Ankaios, as well as more general terms that provide useful context.

## Node

A machine, either physical or virtual, that provides the necessary prerequisites (e.g. OS) to run an Ankaios server and/or agent.

## Runtime

The base an which a workload can be started. For OCI container this is a container runtime or engine. For native applications the runtime is the OS itself.

## Workload

A functionality that the Ankaios orchestrator can manage (e.g. start, stop). A workload could be packed inside an OCI [container](#container) (e.g. [Podman container](#podman-container)) or could also be just a native program ([native workload](#native-workload)). Ankaios is build to be extensible for different workload types by adding support for other [runtimes](#runtime).

## Container

A container is a lightweight, standalone, executable software package that includes everything needed to run an application, including the binaries, runtime, system libraries and dependencies. Containers provide a consistent and isolated environment for applications to run, ensuring that they behave consistently across different computing environments, from development to testing to production.

## Podman container

A Podman container refers to a [container](#container) managed by [Podman](https://docs.podman.io/en/latest/), which is an open-source container engine similar to Docker. [Podman](https://docs.podman.io/en/latest/) aims to provide a simple and secure container management solution for developers and system administrators.

## Native workload

An application developed specifically for a particular platform or operating system (OS). It is designed to run directly on the target platform without the need for bringing in any additional translation or emulation layers.
