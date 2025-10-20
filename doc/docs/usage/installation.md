# Installation

Ankaios has been tested with the following Linux distributions. Others might
work as well but have not been tested.

* Ubuntu 24.04 LTS
* Ubuntu 22.04 LTS

## System requirements

Ankaios currently requires a Linux OS and is available for x86_64 and arm64
targets.

The minimum system requirements are (tested with [EB corbos Linux – built on Ubuntu](https://www.elektrobit.com/products/ecu/eb-corbos/linux/)):

| Resource | Min     |
| -------- | ------- |
| CPU      | 1 core  |
| RAM      | 256 MB  |

## Container runtime

Ankaios supports multiple container runtimes. Depending on which runtime is to be used, only certain container runtimes or all supported runtimes can be installed using the following instructions.

### Podman

For using the Ankaios `podman` and `podman-kube` runtimes,
[Podman](https://podman.io) needs to be installed as this is used as
container runtime
(see [Podman installation instructions](https://podman.io/docs/installation)).
For using the `podman` runtime, Podman version 3.4.2 is sufficient but the
`podman-kube` runtime requires at least Podman version 4.3.1.

### Containerd

For using the Ankaios `containerd` runtime, follow the [containerd installation instructions](https://github.com/containerd/containerd/blob/main/docs/getting-started.md#installing-containerd) to install the containerd daemon.

Ankaios uses the `nerdctl` command-line interface (CLI) to manage containers with the containerd runtime. Install a compatible version of the `nerdctl` CLI for the containerd runtime, or install the full `nerdctl` package, including dependencies such as containerd, runc, and CNI. Note that if you are not using the version distributed by your package manager, you must check the platform compatibility of containerd. Download and install the `nerdctl` package from the [official nerdctl releases](https://github.com/containerd/nerdctl/releases).

## Installation methods

There are two ways to install Ankaios, depending on your specific needs and focus. If you are new to Ankaios or TLS is not a top priority, we recommend following the setup instructions in [Setup with script](#setup-with-script) without enabling mutual transport layer security (mTLS) for communication. On the other hand, if you want to setup Ankaios in a production environment, follow the setup instructions in [Setting up Ankaios with mTLS](mtls-setup.md).

### Setup with script

The recommended way to install Ankaios is using the installation script.
To install the latest release version of Ankaios, please run the following command:

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -
```

!!! note

    Please note that installing the latest version of Ankaios in an automated workflow is discouraged.
    If you want to install Ankaios during an automated workflow, please install a specific version as described below.

The installation process automatically detects the platform and downloads the appropriate binaries.
The installation path for the binaries is `/usr/local/bin`.
The installation also creates systemd unit files and an uninstall script.

Supported platforms: `linux/amd64`, `linux/arm64`

!!! note

    The script requires root privileges to install the pre-built binaries into
    the installation path `/usr/local/bin` and also for systemd
    integration. You can disable systemd unit
    file generation if required.

The following table shows the optional arguments that can be passed to the script:

| Supported parameters | Description |
| --- | --- |
| -v <version\> | e.g. `v0.1.0`, default: latest version |
| -t <install-type\> | Installation type for systemd integration: `server`, `agent`, `none` or `both` (default) |

To install a specific version run the following command and substitute `<version>` with a specific version tag e.g. `v0.1.0`:

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/<version>/install.sh | bash -s -- -v <version>
```

For available versions see the [list of releases](https://github.com/eclipse-ankaios/ankaios/tags).

### Set the log level for `ank-server` and `ank-agent` services

To configure the log levels for `ank-server` and `ank-agent` during the installation process using the provided environment variables, follow these steps:

1. Set the desired log levels for each service by assigning valid values to the environment variables `INSTALL_ANK_SERVER_RUST_LOG` and `INSTALL_ANK_AGENT_RUST_LOG`. For the syntax see the [documentation for `RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging).

2. Run the installation script, making sure to pass these environment variables as arguments if needed:

    For a specific version:

    ```shell
    curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/<version>/install.sh | INSTALL_ANK_SERVER_RUST_LOG=debug INSTALL_ANK_AGENT_RUST_LOG=info bash -s -- -t both -v <version>
    ```

    For the latest version:

    ```shell
    curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/latest/install.sh | INSTALL_ANK_SERVER_RUST_LOG=debug INSTALL_ANK_AGENT_RUST_LOG=info bash -s -- -t both
    ```

Now, both services will output logs according to the specified log levels. If no explicit value was provided during installation, both services will default to `info` log level. You can always change the log level by updating the environment variables and reinstalling the services.

### Uninstall Ankaios

If Ankaios has been installed with the installation script, it can be uninstalled with:

```shell
ank-uninstall.sh
```

The folder `/etc/ankaios` will remain.

### Manual download of binaries

As an alternative to the installation script, the pre-built binaries can be downloaded manually from the Ankaios repository [here](https://github.com/eclipse-ankaios/ankaios/releases).
This is useful if the automatic detection of the platform is failing in case of `uname` system command is not allowed or supported on the target.

### Build from source

For building Ankaios from source see [Build](../development/build.md).
