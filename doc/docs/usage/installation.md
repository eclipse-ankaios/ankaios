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

## Installation methods

There a different ways to install Ankaios.

### Setup with script

The recommend way to install Ankaios is using the installation script.
To install the latest pre-built version of Ankaios into the default installation path `/usr/local/bin`, please run the following command:

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -
```

The installation process automatically detects the platform and downloads the appropriate binaries.

Supported platforms: `linux/amd64`, `linux/arm64`

!!! note

    The script requires root privileges to install the pre-built binaries into the default installation path `/usr/local/bin`. You can set a custom installation path if only non-root privileges are available.

The following table shows the optional arguments that can be passed to the script:

| Supported parameters | Description |
| --- | --- |
| -v <version\> | e.g. `v0.1.0`, default: latest version |
| -i <install-path\> | File path where Ankaios will be installed, default: `/usr/local/bin` |

To install a specific version run the following command and substitute `<version>` with a specific version tag e.g. `v0.1.0`:

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/download/<version>/install.sh | bash -s -- -v <version>
```

For available versions see the [list of releases](https://github.com/eclipse-ankaios/ankaios/tags).

### Manual download of binaries

As an alternative to the installation script, the pre-built binaries can be downloaded manually from the Ankaios repository [here](https://github.com/eclipse-ankaios/ankaios/releases).
This is useful if the automatic detection of the platform is failing in case of `uname` system command is not allowed or supported on the target.

### Build from source

For building Ankaios from source see [Build](../development/build.md).
