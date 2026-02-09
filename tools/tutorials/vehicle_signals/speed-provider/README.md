# Tutorial example speed-provider

Provides a container using Kuksa.val SDK to feed vehicle speed values to a databroker.

## Build

```shell
# locally
docker build -t ghcr.io/eclipse-ankaios/speed-provider:latest .

# or for multiple platforms (add --push for pushing the image)
docker buildx build --platform linux/amd64,linux/arm64 -t ghcr.io/eclipse-ankaios/speed-provider:latest .
```
