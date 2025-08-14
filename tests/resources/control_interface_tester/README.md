# Control Interface Tester workload

The Control Interface Tester workload is used in system tests to verify the correct execution of the Ankaios Control Interface.
The workload reads commands from a file, executes them and writes an output file containing the results of the commands.

If no Control Interface instance was provided to the workload, a `NoAPI` result is written. If the Control Interface was preliminary closed by Ankaios, e.g., due to a protocol error, a `ConnectionClosed` result is provided.

## Building a new image

When running the stests, the correct image will be fetched automatically from `ghcr.io/eclipse-ankaios`, based on the hash code. To manually build the image, a just command is available:

```bash
just build-stest-image
```

To get the has code of the image, a script is provided:

```bash
./tools/control_interface_workload_hash.sh
```

## Pushing the new image to the registry

To push the new image to the GitHub container registry, you will need to generate an access token that is allowed to write packages and login to `ghcr.io`:

```bash
podman login ghcr.io
```

Afterwards the new image can be published with:

```bash
podman push ghcr.io/eclipse-ankaios/control_interface_tester:$(./tools/control_interface_workload_hash.sh)
```
