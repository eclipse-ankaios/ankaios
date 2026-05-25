# Workload Persistence

## Overview

By default, workloads added at runtime via CLI or API are ephemeral and lost when the Ankaios server restarts. The basic persistence plugin allows you to mark workloads as persistent using tags, ensuring they survive server restarts.

The persistence plugin watches workload state changes via the Events API and automatically saves persistent workloads to a runtime state file.

## How It Works

The persistence plugin:
1. Watches for workload additions/changes via the Events API
2. Filters workloads based on their `persist` tag
3. Writes each persistent workload to its own file in `/var/lib/ankaios/workloads/<workload_name>.yaml`
4. On plugin startup, restores all persisted workloads from the workloads directory

## Persistence Modes

The plugin supports two persistence modes via the `persist` tag:

### ALWAYS
Persists the workload immediately when added to Ankaios (when it appears in desired state).

**Use cases:**
- Core infrastructure services
- Database containers
- Message queues
- Always-on monitoring agents

### ON_RUNNING
Persists the workload only after it successfully reaches the Running execution state (quality gate).

**Use cases:**
- Applications that must pass initialization checks
- Services where you only want to restore successfully started workloads
- Workloads with complex startup dependencies

## Marking Workloads as Persistent

### Using YAML Manifests

Add a `persist` tag to workload definitions:

```yaml
apiVersion: v1
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    tags:
      persist: ALWAYS  # Persist immediately
    runtimeConfig: |
      image: nginx:latest

  app-server:
    runtime: podman
    agent: agent_B
    tags:
      persist: ON_RUNNING  # Persist only after running
    runtimeConfig: |
      image: myapp:v1.2.3

  debug-shell:
    runtime: podman
    agent: agent_A
    # No persist tag - temporary (default)
    runtimeConfig: |
      image: busybox:latest
```

### Using the CLI

Currently, the `ank run workload` command does not support setting tags directly. Use `ank apply` with a YAML manifest instead.

```bash
# Create a manifest with persistence tags
cat > persistent-workload.yaml <<EOF
apiVersion: v1
workloads:
  redis:
    runtime: podman
    agent: cache_node
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: redis:alpine
EOF

# Apply the manifest
ank apply persistent-workload.yaml
```

## Installing the Persistence Plugin

### Prerequisites

The persistence plugin must be deployed as a workload with control interface access.

### Installation

Add the plugin to your startup manifest (`/etc/ankaios/state.yaml`):

```yaml
apiVersion: v1
workloads:
  basic_persistency:
    runtime: podman
    agent: qm_agent
    tags:
      - key: owner
        value: Ankaios Team
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

### Building the Plugin Container

```bash
# From the ankaios repository root
cd examples/plugins/basic_persistency

# Build the container image
podman build -t localhost/ank-persist:latest -f Dockerfile ../../..

# The image is now available for the plugin workload
```

## Startup and Runtime Behavior

### Plugin Startup

1. **Connect to Control Interface** - Plugin establishes connection to Ankaios
2. **Restore Persisted State** - Reads all `.yaml` files from `/var/lib/ankaios/workloads/` and applies workloads
3. **Subscribe to Events** - Watches for state changes via Events API
4. **Enter Event Loop** - Continuously monitors and persists workload changes

### Runtime Operation

When workloads are added, modified, or removed:

1. **Event Received** - Plugin receives state change notification
2. **Filter by Tag** - Only processes workloads with `persist` tag
3. **Check Persistence Mode**:
   - **ALWAYS**: Write workload to its own file immediately
   - **ON_RUNNING**: Wait for Running state, then write to file
4. **Atomic Write** - Each workload file is written atomically (temp file + rename)
5. **Workload Deletion** - When a workload is removed, its file is deleted from the workloads directory

### Server Restart

When the Ankaios server restarts:

1. Plugin starts with the server (if in startup manifest)
2. Plugin reads all workload files from `/var/lib/ankaios/workloads/` directory
3. Plugin sends separate UpdateStateRequest for each persisted workload
4. **Persistent workloads** are automatically restored with complete signature integrity
5. **Temporary workloads** remain lost (as intended)

## File Locations

| File/Directory | Purpose | Modified By |
|----------------|---------|-------------|
| `/etc/ankaios/state.yaml` | Startup manifest (includes plugin definition) | User (manual) |
| `/var/lib/ankaios/workloads/` | Directory containing persistent workload files | Plugin (automatic) |
| `/var/lib/ankaios/workloads/<workload_name>.yaml` | Individual workload persistence file with complete signature | Plugin (automatic) |
| `/var/lib/ankaios/workloads/.<workload_name>.yaml.tmp` | Temporary file during atomic writes | Plugin (automatic) |

## Use Cases

### Infrastructure Workloads (ALWAYS)

```yaml
workloads:
  prometheus:
    runtime: podman
    agent: monitoring_node
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: prom/prometheus:latest
```

### Application Workloads (ON_RUNNING)

```yaml
workloads:
  web-app:
    runtime: podman
    agent: app_node
    tags:
      persist: ON_RUNNING  # Only persist if startup succeeds
    runtimeConfig: |
      image: mycompany/webapp:v2.1
