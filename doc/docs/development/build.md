# Build

## Dev container 

The repo provides a Visual Studio Code [dev container](https://code.visualstudio.com/docs/devcontainers/containers) which includes all necessary tools to build all components and the documentation, but it does not provide the tools to run Ankaios as it's not the target platform. In case you want to extend the dev container see [extending the dev container](extending-dev-container.md).

### Prerequisites
- Docker ([Installation instructions](https://docs.docker.com/engine/install))
- Microsoft's Visual Studio Code Extension [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)

## Build Ankaios

To build and test the Ankaios agent and server, run the following command inside the dev container:

```shell
cargo build
```

and for release

```shell
cargo build --release
```

As Ankaios uses musl for static linking, the binaries will be located in `target/x86_64-unknown-linux-musl`.

## Build for arm64 target on x86 host

The dev container adds required tools for `arm64` architecture. To build Ankaios for `arm64`, run the following command inside the dev container:

```shell
cargo build --target aarch64-unknown-linux-musl --release
```
