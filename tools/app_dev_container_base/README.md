# Ankaios devcontainer for app development

This subfolder contains configurations for a devcontainer image that can be used to develop applications to be managed by Ankaios.

The devcontainer image makes it easy to start developing workloads using Ankaios as your orchestrator and minimizes the effort required to get started.

You can simply use the pre-built public devcontainer as a base image in your specific devcontainer setup:

Example devcontainer Dockerfile:

```Docker
FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version>

RUN ... # customize the image with your dev dependencies
```

**NOTE:** Replace the `<ankaios_version>` with a tag that points to an Ankaios release, e.g. 0.5.0.

The devcontainer contains the following:

- Pre-built Ankaios executables:
    - Ankaios server
    - Ankaios agent
    - Ankaios CLI

- Control interface dependencies:
    - ank_base.proto and control_api.proto (located at /usr/local/lib/ankaios/proto)
    - protobuf-compiler
    - grpcurl

- Podman 4 (daemonless container engine)

The control interface dependencies are essentially needed for use cases where your app needs to use the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) to communicate with the Ankaios orchestrator. An example use case would be to write a workload that requests Ankaios to dynamically start another workload.

The pre-built public container can be downloaded using the following command:

```shell
docker pull ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version>
```

## Run

Use the container with rootless podman inside (recommended):

```shell
docker run --privileged -it --rm --user ankaios ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version> /bin/bash
```

**Note:** The ankaios user has the starship shell activated, which contains a command prompt and tools more suited for development tasks.

Use the container with rootful podman inside:

```shell
docker run --privileged -it --rm ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version> /bin/bash
```

Next, follow the steps in the [Quickstart guide](https://eclipse-ankaios.github.io/ankaios/main/usage/quickstart/) to try Ankaios out within the devcontainer.

## Build for multi-platform

```shell
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes  --credential yes
docker buildx create --name mybuilder --driver docker-container --bootstrap
docker buildx use mybuilder
docker buildx build -t ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version> --platform linux/amd64,linux/arm64 .
```

**NOTE:** If you are committer to the Eclipse Ankaios project you can push the image to the organization's public repository. Just add `--push` to the above command.

## Build for local usage

```shell
docker build -t app-ankaios-dev:test .
```

To build a specific version provide the build arg `ANKAIOS_VERSION` to the build command like the following:

```shell
docker build -t app-ankaios-dev:test . --build-arg ANKAIOS_VERSION=v0.1.0
```

You can find all available release tags [here](https://github.com/eclipse-ankaios/ankaios/tags).
