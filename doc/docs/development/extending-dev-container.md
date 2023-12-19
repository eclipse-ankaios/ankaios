# Extending the dev container

The dev container is relatively large. Thus, we have split a base container which is available from `ghcr.io/eclipse-ankaios/devcontainer`.

If there is a need to include additional items in the dev container, please note that it is split into two parts due to its size:

* a base container which, in case of a change, needs to be build manually from .devcontainer/Dockerfile.base by running the following outside of the dev container:

    ```shell
    # Prepare the build with buildx. Depending on you environment
    # the following steps might be necessary:
    docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
    # Create and use a new builder. This needs top be called only once:
    docker buildx create --name mybuilder --driver docker-container --bootstrap
    docker buildx use mybuilder

    # Now build new base image for the dev container
    cd .devcontainer
    docker buildx build -t ghcr.io/eclipse-ankaios/devcontainer-base:<version> --platform linux/amd64,linux/arm64 -f Dockerfile.base .
    ```

    In order to push the base image append `--push` to the previous command.

    Note: If you wish to locally test the base image in VSCode before proceeding, utilize the default builder and exclusively build for the default platform like

    ```shell
    docker buildx use default
    docker buildx build -t ghcr.io/eclipse-ankaios/devcontainer-base:<version> -f Dockerfile.base --load .
    ```

* a docker container which derives from the base image mentioned above is specified in `.devcontainer/Dockerfile` (so don't forget to reference your new version there once you build one).

If you want to add some additional tools, you can initially do it in `.devcontainer/Dockerfile`, but later on they need to be pulled in the base image at some point in order to speed up the initial dev container build.
