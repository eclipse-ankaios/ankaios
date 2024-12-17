# Control Interface Tester workload

The Control Interface Tester workload is used in system tests to verify the correct execution of the Ankaios Control Interface.
The workload reads commands from a file, executes them and writes an output file containing the results of the commands.

If no Control Interface instance was provided to the workload, a `NoAPI` result is written. If the Control Interface was preliminary closed by Ankaios, e.g., due to a protocol error, a `ConnectionClosed` result is provided.

## Building and pushing a new image

It is planned to automate the process of building and pushing a new version of the container, but for now the process is done manually.

To build a new image run the following command from the project root folder:

```bash
podman build -t ghcr.io/eclipse-ankaios/control_interface_tester:manual-build-<new version number> . -f tests/resources/image/Dockerfile
```

To push the new image to GitHub container registry, you will need to generate an access token that is allowed to write packages and login to `ghcr.io`:

```bash
podman login ghcr.io
```

Afterwards the new image can be published with:

```bash
podman push ghcr.io/eclipse-ankaios/control_interface_tester:manual-build-<new version number>
```
