---
title: Welcome
description: Eclipse Ankaios is a workload and container orchestrator purpose-built for embedded and automotive platforms.
hide:
  - navigation
  - toc
  - navigation.title
  - navigation.tabs
  - navigation.sections
  - navigation.expand
  - navigation.path
  - toc.integrate
---

<!-- Hero Section with Full Background -->
<section class="hero-section">
  <div class="hero-background"></div>
  <div class="hero-overlay"></div>
  <div class="hero-content">
    <div class="hero-text">
      <div class="hero-logo-container">
        <img src="assets/Ankaios__logo_for_dark_bgrd_clipped.png" alt="Eclipse Ankaios" class="logo-dark" />
        <img src="assets/Ankaios__logo_for_light_bgrd_clipped.png" alt="Eclipse Ankaios" class="logo-light" />
      </div>
      <p class="hero-description">
        Eclipse Ankaios is a workload orchestrator purpose-built for embedded and automotive platforms.
        Designed to meet the unique demands of resource-constrained environments and High-Performance Computing (HPC) systems in vehicles, Ankaios delivers reliable workload management where it matters most.
      </p>
      <div class="hero-buttons">
        <a href="usage/quickstart/" class="hero-button hero-button-primary">Get Started →</a>
        <a href="https://github.com/eclipse-ankaios/ankaios" class="hero-button hero-button-secondary">View on GitHub</a>
      </div>
    </div>
  </div>
</section>

<!-- Main Content -->
<div class="main-content">

<!-- Feature Introduction -->
<div class="feature-intro">
<h3>🚀 Flexible runtime support</h3>
<p>Ankaios supports both Podman and containerd runtimes, giving you the freedom to choose the container technology that best fits your architecture and security requirements. Whether managing a single ECU or orchestrating across multiple compute domains, Ankaios provides a unified API to start, stop, configure, and update workloads deployed as containers.</p>
</div>

<div class="feature-intro">
<h3>📈 Horizontal scalability</h3>
<p>Built on a server-agent architecture, Ankaios offers a central place to manage automotive applications across your entire system. The setup consists of one server and multiple agents — typically one agent per node — with each agent connecting to one or more runtimes that execute your workloads. This design ensures scalability from simple single-node deployments to complex distributed systems.</p>
</div>

<div class="feature-intro">
<h3>💡 Why choose Ankaios?</h3>
<p>Built specifically for embedded and automotive use cases, Ankaios understands the constraints you face: limited resources, real-time requirements, safety considerations, and complex system integration. Unlike existing orchestrators, Ankaios is optimized for deterministic behavior, minimal overhead, and seamless integration with automotive-grade platforms for environments where reliability and efficiency are non-negotiable.</p>
</div>

<!-- Key Features -->
<section class="key-features-section">
  <h2>Key Features</h2>
  <div class="grid cards">
    <div class="feature-card">
      <div class="feature-icon">📋</div>
      <h3>Declarative configuration</h3>
      <p>Define your entire system state in a single manifest. Ankaios ensures your workloads match your desired configuration, automatically reconciling any drift. Update, configure and rollback your applications with simple manifest changes.</p>
    </div>

    <div class="feature-card">
      <div class="feature-icon">🔄</div>
      <h3>Multi-runtime flexibility</h3>
      <p>Native support for Podman and containerd gives you runtime choice and other runtimes can also be easily added. Mix runtimes on the same node or run different runtimes on different nodes based on your specific requirements.</p>
    </div>

    <div class="feature-card">
      <div class="feature-icon">🚗</div>
      <h3>Built for automotive constraints</h3>
      <p>Optimized for deterministic behavior and minimal resource overhead. Ankaios respects the requirements and constraints of automotive platforms while providing modern container orchestration capabilities.</p>
    </div>

    <div class="feature-card">
      <div class="feature-icon">🌐</div>
      <h3>Distributed by design</h3>
      <p>The server-agent architecture scales from single-node deployments to complex multi-domain systems. Manage workloads across ECUs, HPCs, and edge devices from a central control point with consistent APIs.</p>
    </div>

    <div class="feature-card">
      <div class="feature-icon">⚡</div>
      <h3>Dynamic workload management</h3>
      <p>Start, stop, update, and monitor containerized workloads in real-time. Ankaios handles dependencies, ensures proper startup sequences, and provides visibility into workload health across your entire system.</p>
    </div>

    <div class="feature-card">
      <div class="feature-icon">💻</div>
      <h3>Programmable orchestration</h3>
      <p>Native SDKs allow workloads to communicate with Ankaios programmatically. Applications can query the system state, trigger workload updates, and react to orchestration events, creating intelligent systems that adapt to their deployment environment.</p>
    </div>
  </div>
</section>

<!-- Getting Started -->
<section class="getting-started">
  <h2>Getting Started</h2>
  <div class="getting-started-grid">
    <div class="getting-started-item">
      <strong>📦 Installation</strong><br>
      Get Ankaios up and running on your system<br>
      <a href="usage/installation/">Installation Guide →</a>
    </div>
    <div class="getting-started-item">
      <strong>🚀 Quick Start</strong><br>
      Deploy your first workload in minutes<br>
      <a href="usage/quickstart/">Quick Start Tutorial →</a>
    </div>
    <div class="getting-started-item">
      <strong>🏗️ Architecture</strong><br>
      Understand how Ankaios works under the hood<br>
      <a href="architecture/">Architecture Overview →</a>
    </div>
    <div class="getting-started-item">
      <strong>📡 Vehicle Signals</strong><br>
      Send and receive vehicle signals with workloads<br>
      <a href="usage/tutorial-vehicle-signals/">Vehicle Signals Tutorial →</a>
    </div>
    <div class="getting-started-item">
      <strong>☁️ Fleet Management</strong><br>
      Manage vehicle fleets from the cloud<br>
      <a href="usage/tutorial-fleet-management/">Fleet Management Tutorial →</a>
    </div>
    <div class="getting-started-item">
      <strong>📚 API Reference</strong><br>
      Explore the complete API documentation<br>
      <a href="reference/control-interface/">API Reference →</a>
    </div>
  </div>
</section>

<!-- Community & Resources -->
<section class="community-section">
  <h2>Community & Resources</h2>
  <div class="community-grid">
    <div class="community-item">
      <a href="https://youtube.com/playlist?list=PLXGqib0ZinZFwXpqN9pdFBrtflJVZ--_p">
        <span class="youtube-icon">▶️</span> Eclipse Ankaios Playlist
      </a>
    </div>
    <div class="community-item">
      <a href="usage/awesome-ankaios/">
        ⭐ Awesome Ankaios
      </a>
    </div>
    <div class="community-item">
      <a href="support/">
        💬 Get Support
      </a>
    </div>
    <div class="community-item">
      <a href="development/build/">
        🔧 Contributing Guide
      </a>
    </div>
  </div>
</section>

</div>
