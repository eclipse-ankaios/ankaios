# Tutorial fleet connector

Provides a container using the ank-sdk-python to connect to the control interface and also subscribes to MQTT.

## Build

```shell
# locally
docker build -t ghcr.io/eclipse-ankaios/fleet-connector:latest .

# or for multiple platforms (add --push for pushing the image)
docker buildx build --platform linux/amd64,linux/arm64 -t ghcr.io/eclipse-ankaios/fleet-connector:latest .
```
