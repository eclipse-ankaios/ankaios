# Extending the dev container

The dev container is relatively large.
If there is a need to include additional items in the dev container, please note that it is split into two parts due to its size:

* A base container available from `ghcr.io/eclipse-ankaios/devcontainer` which, in case of a change, needs to be build manually from `.devcontainer/Dockerfile.base` (see below for instructions).

* A docker container which derives from the base image mentioned above is specified in `.devcontainer/Dockerfile` (so don't forget to reference your new version there once you build one).

If you want to add some additional tools, you can initially do it in `.devcontainer/Dockerfile`, but later on they need to be pulled in the base image in order to speed up the initial dev container build.

## Build the base container

The base container is available for amd64 and arm64/v8 architectures. There are two options to build the base container:

1. Multiplatform build for amd64 and arm64
2. Separately building images for amd64 and arm64 and joining them afterwards

### Multiplatform build

In case the multiplatform build is used, one image can be build natively on the host platform (usually amd64) while the other needs to be emulated.

Build the base container by running the following commands outside of the dev container:

```shell
# Prepare the build with buildx. Depending on you environment
# the following steps might be necessary:
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes  --credential yes

# Create and use a new builder. This needs to be called only once:
docker buildx create --name mybuilder --driver docker-container --bootstrap
docker buildx use mybuilder

# Now build the new base image for the dev container
cd .devcontainer
docker buildx build -t ghcr.io/eclipse-ankaios/devcontainer-base:<version> --platform linux/amd64,linux/arm64 -f Dockerfile.base .
```

In order to push the base image append `--push` to the previous command.

Note: If you wish to locally test the base image in VSCode before proceeding, utilize the default builder and exclusively build for the default platform like

```shell
docker buildx use default
docker buildx build -t ghcr.io/eclipse-ankaios/devcontainer-base:<version> -f Dockerfile.base --load .
```

### Separate builds for different architectures

Due to the emulation for the non-host architecture, the previous multiplatform build might take some time.
An alternative is to build the two images separately on different hosts matching the target architecture.
For arm64 for example cloud instances with ARM architecture (like AWS Graviton) can be used.

To build the base image this way, perform the following steps:

```shell
# On arm64 host: Build arm64 image
cd .devcontainer
docker buildx build -t ghcr.io/eclipse-ankaios/devcontainer-base:<version>-arm64 -f Dockerfile.base --push .

#  On amd64 host: Build amd64 image
cd .devcontainer
docker buildx build -t ghcr.io/eclipse-ankaios/devcontainer-base:<version>-amd64 -f Dockerfile.base --push .

# On any host: Create manifest list referencing both images
docker buildx imagetools create \
  -t ghcr.io/eclipse-ankaios/devcontainer-base:<version> \
  ghcr.io/eclipse-ankaios/devcontainer-base:<version>-amd64 \
  ghcr.io/eclipse-ankaios/devcontainer-base:<version>-arm64
```