```

### Ephemeral Workloads (No Tag)

```yaml
workloads:
  debug-tools:
    runtime: podman
    agent: any_node
    # No persist tag - temporary
    runtimeConfig: |
      image: nicolaka/netshoot:latest
```

## Examples

### Example 1: Mixed Infrastructure

```yaml
apiVersion: v1
workloads:
  # Core infrastructure - always persist
  prometheus:
    runtime: podman
    agent: monitoring_node
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: prom/prometheus:latest

  # Application - only persist if it starts successfully
  api-server:
    runtime: podman
    agent: app_node
    tags:
      persist: ON_RUNNING
    runtimeConfig: |
      image: myapp/api:v1.0.0

  # Debug container - temporary
  netshoot:
    runtime: podman
    agent: app_node
    runtimeConfig: |
      image: nicolaka/netshoot:latest
```

After server restart:
- ✅ prometheus: Restored (ALWAYS)
- ✅ api-server: Restored (if it reached Running)
- ❌ netshoot: Gone (no persist tag)

### Example 2: Adding Persistent Service at Runtime

```bash
# Create manifest
cat > redis.yaml <<EOF
apiVersion: v1
workloads:
  redis:
    runtime: podman
    agent: cache_node
    tags:
      persist: ALWAYS
    runtimeConfig: |
      image: redis:alpine
EOF

# Apply it
ank apply redis.yaml

# Redis is now running AND will survive server restarts
```

## Checking Persistence Status

View persisted workload files:

```bash
# List all persisted workloads
ls -la /var/lib/ankaios/workloads/

# View a specific workload's persisted state
cat /var/lib/ankaios/workloads/<workload_name>.yaml
```

Check plugin logs:

```bash
# View plugin activity
journalctl -u ank-agent -f | grep basic_persistency
```

Expected log messages:
- `Adding workload 'X' with persist: ALWAYS` - Workload will be persisted immediately
- `Workload 'X' has persist: ON_RUNNING, waiting for Running state` - Waiting for quality gate
- `Workload 'X' reached Running state, fetching definition to persist` - ON_RUNNING workload persisted
- `Persisted workload 'X' to "/var/lib/ankaios/workloads/X.yaml"` - Workload file written
- `Removed persisted workload 'X' from "/var/lib/ankaios/workloads/X.yaml"` - Workload file deleted

## Troubleshooting

### Workload Not Persisting

**Problem:** Workload disappears after server restart

**Solutions:**

1. Check that the `persist` tag is set correctly:
   ```bash
   ank get state desiredState.workloads.<workload-name>
   # Look for: tags.persist: ALWAYS or ON_RUNNING
   ```

2. Verify the plugin is running:
   ```bash
   ank get workloads | grep basic_persistency
   ```

3. Check plugin logs for errors:
   ```bash
   journalctl -u ank-agent -f | grep -i persist
   ```

4. Check directory permissions:
   ```bash
   ls -la /var/lib/ankaios/workloads/
   # Directory should be writable by the plugin container
   ```

### Plugin Not Starting

**Problem:** Plugin container crashes or doesn't start

**Solutions:**

1. Check control interface permissions in startup manifest (see Installation section)

2. Ensure `/var/lib/ankaios/workloads` directory exists:
   ```bash
   sudo mkdir -p /var/lib/ankaios/workloads
   ```

3. Check plugin container logs:
   ```bash
   podman logs <plugin-container-id>
   ```

### ON_RUNNING Workload Never Persisted

**Problem:** Workload with `persist: ON_RUNNING` runs but isn't in the persistence file

**Possible causes:**

1. **Workload never reached Running state** - Check execution state:
   ```bash
   ank get workloads <workload-name>
   ```

2. **Very short-lived workload** - Container may have exited before reaching Running:
   ```bash
   # Check plugin logs for state transitions
   journalctl -u ank-agent -f | grep "current state:"
   ```

   If you see `current state: Succeeded, is_running: false`, the workload exited too quickly. Consider using `persist: ALWAYS` instead.

### Persistence File Corruption

**Problem:** Plugin logs show YAML parsing errors for specific workloads

**Solution:**

```bash
# Check individual workload files
ls -la /var/lib/ankaios/workloads/
cat /var/lib/ankaios/workloads/<workload_name>.yaml

