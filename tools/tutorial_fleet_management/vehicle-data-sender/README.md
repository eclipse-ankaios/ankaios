# Tutorial example vehicle data sender

Sample container for connecting to an MQTT broker and sending speed values at configurable intervals.

## Build

```shell
# locally
docker build -t ghcr.io/eclipse-ankaios/vehicle-data-sender:latest .

# or for multiple platforms (add --push for pushing the image)
docker buildx build --platform linux/amd64,linux/arm64 -t ghcr.io/eclipse-ankaios/vehicle-data-sender .
```
