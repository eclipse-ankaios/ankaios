# Tutorial example speed consumer

Provides a container using Kuksa.val SDK which subscribes to the vehicle speed signal.

## Build

```shell
# locally
docker build -t ghcr.io/eclipse-ankaios/speed-consumer:latest .

# or for multiple platforms (add --push for pushing the image)
docker buildx build --platform linux/amd64,linux/arm64 -t ghcr.io/eclipse-ankaios/speed-consumer:latest .
```
