# Mount files

Ankaios supports to mount files into a workload. Define the mount files in the `files` field of a workload configuration, which supports text and base64 encoded content. The files are mounted in readonly mode. Mount files are not supported for a workload with runtime `podman-kube`. Instead, use the built-in `ConfigMaps` feature of `podman-kube`.

The following manifest contains a workload with a mounted web server configuration and another workload executing a mounted binary file. It also combines the [Ankaios Shareable Configuration Approach](config-rendering.md) with mount files by defining the contents of the mount files once and sharing them with workloads. Ankaios expands the templated subfields `data` and `binaryData` using the handlebars template engine.

To get an overview about which workload configuration fields currently support template expansion, see [here](config-rendering.md).

```yaml linenums="1" hl_lines="8-10 19-21"
apiVersion: v0.1
workloads:
  nginx:
    agent: agent_A
    runtime: podman
    configs:
      nginx_conf: nginx_config
    files:
      - mountPoint: "/etc/nginx/nginx.conf" # mount point in the container
        data: "{{nginx_conf}}" # (1)!
    runtimeConfig: |
      image: docker.io/nginx:latest
      commandOptions: [ "-p", "8087:80" ]
  hello:
    agent: agent_A
    runtime: podman
    configs:
      bin_data: bin_data
    files:
      - mountPoint: "/hello" # mount point in the container
        binaryData: "{{bin_data}}" # (2)!
    runtimeConfig: |
      image: docker.io/alpine:latest
      commandOptions: [ "--entrypoint", "/bin/sh" ]
      commandArgs: [ "-c", "cat /hello" ]
configs:
  nginx_config: |
    worker_processes  1;

    events {
        worker_connections  1024;
    }

    http {
        server {
            listen 80;
            server_name custom_nginx;

            location /custom {
                default_type text/plain;
                return 200 "The mounted custom nginx.conf is being used!\n";
            }
        }
    }
  # base64 encoded content
  bin_data: SGVsbG8sIFdvcmxkCg==
```

1. The contents of the `data` field will be rendered and replaced with the custom web server configuration of `nginx_config` part of the `configs` field below.

2. The contents of the `binaryData` field will be rendered and replaced with the content of `bin_data` part of the `configs` field below.
