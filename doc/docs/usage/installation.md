# Installation

Ankaios has been tested with the following Linux distributions. Others might
work as well but have not been tested.

* Ubuntu 23.04
* Ubuntu 22.04 LTS
* Ubuntu 20.04 LTS

## Pre-requisites

Ankaios currently requires a Linux OS and is available for x86_64 and arm64
targets. [Podman](https://podman.io) needs to be installed as this is used as 
container runtime
(see [Podman installation instructions](https://podman.io/docs/installation)).
For using the `podman` runtime, Podman version 3.4.2 is sufficient but the
`podman-kube` runtime requires at least Podman version 4.3.1.

## Installation methods

There a different ways to install Ankaios.

### Setup with script

The recommended way to install Ankaios is using the installation script.
To install the latest release version of Ankaios, please run the following command:

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -
```

The installation process automatically detects the platform and downloads the appropriate binaries.
The default installation path for the binaries is `/usr/local/bin` but can be changed.
The installation also creates systemd unit files and an uninstall script.

Supported platforms: `linux/amd64`, `linux/arm64`

!!! note

    The script requires root privileges to install the pre-built binaries into
    the default installation path `/usr/local/bin` and also for systemd
    integration. You can set a custom installation path and disable systemd unit
    file generation if only non-root privileges are available.

The following table shows the optional arguments that can be passed to the script:

| Supported parameters | Description |
| --- | --- |
| -v <version\> | e.g. `v0.1.0`, default: latest version |
| -i <install-path\> | File path where Ankaios will be installed, default: `/usr/local/bin` |
| -t <install-type\> | Installation type for systemd integration: `server`, `agent`, `none` or `both` (default) |
| -s <server-options\> | Options which will be passed to the Ankaios server. Default `--startup-config /etc/ankaios/state.yaml` |
| -a <agent-options\> | Options which will be passed to the Ankaios agent. Default `--name agent_A` |

To install a specific version run the following command and substitute `<version>` with a specific version tag e.g. `v0.1.0`:

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/<version>/install.sh | bash -s -- -v <version>
```

For available versions see the [list of releases](https://github.com/eclipse-ankaios/ankaios/tags).

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
