# Configuration files

Ankaios now supports configuration files to manage settings more efficiently. These configuration files are optional but provide a convenient way to manage parameters that you may not want to pass as command-line arguments. The priority for parameters is as follows:

1. Command-line arguments
2. Environment variables
3. Configuration file

The configuration files are formatted in [TOML](https://toml.io/), which is easy to parse and fast to process.

## Configuration File Locations

The Ankaios configuration files are per default loaded from the following location:

- **Server Configuration**: `/etc/ankaios/ank-server.conf`
- **Agent Configuration**: `/etc/ankaios/ank-agent.conf`
- **CLI Configuration**: `$HOME/.config/ankaios/ank.conf`

## Configuration File Structure

The following three examples show how the Ankaios configuration files look like:

### Ankaios Server Configuration (`ank-server.conf`)

```toml
# This is the configuration file for an Ankaios server.
# This configuration file is formatted in the TOML language.

# The format version of the configuration file to ensure compatibility.
version = 'v1'

# The path to the startup configuration manifest file.
# By default, no startup configuration manifest is used.
startup_manifest = '/etc/ankaios/state.yaml'

# The address, including the port, to which the server should listen.
address = '127.0.0.1:25551'

# The flag to disable TLS communication between
# the Ankaios server, agents and the ank CLI.
# If set to 'true' and the certificates are not provided, then the server shall not use TLS.
insecure = false

# The path to ca certificate pem file.
ca_pem = '/etc/ankaios/certs/ca.pem'

# The path to server certificate pem file.
crt_pem = '/etc/ankaios/certs/ank-server.pem'

# The path to server key pem file.
key_pem = '/etc/ankaios/certs/ank-server-key.pem'

# The content of the ca certificate pem file.
# You can either provide key_pem or key_pem_content, but not both.
# ca_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the server certificate pem file.
# You can either provide key_pem or key_pem_content, but not both.
# crt_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the server key pem file.
# You can either provide key_pem or key_pem_content, but not both.
# key_pem_content = '''-----BEGIN PRIVATE KEY-----
# ...
# -----END PRIVATE KEY-----'''
```

### Ankaios Agent Configuration (`ank-agent.toml`)

```toml
# This is the configuration file for an Ankaios agent.
# This configuration file is formatted in the TOML language.

# The format version of the configuration file to ensure compatibility.
version = 'v1'

# The name to use for the registration with the server.
# Every agent has to register with a unique name.
# Agent name shall contain only regular upper and lowercase characters
# (a-z and A-Z), numbers and the symbols "-" and "_".
name = 'agent_1'

# The server URL.
server_url = 'https://127.0.0.1:25551'

# An existing path where to manage the fifo files.
# If not set, defaults to '$TMPDIR/ankaios/' (falls back to '/tmp/ankaios/' if TMPDIR is not set).
run_folder = '/tmp/ankaios/'

# The flag to disable TLS communication with the server.
# If set to 'true', then the agent shall not use TLS.
insecure = false

# The path to the ca certificate pem file.
ca_pem = '/etc/ankaios/certs/ca.pem'

# The path to agent certificate pem file.
crt_pem = '/etc/ankaios/certs/ank-agent.pem'

# The path to agent key pem file.
key_pem = '/etc/ankaios/certs/ank-agent-key.pem'

# The content of the ca certificate pem file.
# You can either provide ca_pem or ca_pem_content, but not both
# ca_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the agent certificate pem file.
# You can either provide crt_pem or crt_pem_content, but not both
# crt_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the agent key pem file.
# You can either provide key_pem or key_pem_content, but not both
# key_pem_content = '''-----BEGIN PRIVATE KEY-----
# ...
# -----END PRIVATE KEY-----'''
```

### Ankaios CLI Configuration (`ank.conf`)

```toml
# This is the configuration file for the ank CLI.
# This configuration file is formatted in the TOML language.

# The format version of the configuration file to ensure compatibility.
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
# If started in insecure mode then the HTTP protocol shall be used,
# otherwise the HTTPS protocol shall be used.
server_url = 'https://127.0.0.1:25551'

# The flag to disable TLS communication with the server.
# If set to 'true', then the CLI shall not use TLS.
insecure = false

# The path to the ca certificate pem file.
ca_pem = '/etc/ankaios/certs/ca.pem'

# The path to CLI certificate pem file.
crt_pem = '/home/ankaios/.config/ankaios/ank.pem'

# The path to CLI key pem file.
key_pem = '/home/ankaios/.config/ankaios/ank-key.pem'

# The content of the ca certificate pem file.
# You can either provide ca_pem or ca_pem_content, but not both.
# ca_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the CLI certificate pem file.
# You can either provide crt_pem or crt_pem_content, but not both.
# crt_pem_content = '''-----BEGIN CERTIFICATE-----
# ...
# -----END CERTIFICATE-----'''

# The content of the CLI key pem file.
# You can either provide key_pem or key_pem_content, but not both.
# key_pem_content = '''-----BEGIN PRIVATE KEY-----
# ...
# -----END PRIVATE KEY-----'''
```

## Using the Configuration Files

To use the configuration files, just place a config file at the default location specified [above](#configuration-file-locations), or specify the path to the configuration file using the `-x` command-line argument:

```sh
ank-server -x /path/to/ank-server.conf
ank-agent -x /path/to/ank-agent.conf
ank -x /path/to/ank.conf
```

## Notes

- The configuration files are optional. If not provided, the default values will be used.
- You can specify either the path to the certificate files or the content of the certificate files, but not both.
- The `version` field is mandatory to ensure compatibility.

This documentation provides an introduction in the usage of config files for Ankaios components. Please fell free to contact us if you have
any questions or need further assistance.
