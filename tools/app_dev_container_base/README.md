# Ankaios devcontainer for app development

This subfolder contains configurations for a devcontainer image that can be used to develop applications to be managed by Ankaios.

The devcontainer image simplyfies the start of developing workloads by using Ankaios as your orchestrator and minimizes efforts to get started with the development.

You can just use the prebuilt public devcontainer as base image in your specific devcontainer setup:

Example devcontainer Dockerfile:

```Docker
FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version>

RUN ... # customize the image with your dev dependencies
```

**NOTE:** Replace the `<ankaios_version>` with a tag pointing to an Ankaios release, e.g. 0.1.0.

The devcontainer contains the following:

- Prebult Ankaios executables:
  - Ankaios server
  - Ankaios agent
  - Ankaios CLI

- Control interface dependencies:
  - ankaios.proto (located at /usr/local/lib/ankaios/ankaios.proto)
  - protobuf-compiler
  - grpcurl

- Podman 4 (daemonless container orchestrator)

The control interface dependencies are essentially needed for use cases in which your app shall use the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/main/reference/control-interface/) to be able to communicate with the Ankaios orchestrator. An example use case would be to write a workload that shall request Ankaios to dynamically start another workload.

The prebuilt public container can be downloaded with the following command:

```shell
docker pull ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version>
```

## Run

Use the container with rootful podman inside:

```shell
docker run --privileged -it --rm ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version> /bin/bash
```

Use the container with rootless podman inside:

```shell
docker run --privileged -it --rm --user ankaios ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version> /bin/bash
```

Next, follow the steps in the [Quickstart guide](https://eclipse-ankaios.github.io/ankaios/main/usage/quickstart/) to try Ankaios out within the devcontainer.

## Build for multi-platform

```shell
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
docker buildx create --name mybuilder --driver docker-container --bootstrap
docker buildx use mybuilder
docker buildx build -t ghcr.io/eclipse-ankaios/app-ankaios-dev:<ankaios_version> --platform linux/amd64,linux/arm64 .
```

**NOTE:** If you are committer to Eclipse Ankaios project you are allowed to push the image to the public package repository of the organization. Just add `--push` to the command above.

## Build for local usage

```shell
docker build -t app-ankaios-dev:test .
```

To build a specific version provide the build arg `ANKAIOS_VERSION` to the build command like the following:

```shell
docker build -t app-ankaios-dev:test . --build-arg ANKAIOS_VERSION=v0.1.0
```

You can find all available release tags [here](https://github.com/eclipse-ankaios/ankaios/tags).
