# Build

## Dev container

The repo provides a Visual Studio Code [dev container](https://code.visualstudio.com/docs/devcontainers/containers) which includes all necessary tools to build all components and the documentation, but it does not provide the tools to run Ankaios as it's not the target platform. In case you want to extend the dev container see [extending the dev container](extending-dev-container.md).

### Prerequisites

As prerequisites, you need to have the following tools set up:

- Docker ([Installation instructions](https://docs.docker.com/engine/install))
- Visual Studio Code ([Installation instructions](https://code.visualstudio.com/download))
- Microsoft's Visual Studio Code Extension [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)

## Build Ankaios

The following steps assume an x86_64 host.
For Mac with Apple silicon, see chapter [Build for arm64 target](#build-for-arm64-target).

To build and test the Ankaios agent and server, run the following command inside the dev container:

```shell
cargo build
```

and for release

```shell
cargo build --release
```

As Ankaios uses musl for static linking, the binaries will be located in `target/x86_64-unknown-linux-musl`.

## Build for arm64 target

The dev container adds required tools for `arm64` architecture. To build Ankaios for `arm64`, run the following command inside the dev container:

```shell
cargo build --target aarch64-unknown-linux-musl --release
```

!!! info

    When using a dev container on Mac with Apple silicon and the build fails, change the file sharing implementation in Docker Desktop.
    Goto Docker Desktop and `Settings`, then `General` and change the file sharing implementation from `VirtioFS` to `gRPC FUSE`.
    See also [eclipse-ankaios/ankaios#147](https://github.com/eclipse-ankaios/ankaios/issues/147).
