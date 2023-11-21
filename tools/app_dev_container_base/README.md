# Ankaios devcontainer for app development

This subfolder contains configurations for a devcontainer image that can be used to develop applications to be managed by Ankaios.

The devcontainer image was designed to make the start of developing workloads by using Ankaios as your orchestrator easy and to minimize your effort and time to get started with the development.

You can just use the prebuilt public devcontainer as base image in your specific devcontainer setup:

Example devcontainer Dockerfile:
```Docker
FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:latest

RUN ... # customize the image with your dev dependencies
```

The devcontainer contains the prebuilt binaries of Ankaios of the latest release and the daemonless podman container orchestration tool which is used by Ankaios.

The prebuilt Ankaios binaries are the following:

- Ankaios server
- Ankaios agent
- Ankaios CLI

Furthermore, the devcontainer image contains the dev dependencies protobuf-compiler and grpcurl which are needed for use cases in which your app shall use the [Ankaios Control Interface](https://eclipse-ankaios.github.io/ankaios/latest/reference/control-interface/) to be able to communicate with the Ankaios orchestrator. An example use case would be to write a workload that shall request Ankaios to dynamically start another workload.

The prebuilt public container image containing the latest release of Ankaios binaries can be downloaded with the following command:

```shell
docker pull ghcr.io/eclipse-ankaios/app-ankaios-dev:latest
```

If you need a previous version, please built the devcontainer image by yourself according to the steps described [here](#build-for-local-usage).

## Build for local usage

To build the latest version use the following command:

```shell
cd app_ankaios_dev
docker build -t app-ankaios-dev:latest .
```

To build a specific version provide the build arg `ANKAIOS_VERSION` to the build command like the following:

```shell
cd app_ankaios_dev
docker build -t app-ankaios-dev:latest.0 . --build-arg ANKAIOS_VERSION=v0.1.0
```

You can find all available release tags [here](https://github.com/eclipse-ankaios/ankaios/tags).

## Build for publishing a new devcontainer

Publishing the devcontainer image into the Eclipse-Ankaios organaziation's package registry is only allowed for maintainers.

Because the Dockerfile will pull the latest Ankaios binaries in this case, the devcontainer image shall only built and published after the newest Ankaios binaries were released.

To publish the latest version run the following commands:

```shell
docker build -t ghcr.io/eclipse-ankaios/app-ankaios-dev:latest .
docker push ghcr.io/eclipse-ankaios/app-ankaios-dev:latest
```

### Run Ankaios with some example workload

First, put the example startup state below containing an nginx example workload into a file on your local filesystem named `startupState.yaml`:

Copy and paste the content below into the file `/tmp/startupState.yaml`. You can put the file in another location as well, but then you have to replace the file paths in the commands below.

```yaml
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    restart: true
    updateStrategy: AT_MOST_ONCE
    accessRights:
      allow: []
      deny: []
    tags:
      - key: owner
        value: Ankaios team
    runtimeConfig: |
      image: docker.io/nginx:latest
      ports:
      - containerPort: 80
        hostPort: 8081
```

Start the container with the following command:

```bash
docker run -it --rm -v /tmp/startupState.yaml:/tmp/startupState.yaml --privileged ghcr.io/eclipse-ankaios/app-ankaios-dev:latest /bin/bash
```
If you want to run a container image built locally you must replace the image url in the run command above.

Start the Ankaios server as background process:

```bash
ank-server --startup-config /tmp/startupState.yaml > /var/log/ankaios-server.log 2>&1 &
```

Start an Ankaios agent `agent_A` as background process:

```bash
ank-agent --name agent_A > /var/log/ankaios-agent_A.log 2>&1 &
```

You can check the logs of the Ankaios agent with the following command to see when the workload nginx becomes state EXEC_RUNNING:

```shell
tail -f /var/log/ankaios-agent_A.log
```

When the nginx workload is running, you can access the welcome page with the following command:

```bash
curl 127.0.0.1:8081
```
