# Basic Persistence Plugin for Ankaios

A plugin that persists Ankaios workloads based on tags, allowing selective workload restoration after server restarts.

## Overview

This plugin watches workload state changes via the Ankaios Events API and persists workloads marked with a `persist` tag to a runtime state file (`/var/lib/ankaios/runtime_state.yaml`). When the Ankaios server restarts, workloads from this file are automatically restored.

## Persistence Modes

Configure persistence using the `persist` tag in your workload manifest:

- **`ALWAYS`**: Persist the workload as soon as the server accepts it (in desired state), even if deployment is pending or failed
- **`ON_RUNNING`**: Persist only when workload reaches the Running execution state (quality gate)

**Note**: For very short-lived workloads, `ON_RUNNING` may not capture the Running state if the container exits too quickly. Use `ALWAYS` for such workloads.

## Usage

### 1. Build the Plugin Image

```bash
# From the Ankaios repository root
podman build -t localhost/ank-persist:latest -f examples/plugins/basic_persistency/Dockerfile .
```

### 2. Deploy the Plugin

Add the plugin to your startup manifest (`/etc/ankaios/state.yaml`):

```yaml
apiVersion: v1
workloads:
  basic_persistency:
    runtime: podman
    agent: qm_agent
    runtimeConfig: |
      image: localhost/ank-persist:latest
      commandOptions: ["-v", "/var/lib/ankaios:/var/lib/ankaios"]
    controlInterfaceAccess:
      allowRules:
        # Write permission to restore persisted state on startup
        - type: StateRule
          operation: Write
          filterMasks:
            - "desiredState.workloads.*"
            - "desiredState.configs.*"
        # Read permission to monitor state changes
        - type: StateRule
          operation: Read
          filterMasks:
            - "workloadStates.*.*.*.state"
            - "desiredState.workloads.*"
            - "desiredState.configs"
```

Then restart the server to load the plugin:

```bash
sudo systemctl restart ank-server
```

The plugin will start as a workload and begin watching for state changes.

### 3. Mark Workloads for Persistence

Add the `persist` tag to workloads you want to survive server restarts:

```yaml
apiVersion: v1
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    tags:
      persist: ALWAYS  # This workload will be persisted
      owner: infrastructure
    runtimeConfig: |
      image: nginx:latest
```

### 4. Verify Persistence

Check the runtime state file:

```bash
cat /var/lib/ankaios/runtime_state.yaml
```

### 5. Test Restart Behavior

```bash
# Restart the Ankaios server
sudo systemctl restart ank-server

# Verify workloads were restored
ank get workloads
```

## Configuration

The plugin can be configured via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PERSISTENCE_FILE_PATH` | `/var/lib/ankaios/runtime_state.yaml` | Path to the runtime state file |
| `RUST_LOG` | `info` | Logging level (`debug`, `info`, `warn`, `error`) |

To set environment variables in the workload manifest:

```yaml
workloads:
  basic_persistency:
    runtimeConfig: |
      image: localhost/ank-persist:latest
      commandOptions:
        - "-v"
        - "/var/lib/ankaios:/var/lib/ankaios"
        - "-e"
        - "RUST_LOG=debug"
```

## Architecture

The plugin:

1. **Connects to Control Interface**: Establishes connection via named pipes
2. **Restores State on Startup**: Reads `/var/lib/ankaios/runtime_state.yaml` and sends UpdateStateRequest to restore workloads
3. **Subscribes to Events**: Watches for workload state changes via the Ankaios Events API
4. **Filters Workloads**: Extracts workloads with the `persist` tag
5. **Caches ON_RUNNING Workloads**: Maintains a HashSet of workloads waiting for Running state
6. **Writes Atomically**: Uses atomic file writes (temp → rename) to prevent corruption

## File Format

The runtime state file uses the same YAML format as Ankaios manifests:

```yaml
apiVersion: v1
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: nginx:latest

configs:
  # All configs are persisted
  app_config: "..."
```

## Limitations

- The plugin must have write access to `/var/lib/ankaios/` directory
- The `/var/lib/ankaios/` directory must exist before the plugin starts
- Plugin must be running for persistence to work (no persistence if plugin crashes)
- ON_RUNNING mode may miss very short-lived containers (use ALWAYS instead)
- No CLI support for setting tags (must use YAML manifests)
- All configs are persisted (cannot selectively persist configs)

## Security Considerations

- The plugin requires **write access** to restore persisted state (via UpdateStateRequest)
- Control interface access is **restricted by allowRules** (only specific state paths)
- File writes are **isolated** to `/var/lib/ankaios/` via volume mount
- Uses **atomic file writes** to prevent corruption during concurrent access
- Consider using SELinux/AppArmor policies for production deployments

## Troubleshooting

### Plugin not starting

Check the plugin logs:
```bash
ank get workload basic_persistency
ank logs basic_persistency
```

### Workload not persisting

1. Verify the `persist` tag is set correctly (case-sensitive)
2. Check plugin logs for warnings about invalid tag values
3. Verify the plugin has write access to `/var/lib/ankaios/`

### State file corruption

If the state file becomes corrupted, the plugin will log errors. To recover:

```bash
# Stop the server
sudo systemctl stop ank-server

# Remove corrupted file (plugin will recreate on next workload change)
sudo rm /var/lib/ankaios/runtime_state.yaml

# Restart the server
sudo systemctl start ank-server
```

## Development

### Running Locally

```bash
# Build the plugin
cargo build --release

# Set environment variables
export PERSISTENCE_FILE_PATH=/tmp/runtime_state.yaml
export RUST_LOG=debug

# Run the plugin
./target/release/basic_persistency
```

### Testing

Create a test workload with persistence:

```yaml
apiVersion: v1
workloads:
  test:
    runtime: podman
    agent: agent_A
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: alpine:latest
      commandArgs: ["sleep", "infinity"]
```

Apply it and check the runtime state file:

```bash
# Save the manifest
cat > test-persist.yaml <<EOF
apiVersion: v1
workloads:
  test:
    runtime: podman
    agent: agent_A
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: alpine:latest
      commandArgs: ["sleep", "infinity"]
EOF

# Apply it
ank apply test-persist.yaml

# Check persistence file
cat /var/lib/ankaios/runtime_state.yaml

# Check plugin logs
journalctl -u ank-agent -f | grep "Adding workload 'test'"
```

## Contributing

This plugin serves as an example of how to build Ankaios plugins. Contributions welcome:

- Add metrics/monitoring support
- Support remote storage backends (S3, etcd, etc.)
- Add unit tests
- Add state transition tracking for more sophisticated persistence policies

## License

Apache License 2.0
