# Shareable configuration

Ankaios supports a shareable configuration approach, which allows configurations to be defined once and assigned to any number of workloads. To use sharable configurations, you can define config references in the [handlebars](https://github.com/sunng87/handlebars-rust) templating syntax. The following workload configuration fields currently support template expansion:

* `agent`
* `runtimeConfig`
* the subfields `data` and `binaryData` within the `files` field

For a basic example of sharing configuration with workloads, see [here](../../reference/startup-configuration.md). For detailed information about using the `files` field, see [here](mount-files.md).

## Indentation for multi-line configuration

When using the default handlebars template syntax (`{{config_variable}}`), the line indentation of the current context is not considered. To ensure the validity of certain layouts that rely on the indentation level of multi-line configuration, utilize the following custom `indent` control structure, highlighted below:

```yaml linenums="1" hl_lines="16"
apiVersion: v0.1
workloads:
  frontend:
    agent: agent_A
    runtime: podman-kube
    configs:
      nginx_conf: nginx_config
    runtimeConfig: |
      manifest: |
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: nginx-config
        data:
          nginx.conf: |
            {{> indent content=nginx_conf}}
        ---
        apiVersion: v1
        kind: Pod
        metadata:
          name: nginx-pod
        spec:
          restartPolicy: Never
          containers:
          - name: nginx-container
            image: docker.io/nginx:latest
            ports:
            - containerPort: 80
              hostPort: 8080
            volumeMounts:
            - name: nginx-config-volume
              mountPath: /etc/nginx/nginx.conf
              subPath: nginx.conf
          volumes:
          - name: nginx-config-volume
            configMap:
              name: nginx-config
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
```

With the default template syntax (`{{nginx_conf}}`) instead, the expanded state will result in an invalid YAML:

```yaml
...
    runtimeConfig: |
      manifest: |
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: nginx-config
        data:
          nginx.conf: |
            worker_processes  1;

events {
    worker_connections  1024;
}

http {
    server {
...
```

By using the `indent` control structure, the line indentation of the current context will be considered which results in an error-free YAML file.

```yaml
...
    runtimeConfig: |
      manifest: |
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: nginx-config
        data:
          nginx.conf: |
            worker_processes  1;

            events {
                worker_connections  1024;
            }

            http {
                server {
...
```
