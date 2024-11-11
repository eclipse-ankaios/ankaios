---
title: Welcome
description: Eclipse Ankaios provides workload and container orchestration for automotive High Performance Computing Software (HPCs).
---

<picture style="padding-bottom: 1em;">
  <source media="(prefers-color-scheme: dark)" srcset="assets/Ankaios__logo_for_dark_bgrd_clipped.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/Ankaios__logo_for_light_bgrd_clipped.png">
  <img alt="Shows Ankaios logo" src="logo/Ankaios__logo_for_light_bgrd_clipped.png">
</picture>

# Eclipse Ankaios

<figure markdown>
  <a href="https://www.youtube.com/watch?v=GUaMxwh5jdU" target="_blank">
    ![Youtube introduction video](assets/youtube_ankaios_introduction.png)
  </a>
  <figcaption>Watch Eclipse Ankaios presentation at Eclipse SDV community day on July 6, 2023 on Youtube</figcaption>
</figure>

## Scope

Eclipse Ankaios provides workload and container orchestration for automotive
High Performance Computing (HPC) software. While it can be used for various
fields of applications, it is developed from scratch for automotive use cases
and provides a slim, yet powerful solution to manage containerized applications.
It supports various container runtimes with Podman as the first one, but other
container runtimes and even native applications can be supported. Eclipse
Ankaios is independent of existing communication frameworks like SOME/IP, DDS,
or REST API.

Eclipse Ankaios manages multiple nodes and virtual machines with a single unique
API in order to start, stop, configure, and update containers and workloads. It
provides a central place to manage automotive applications with a setup
consisting of one server and multiple agents. Usually one agent per node
connects to one or more runtimes that are running the workloads.

## Next steps

* For first steps see [installation](usage/installation.md) and
[quickstart](usage/quickstart.md).
* An overview how Ankaios works is given on the [architecture](architecture.md) page.
* A tutorial [Sending and receiving vehicle signals](usage/tutorial-vehicle-signals.md) demonstrates the use of Ankaios with some workloads.
* The [Manage a fleet of vehicles from the cloud](usage/tutorial-fleet-management.md) tutorial shows how an Ankaios workload can access the Ankaios control interface in order to provide remote management capabilities.
* The API is described in the [reference](reference/control-interface.md) section.
* For contributions have a look at the [contributing](development/build.md) pages.

## Background

Eclipse Ankaios follows the UNIX philosophy to have one tool for one job and do
that job well. It does not depend on a specific init system like systemd but can
be started with any init system. It also does not handle persistency but can use
 an existing automotive persistency handling, e.g. provided by AUTOSAR Adaptive.

The workloads are provided access to the Eclipse Ankaios API using access
control and thus are able to dynamically reconfigure the system. One possible
use case is the dynamic startup of an application that is only required in a
particular situation such as a parking assistant. When the driver wants to park
the car, a control workload can start the parking assistant application. When
the parking is finished, the parking assistant workload is stopped again.

Eclipse Ankaios also provides a CLI that allows developers to develop and test
configurations. In order to gain compatibility with Kubernetes, Eclipse Ankaios
accepts pod specifications.

An optional fleet connector can use the Eclipse Ankaios API to connect to a cloud-based
software update system, which allows an OEM to manage a fleet of vehicles and
provide new states to Eclipse Ankaios in order to update single or all
applications.

In order to support the Automotive SPICE process, Eclipse Ankaios comes with
requirements tracing supported by
[OpenFastTrace](https://github.com/itsallcode/openfasttrace).

<!-- markdownlint-disable-file MD025 MD033 -->
