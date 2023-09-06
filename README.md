<picture style="padding-bottom: 1em;">
  <source media="(prefers-color-scheme: dark)" srcset="logo/Ankaios__logo_for_dark_bgrd_clipped.png">
  <source media="(prefers-color-scheme: light)" srcset="logo/Ankaios__logo_for_light_bgrd_clipped.png">
  <img alt="Shows Ankaios logo" src="logo/Ankaios__logo_for_light_bgrd_clipped.png">
</picture>

# Eclipse Ankaios (a great project)

Eclipse Ankaios provides workload and container orchestration for automotive
High Performance Computing Software (HPCs). While it can be used for various
fields of applications, it is developed from scratch for automotive use cases
and provides a slim yet powerful solution to manage containerized applications.
It supports various container runtimes with Podman as the first one, but other
container runtimes and even native applications can be supported. Eclipse
Ankaios is independent of existing communication frameworks like SOME/IP, DDS,
or REST API.

Eclipse Ankaios manages multiple nodes and virtual machines with a single unique
API in order to start, stop, configure, and update containers and workloads. It
provides a central place to manage automotive applications with a setup
consisting of one server and multiple agents. Usually one agent per node
connects to one or more runtimes that are running the workloads.

## Usage

For using Ankaios see [documentation](https://eclipse-ankaios.github.io/ankaios).

## Contribution

This project welcomes contributions and suggestions. Before contributing, make sure to read the
[contribution guideline](https://github.com/eclipse-ankaios/ankaios/blob/master/CONTRIBUTING.md).

## License

Eclipse Ankaios is licensed using the Apache License Version 2.0.
