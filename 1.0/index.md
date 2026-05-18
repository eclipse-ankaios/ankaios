Eclipse Ankaios is a workload orchestrator purpose-built for embedded and automotive platforms. Designed to meet the unique demands of resource-constrained environments and High-Performance Computing (HPC) systems in vehicles, Ankaios delivers reliable workload management where it matters most.

[Get Started →](https://eclipse-ankaios.github.io/ankaios/1.0/usage/quickstart/index.md) [View on GitHub](https://github.com/eclipse-ankaios/ankaios)

### 🚀 Flexible runtime support

Ankaios supports both Podman and containerd runtimes, giving you the freedom to choose the container technology that best fits your architecture and security requirements. Whether managing a single ECU or orchestrating across multiple compute domains, Ankaios provides a unified API to start, stop, configure, and update workloads deployed as containers.

### 📈 Horizontal scalability

Built on a server-agent architecture, Ankaios offers a central place to manage automotive applications across your entire system. The setup consists of one server and multiple agents — typically one agent per node — with each agent connecting to one or more runtimes that execute your workloads. This design ensures scalability from simple single-node deployments to complex distributed systems.

### 💡 Why choose Ankaios?

Built specifically for embedded and automotive use cases, Ankaios understands the constraints you face: limited resources, real-time requirements, safety considerations, and complex system integration. Unlike existing orchestrators, Ankaios is optimized for deterministic behavior, minimal overhead, and seamless integration with automotive-grade platforms for environments where reliability and efficiency are non-negotiable.

## Key features

📋

### Declarative configuration

Define your entire system state in a single manifest. Ankaios ensures your workloads match your desired configuration, automatically reconciling any drift. Update, configure and rollback your applications with simple manifest changes.

🔄

### Multi-runtime flexibility

Native support for Podman and containerd gives you runtime choice and other runtimes can also be easily added. Mix runtimes on the same node or run different runtimes on different nodes based on your specific requirements.

🚗

### Built for automotive constraints

Optimized for deterministic behavior and minimal resource overhead. Ankaios respects the requirements and constraints of automotive platforms while providing modern container orchestration capabilities.

🌐

### Distributed by design

The server-agent architecture scales from single-node deployments to complex multi-domain systems. Manage workloads across ECUs, HPCs, and edge devices from a central control point with consistent APIs.

⚡

### Dynamic workload management

Start, stop, update, and monitor containerized workloads in real-time. Ankaios handles dependencies, ensures proper startup sequences, and provides visibility into workload health across your entire system.

💻

### Programmable orchestration

Native SDKs allow workloads to communicate with Ankaios programmatically. Applications can query the system state, trigger workload updates, and react to orchestration events, creating intelligent systems that adapt to their deployment environment.

## Getting started

**📦 Installation**\
Get Ankaios up and running on your system\
[Installation guide →](https://eclipse-ankaios.github.io/ankaios/1.0/usage/installation/index.md)

**🚀 Quick start**\
Deploy your first workload in minutes\
[Quick start tutorial →](https://eclipse-ankaios.github.io/ankaios/1.0/usage/quickstart/index.md)

**🏗️ Architecture**\
Understand how Ankaios works under the hood\
[Architecture overview →](https://eclipse-ankaios.github.io/ankaios/1.0/architecture/index.md)

**📡 Vehicle signals**\
Send and receive vehicle signals with workloads\
[Vehicle signals tutorial →](https://eclipse-ankaios.github.io/ankaios/1.0/usage/tutorial-vehicle-signals/index.md)

**☁️ Fleet management**\
Manage vehicle fleets from the cloud\
[Fleet management tutorial →](https://eclipse-ankaios.github.io/ankaios/1.0/usage/tutorial-fleet-management/index.md)

**📚 API reference**\
Explore the complete API documentation\
[API reference →](https://eclipse-ankaios.github.io/ankaios/1.0/reference/control-interface/index.md)

## Community & Resources

[▶ Eclipse Ankaios playlist](https://youtube.com/playlist?list=PLXGqib0ZinZFwXpqN9pdFBrtflJVZ--_p)

[⭐ Awesome Ankaios](https://eclipse-ankaios.github.io/ankaios/1.0/usage/awesome-ankaios/index.md)

[💬 Get support](https://eclipse-ankaios.github.io/ankaios/1.0/support/index.md)

[🔧 Contributing guide](https://eclipse-ankaios.github.io/ankaios/1.0/development/build/index.md)
