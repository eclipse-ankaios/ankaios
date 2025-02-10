# Sleepy System Tests container

This container executes a very long sleep and implements a SIGTERM handler that takes care that the workload si quickly stopped by Podman (as opposed to the 10 second wait, if the signal is not handled).

The container is very useful for system test where a running workload is needed, but no ports or anything else is checked.

The container is also pushed to the Ankaios organization for both amd64 and arm64 platforms.
If you require some changes, please ensure to build and push the new version for both platforms, e.g., via the following command as `latest`:

```bash
docker buildx build -t ghcr.io/eclipse-ankaios/tests/sleepy:latest --platform linux/amd64,linux/arm64 -f Containerfile --push .
```
