---
title: Welcome
description: Eclipse Ankaios is a workload and container orchestrator purpose-built for embedded and automotive platforms.
template: home.html
---

![Ankaios logo](assets/Ankaios__logo_for_light_bgrd_clipped.png#only-light)
![Ankaios logo](assets/Ankaios__logo_for_dark_bgrd_clipped.png#only-dark)

# Eclipse Ankaios

Eclipse Ankaios is a workload orchestrator purpose-built for embedded and automotive platforms.
Designed to meet the unique demands of resource-constrained environments and High-Performance Computing (HPC) systems in vehicles, Ankaios delivers reliable workload management where it matters most.

<!-- markdownlint-disable MD001 -->
### Flexible Runtime Support

Ankaios supports both Podman and containerd runtimes, giving you the freedom to choose the container technology that best fits your architecture and security requirements. Whether managing a single ECU or orchestrating across multiple compute domains, Ankaios provides a unified API to start, stop, configure, and update workloads deployed as containers.

Built on a server-agent architecture, Ankaios offers a central place to manage automotive applications across your entire system. The setup consists of one server and multiple agents — typically one agent per node — with each agent connecting to one or more runtimes that execute your workloads. This design ensures scalability from simple single-node deployments to complex distributed systems.

### Why Choose Ankaios?

Built specifically for embedded and automotive use cases, Ankaios understands the constraints you face: limited resources, real-time requirements, safety considerations, and complex system integration. Unlike existing orchestrators, Ankaios is optimized for deterministic behavior, minimal overhead, and seamless integration with automotive-grade platforms for environments where reliability and efficiency are non-negotiable.

## Key features

<div class="grid cards" markdown>

* <span class="icon-wrapper">📋</span> __Declarative Configuration__

    ---

    Define your entire system state in a single manifest. Ankaios ensures your workloads match your desired configuration, automatically reconciling any drift. Update, configure and rollback your applications with simple manifest changes.

* <span class="icon-wrapper">🔄</span> __Multi-Runtime Flexibility__

    ---

    Native support for Podman and containerd gives you runtime choice. Switch between runtimes or run different runtimes on different nodes based on your specific requirements.

* <span class="icon-wrapper">🚗</span> __Built for Automotive Constraints__

    ---

    Optimized for deterministic behavior and minimal resource overhead. Ankaios respects the requirements and constraints of automotive platforms while providing modern container orchestration capabilities.

* <span class="icon-wrapper">🌐</span> __Distributed by Design__

    ---

    The server-agent architecture scales from single-node deployments to complex multi-domain systems. Manage workloads across ECUs, HPCs, and edge devices from a central control point with consistent APIs.

* <span class="icon-wrapper">⚡</span> __Dynamic Workload Management__

    ---

    Start, stop, update, and monitor containerized workloads in real-time. Ankaios handles dependencies, ensures proper startup sequences, and provides visibility into workload health across your entire system.

* <span class="icon-wrapper">💻</span> __Programmable Orchestration__

    ---

    Native SDKs allow workloads to communicate with Ankaios programmatically. Applications can query the system state, trigger workload updates, and react to orchestration events, creating intelligent systems that adapt to their deployment environment.

</div>

## Getting Started

* For first steps see [installation](usage/installation.md) and
[quickstart](usage/quickstart.md).
* An overview how Ankaios works is given on the [architecture](architecture.md) page.
* A tutorial [Sending and receiving vehicle signals](usage/tutorial-vehicle-signals.md) demonstrates the use of Ankaios with some workloads.
* The [Manage a fleet of vehicles from the cloud](usage/tutorial-fleet-management.md) tutorial shows how an Ankaios workload can access the Ankaios control interface in order to provide remote management capabilities.
* The API is described in the [reference](reference/control-interface.md) section.

## Community & Resources

* [:fontawesome-brands-youtube:{ .youtube } Eclipse Ankaios playlist](https://youtube.com/playlist?list=PLXGqib0ZinZFwXpqN9pdFBrtflJVZ--_p)
* See how others use Ankaios or provides extenstions on the [Awesome Ankaios](usage/awesome-ankaios.md) page.
* There are various ways to get [support](support.md).
* For contributions have a look at the [contributing](development/build.md) pages.

<!-- markdownlint-disable-file MD025 MD033 -->
