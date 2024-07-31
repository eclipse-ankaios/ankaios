# Ankaios in docker

For some scenarios it might be useful to run Ankaios in containers itself.
This includes quick evaluation without installation, or running Ankaios on non-Linux platforms like Windows or MacOS.

## Pre-conditions

Docker Desktop or docker engine with the compose plugin must be available.

On Manjaro the compose plugin can be installed with:

```shell
sudo pacman -S docker-compose
```

Afterwards the plugin is avilable as sub command `docker compose`.

## Startup

The setup includes

* One container running Ankaios server
* An other container running Ankaios agent and podman

Just call:

```shell
docker compose up -d
```

Afterwards the `ank` CLI can be used to interact with Ankaios.
Please make sure that the version of the `ank` CLI fits to the version of the Ankaios server and agent (see `compose.yaml`).

```shell
ank -k get workloads
```

## Shutdown

To stop Ankaios just call:

```shell
docker compose down
```

## Modify startup config

The basic setup includes an empty startup config.
In case workloads shall started at startup of the Ankaios server just modify the `server/state.yaml` and call:

```shell
docker compose build
```

Afterwards Ankaios can be started again using:

```shell
docker compose up -d
```