# If a specific workload file is corrupted, remove it
sudo rm /var/lib/ankaios/workloads/<workload_name>.yaml

# Restart plugin to recreate from current state
ank delete workload basic_persistency
ank apply /etc/ankaios/state.yaml
```

## Resetting Runtime State

To clear all persisted workloads and start fresh:

```bash
# Stop the server
sudo systemctl stop ank-server

# Remove all persisted workload files
sudo rm -rf /var/lib/ankaios/workloads/*.yaml

# Start the server
sudo systemctl start ank-server
```

Now only workloads from the startup manifest will be running.

## Best Practices

1. **Use startup manifest for critical services** - Include the persistence plugin itself and core infrastructure
2. **Use ALWAYS for infrastructure** - Databases, queues, monitoring that should always run
3. **Use ON_RUNNING for applications** - Services where you only want to restore successfully started workloads
4. **Don't persist debug containers** - Leave troubleshooting tools without persist tags
5. **Monitor plugin health** - Ensure the plugin is running for persistence to work
6. **Test recovery regularly** - Periodically test server restart to verify persistence works
7. **Back up runtime state** - Keep backups of `/var/lib/ankaios/workloads/` directory

## Limitations

- **Plugin must be running** - Persistence only works when the plugin is active
- **No persistence for configs** - Currently only workloads are persisted (configs are included but not selectively)
- **Manual cleanup of removed workloads** - While workload files are automatically deleted when workloads are removed, orphaned files may remain if the plugin was not running
- **No persistence history** - Only current state is saved, no versioning
- **No CLI support for tags** - Must use YAML manifests to set persist tags

## Advanced Topics

### Custom Persistence Directory Location

By default, the plugin uses `/var/lib/ankaios/workloads/` for persistence files. To change this:

1. Set the `PERSISTENCE_FILE_PATH` environment variable in the plugin workload:

```yaml
workloads:
  basic_persistency:
    runtime: podman
    agent: qm_agent
    runtimeConfig: |
      image: localhost/ank-persist:latest
      commandOptions: 
        - "-v"
        - "/custom/path:/custom/path"
        - "-e"
        - "PERSISTENCE_FILE_PATH=/custom/path/runtime_state.yaml"
```

2. The plugin will create a `workloads/` subdirectory in the parent directory:
   - `PERSISTENCE_FILE_PATH=/custom/path/runtime_state.yaml` → workloads directory: `/custom/path/workloads/`
   - Individual workload files will be: `/custom/path/workloads/<workload_name>.yaml`

3. Ensure the volume mount includes the parent directory with write permissions

### Plugin Development

The basic persistence plugin is located at `examples/plugins/basic_persistency/` in the Ankaios repository.

Key files:
- `src/main.rs` - Main plugin implementation
- `Cargo.toml` - Rust dependencies
- `Dockerfile` - Container build instructions
- `manifest.yaml` - Plugin manifest for deployment
- `README.md` - Plugin-specific documentation

## See Also

- [Control Interface](../reference/control-interface.md)
- [Events API](../reference/events-api.md)
- [Complete State](../reference/complete-state.md)
- [Plugin Development Guide](../development/plugins.md)
