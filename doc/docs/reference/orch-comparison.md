# Comparing Container Orchestrators

This page presents a comparison between Eclipse Ankaios and the Lightweight Kubernetes distribution K3s. All benchmarks were run on identical virtual machines in a controlled environment.

## Test Environment

### Host Machine

| Property     | Value                                                  |
|--------------|--------------------------------------------------------|
| **CPU**      | Intel Core i7-11850H @ 2.50 GHz (8 cores / 16 threads) |
| **RAM**      | 16 GiB                                                 |
| **OS**       | Ubuntu 24.04 via WSL2 (Windows)                        |

Virtual machines are managed with [Incus](https://linuxcontainers.org/incus/) on the host.

### Virtual Machine Configuration

| Property     | Value                |
|--------------|----------------------|
| **VM image** | Ubuntu 24.04 (noble) |
| **vCPUs**    | 4                    |
| **RAM**      | 4 GiB                |
| **Disk**     | 20 GiB               |

One VM was used per orchestrator, reset to a clean snapshot before each measurement. No workloads were running during idle measurements. Benchmark images were pre-pulled into the snapshot so that no registry pulls occur during runs and the measurements are not influenced by network latency or bandwidth fluctuations.

### Orchestrators used

| Property      | Eclipse Ankaios    | K3s                  |
|---------------|--------------------|----------------------|
| **Version**   | v1.0.0             | v1.35.4+k3s1         |
| **Developer** | Eclipse Foundation | Rancher (SUSE)       |
| **Runtime**   | Podman             | containerd (bundled) |

---

## Feature Comparison

| Dimension                     | Ankaios                            | K3s            |
|-------------------------------|------------------------------------|----------------|
| **Focus**                     | Automotive / embedded (SDV)        | Edge / IoT     |
| **Language**                  | Rust                               | Go             |
| **Workload types**            | OCI containers (multiple runtimes) | OCI containers |
| **Multi-node orchestrator**   | ✔️                                 | ✔️             |
| **Reconciliation loop**       | ✔️                                 | ✔️             |
| **API access for workloads**  | ✔️                                 | ✔️             |
| **Supports dependencies**     | ✔️                                 | ❌             |
| **Flash wear-out prevention** | ✔️                                 | ❌             |
| **Requirement tracing**       | ✔️                                 | ❌             |

---

## Benchmark Results

Each benchmark was run 100 times per orchestrator; results below are averages. Memory figures use PSS (Proportional Set Size) from `/proc/<pid>/smaps_rollup` rather than RSS to avoid double-counting shared pages.

---

### Startup Time

Startup time measures how long each orchestrator takes from process launch until its management API is responsive.

<img src="../../assets/comp-startup-time.png" width="700" alt="Startup time" />

---

### Resource usage in idle mode

Resource usage in idle mode is measured after each orchestrator has started and stabilized, with no workloads running.

<img src="../../assets/comp-idle-resources.png" width="700" alt="Idle resource usage" />

---

### Workload Deployment Time

This scenario submits an nginx manifest and measures the time until the first HTTP 200 response, the true end-to-end deployment time. The sampling time is set to 100 ms.

<img src="../../assets/comp-deploy-nginx.png" width="700" alt="Deploy nginx to HTTP 200" />

---

### Rolling Update Time

Rolling update time measures from manifest re-submission until the updated workload is serving traffic.

<img src="../../assets/comp-rolling-update.png" width="700" alt="Rolling update time" />
