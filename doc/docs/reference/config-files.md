# Ankaios Configuration Files

Ankaios now supports configuration files to manage settings more efficiently. These configuration files are optional but provide a convenient way to manage parameters that you may not want to pass as command-line arguments. The priority for parameters is as follows:

1. Command-line arguments
2. Environment variables
3. Configuration file

The configuration files are formatted in TOML, which is easy to parse and fast to process.

## Configuration File Locations

- **Server Configuration**: `/etc/ankaios/server.toml`
- **Agent Configuration**: `/etc/ankaios/server.toml`
- **CLI Configuration**: `$HOME/.config/ankaios/ank.toml`

## Configuration File Structure

### Ankaios Server Configuration (`ank-server.conf`)

```toml
# This is the configuration file for an Ankaios server.
version = 'v1'

# The path to the startup configuration manifest file.
startup_manifest = '/workspaces/ankaios/server/resources/startConfig.yaml'

# The address, including the port, to which the server should listen.
address = '127.0.0.1:25551'

# The flag to disable TLS communication.
insecure = true

# The path to ca certificate pem file.
ca_pem = '/etc/ankaios/certs/ca.pem'

# The path to server certificate pem file.
crt_pem = '/etc/ankaios/certs/ank-server.pem'

# The path to server key pem file.
key_pem = '/etc/ankaios/certs/ank-server-key.pem'

# The content of the ca certificate pem file (optional).
# ca_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the server certificate pem file (optional).
# crt_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the server key pem file (optional).
# key_pem_content = '''-----BEGIN PRIVATE KEY-----
# ...
# -----END PRIVATE KEY-----'''
```

### Ankaios Agent Configuration (`ank-agent.toml`)

```toml
# This is the configuration file for an Ankaios agent.
version = 'v1'

# The name to use for the registration with the server.
name = 'agent_1'

# The server URL.
server_url = 'https://127.0.0.1:25551'

# The path to manage the fifo files.
run_folder = '/tmp/ankaios/'

# The flag to disable TLS communication with the server.
insecure = false

# The path to the ca certificate pem file.
ca_pem = '/etc/ankaios/certs/ca.pem'

# The path to agent certificate pem file.
crt_pem = '/etc/ankaios/certs/ank-agent.pem'

# The path to agent key pem file.
key_pem = '/etc/ankaios/certs/ank-agent-key.pem'

# The content of the ca certificate pem file (optional).
# ca_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the agent certificate pem file (optional).
# crt_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the agent key pem file (optional).
# key_pem_content = '''-----BEGIN PRIVATE KEY-----
# ...
# -----END PRIVATE KEY-----'''
```

### Ankaios CLI Configuration (`ank.conf`)

```toml
# This is the configuration file for the ank CLI.
version = 'v1'

# The timeout in milliseconds to wait for a response from the ank-server.
response_timeout = 3000  # milliseconds

# The flag to enable verbose output.
verbose = false

# The flag to disable all output.
quiet = false

# The flag that enables waiting for workloads to be created/deleted.
no_wait = false

[default]
# The URL to Ankaios server.
server_url = 'https://127.0.0.1:25551'

# The flag to disable TLS communication with the server.
insecure = false

# The path to the ca certificate pem file.
ca_pem = '/etc/ankaios/certs/ca.pem'

# The path to CLI certificate pem file.
crt_pem = '/home/ankaios/.config/ankaios/ank.pem'

# The path to CLI key pem file.
key_pem = '${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank-key.pem'

# The content of the ca certificate pem file (optional).
# ca_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the CLI certificate pem file (optional).
# crt_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the CLI key pem file (optional).
# key_pem_content = '''-----BEGIN PRIVATE KEY-----
# ...
# -----END PRIVATE KEY-----'''
```

## Using the Configuration Files

To use the configuration files, you can specify the path to the configuration file using the `-x` command-line argument:

```sh
ank-server -x /path/to/ank-server.conf
ank-agent -x /path/to/ank-agent.conf
ank -x /path/to/ank.conf
```

## Notes

- The configuration files are optional. If not provided, the default values will be used.
- You can specify either the path to the certificate files or the content of the certificate files, but not both.
- The `version` field is mandatory to ensure compatibility.

This documentation provides an introduction in the usage of config files for Ankaios components. Please fell free to contact us if you have any
any questions or need further assistance.
