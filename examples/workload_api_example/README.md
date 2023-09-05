# Workload API example

An example of how to use the Ankaios API from inside a Workload.

## Build

To build the example workload as a podman container please us the following command:

```
podman build -t ankaios_workload_api_example ../../ -f Dockerfile
```

In the command we explicitly set the context two directories higher in order to get access to the api library needed because of the protobuf dependencies.
