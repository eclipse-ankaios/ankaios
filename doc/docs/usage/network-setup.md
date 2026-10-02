# Connecting Ankaios components over the network

 The standard [installation](installation.md) configures the server, agent and CLI to communicate over a local Unix domain socket (see the respective [configuration files](../reference/config-files.md)), which relies on filesystem permissions and only works when all components run on the same host.
 To connect an agent or the CLI from a different host, or simply to reach the server over the network, the `address` in the server, agent and CLI configuration files needs to be switched from a `unix://` address to a network address (`<host>:<port>`).

!!! info

    For a quicker setup without certificates, intended only for isolated test environments such as VMs or containers set up for this purpose, see [Use network communication without TLS](#use-network-communication-without-tls) below. For anything else, secure the connection with mTLS as described next.

## Secure the connection with mTLS

 Mutual TLS (mTLS) is a security protocol that verifies both the client and server identities before establishing a connection. In Ankaios mTLS can be used to secure communication between the server, agent and ank CLI over the network.
 The CLI can also connect over the network without mTLS; see [Use network communication without TLS](#use-network-communication-without-tls) below.

 This section describes how to switch to network communication, create certificates and **enable mTLS**.

### Prerequisites

- OpenSSL 3.0 or newer

### Set up directories

To set up mTLS with OpenSSL, perform the following actions:

First we need to create a folder to keep certificates and keys for `ank-server` and `ank-agent`:

```shell
sudo mkdir -p /etc/ankaios/certs
```

Then we need to create a folder to keep certificates and keys for the `ank` CLI:

```shell
mkdir -p "$HOME/.config/ankaios"
```

### Generate CA keys and certificate

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ca.cnf`. You are welcome to include additional fields if necessary:

```ini title="ca.cnf"
[req]
distinguished_name = req_distinguished_name
prompt = no

[req_distinguished_name]
CN = ankaios-ca
```

Generate CA key:

```shell
sudo openssl genpkey -algorithm ED25519 -out "./ca-key.pem"
```

Generate CA certificate:

```shell
sudo openssl req -config "./ca.cnf" -new -x509 -key "./ca-key.pem" -out "/etc/ankaios/certs/ca.pem"
```

### Generate key and certificate for `ank-server`

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ank-server.cnf`. You are welcome to include additional fields if necessary:

```ini title="ank-server.cnf"
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = ank-server

[v3_req]
subjectAltName = @alt_names
extendedKeyUsage = serverAuth

[alt_names]
DNS.1 = ank-server
```

Generate ank-server key:

```shell
sudo openssl genpkey -algorithm ED25519 -out "/etc/ankaios/certs/ank-server-key.pem"
```

Generate ank-server certificate signing request:

```shell
sudo openssl req -config "./ank-server.cnf" -new -key "/etc/ankaios/certs/ank-server-key.pem" -out "./ank-server.csr"
```

Generate ank-server certificate:

```shell
sudo openssl x509 -req -in "./ank-server.csr" -CA "/etc/ankaios/certs/ca.pem" -CAkey "./ca-key.pem" -extensions v3_req -extfile "./ank-server.cnf" -out "/etc/ankaios/certs/ank-server.pem"
```

### Generate key and certificate for `ank-agent`

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ank-agent.cnf`. You are welcome to include additional fields if necessary:

```ini title="ank-agent.cnf"
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = ank-agent

[v3_req]
subjectAltName = @alt_names
extendedKeyUsage = clientAuth

[alt_names]
# This certificate can only be used for agents with the names 'agent_A' or 'agent_B'
# To allow the usage for any agent use the character '*'
# like: DNS.1 = *
DNS.1 = agent_A
DNS.2 = agent_B

```

!!! note

    This example shares one certificate between `agent_A` and `agent_B` to keep the tutorial short; for production use, issue a separate certificate per agent, each with a single `DNS.1` entry matching the agent's `--name`, so that an agent cannot connect under another agent's name.

Generate ank-agent key:

```shell
sudo openssl genpkey -algorithm ED25519 -out "/etc/ankaios/certs/ank-agent-key.pem"
```

Generate ank-agent certificate signing request:

```shell
sudo openssl req -config "./ank-agent.cnf" -new -key "/etc/ankaios/certs/ank-agent-key.pem" -out "./ank-agent.csr"
```

Generate ank-agent certificate:

```shell
sudo openssl x509 -req -in "./ank-agent.csr" -CA "/etc/ankaios/certs/ca.pem" -CAkey "./ca-key.pem" -extensions v3_req -extfile "./ank-agent.cnf" -out "/etc/ankaios/certs/ank-agent.pem"
```

### Generate key and certificate for the CLI `ank`

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ank.cnf`. You are welcome to include additional fields if necessary:

```ini title="ank.cnf"
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no
[req_distinguished_name]
CN = ank

[v3_req]
subjectAltName = @alt_names
extendedKeyUsage = clientAuth

[alt_names]
DNS.1 = ank

```

Generate ank key:

```shell
openssl genpkey -algorithm ED25519 -out "$HOME/.config/ankaios/ank-key.pem"
```

Generate ank certificate signing request:

```shell
openssl req -config "./ank.cnf" -new -key "$HOME/.config/ankaios/ank-key.pem" -out "./ank.csr"
```

Generate ank certificate:

```shell
sudo openssl x509 -req -in "./ank.csr" -CA "/etc/ankaios/certs/ca.pem" -CAkey "./ca-key.pem" -extensions v3_req -extfile "./ank.cnf" -out "$HOME/.config/ankaios/ank.pem"
```

!!! warning

    All private key files (`*-key.pem`) should be readable only by their owner, e.g. with `chmod 600`.

### Switch to network communication with mTLS support

To set up Ankaios with mTLS support, you need to supply the necessary mTLS certificates to the already installed `ank-server`, `ank-agent`, and `ank` CLI components. Here's a step-by-step guide:

#### Configure `ank-server` and `ank-agent` with mTLS certificates

Replace the default `address` in `/etc/ankaios/ank-server.conf` with a network address and remove `socket_group`, which only applies to Unix domain sockets:

```toml
address = '0.0.0.0:25551'
```

To use the CA, certificate and key for the server, add the following lines to `/etc/ankaios/ank-server.conf` and set the `insecure` flag inside it to `false`:

```toml
ca_pem = '/etc/ankaios/certs/ca.pem'
crt_pem = '/etc/ankaios/certs/ank-server.pem'
key_pem = '/etc/ankaios/certs/ank-server-key.pem'
```

Similarly, switch `/etc/ankaios/ank-agent.conf` to the network address of the server, using the `https://` scheme, and set `insecure` to `false`:

```toml
address = 'https://127.0.0.1:25551'
insecure = false
```

For the agent add the following lines to `/etc/ankaios/ank-agent.conf`:

```toml
ca_pem = '/etc/ankaios/certs/ca.pem'
crt_pem = '/etc/ankaios/certs/ank-agent.pem'
key_pem = '/etc/ankaios/certs/ank-agent-key.pem'
```

For more information on how the server, agent and CLI can be configured please consult [configuration files](../reference/config-files.md).

Start the Ankaios server and an Ankaios agent as described in the [Quickstart](quickstart.md) and continue below to configure the CLI with mTLS.

#### Configure the `ank` CLI with mTLS certificates

To make it easier, we will set the mTLS certificates for the `ank` CLI by using environment variables:

```shell
export ANK_CA_PEM=/etc/ankaios/certs/ca.pem
export ANK_CRT_PEM=$HOME/.config/ankaios/ank.pem
export ANK_KEY_PEM=$HOME/.config/ankaios/ank-key.pem
```

Now you can use the `ank` CLI as follows:

```shell
ank get workloads
```

Or in a single line call:

```shell
ANK_CA_PEM=/etc/ankaios/certs/ca.pem ANK_CRT_PEM=$HOME/.config/ankaios/ank.pem ANK_KEY_PEM=$HOME/.config/ankaios/ank-key.pem ank get workloads
```

Alternatively, you can pass the mTLS certificates as command line arguments:

```shell
ank --ca_pem=/etc/ankaios/certs/ca.pem --crt_pem="$HOME/.config/ankaios/ank.pem" --key_pem="$HOME/.config/ankaios/ank-key.pem" get workloads
```

Or you can also configure the mTLS certificates in the CLI configuration file `~/.config/ankaios/ank.conf`.
In any case, make sure the `address` in there is set to the network address of the server (e.g. `https://127.0.0.1:25551`) and `insecure = false` is set to prevent a warning when mTLS certificates are provided.

## Use network communication without TLS

!!! warning

    Network communication without TLS is neither authenticated nor encrypted. Since `ank-server` typically runs as root (e.g. via `sudo systemctl`), **any program or user that can reach the port** - every local user on the machine, and every host on the network if the port is not otherwise firewalled - can act as an agent or CLI against the server and start workloads with root privileges. Only use network communication without TLS in isolated test environments such as VMs or containers set up for this purpose. For anything else, secure the connection with mTLS as described above.

Replace the default `address` in `/etc/ankaios/ank-server.conf` and remove `socket_group`, which only applies to Unix domain sockets:

```toml
address = '0.0.0.0:25551'
insecure = true
```

Replace the default `address` in `/etc/ankaios/ank-agent.conf` with the network address of the server and mark the connection as insecure:

```toml
address = 'http://<SERVER_IP>:25551'
insecure = true
```

Do the same in the CLI configuration file `~/.config/ankaios/ank.conf`, or instead pass `ank --insecure -s http://<SERVER_IP>:25551 get state` or set `export ANK_INSECURE=true` and `export ANK_SERVER_URL=http://<SERVER_IP>:25551`.

Restart the server and agent to apply the new configuration:

```shell
sudo systemctl restart ank-server
sudo systemctl restart ank-agent
```
