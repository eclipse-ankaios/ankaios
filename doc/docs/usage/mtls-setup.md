# Setting up Ankaios with mTLS

 Mutual TLS (mTLS) is a security protocol that verifies both the client and server identities before establishing a connection. In Ankaios mTLS can be used to secure communication between the server, agent and ank CLI.

## Prerequisites

- OpenSSL 3.0 or newer

## Set up directories

To set up mTLS with OpenSSL, perform the following actions:

First we need to create a folder to keep certificates and keys for `ank-server` and `ank-agent`:

```shell
sudo mkdir -p /etc/ankaios/certs
```

Then we need to create a folder to keep certificates and keys for the `ank` CLI:

```shell
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/ankaios"
```

## Generate CA keys and certificate

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

## Generate key and certificate for `ank-server`

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

## Generate key and certificate for `ank-agent`

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

## Generate key and certificate for the CLI `ank`

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
openssl genpkey -algorithm ED25519 -out "${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank-key.pem"
```

Generate ank certificate signing request:

```shell
openssl req -config "./ank.cnf" -new -key "${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank-key.pem" -out "./ank.csr"
```

Generate ank certificate:

```shell
sudo openssl x509 -req -in "./ank.csr" -CA "/etc/ankaios/certs/ca.pem" -CAkey "./ca-key.pem" -extensions v3_req -extfile "./ank.cnf" -out "${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank.pem"
```

## Perform Ankaios installation with mTLS support

To set up Ankaios with mTLS support, you need to supply the necessary mTLS certificates to the `ank-server`, `ank-agent`, and `ank` CLI components. Here's a step-by-step guide:

### Install `ank-server` and `ank-agent` with mTLS certificates

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -s -- -s "--startup-config /etc/ankaios/state.yaml --ca_pem /etc/ankaios/certs/ca.pem --crt_pem /etc/ankaios/certs/ank-server.pem --key_pem /etc/ankaios/certs/ank-server-key.pem" -a "--name agent_A --ca_pem /etc/ankaios/certs/ca.pem --crt_pem /etc/ankaios/certs/ank-agent.pem --key_pem /etc/ankaios/certs/ank-agent-key.pem"
```

Start the Ankaios server and an Ankaios agent as described in the [Quickstart](quickstart.md) and continue below to configure the CLI with mTLS.

### Configure the `ank` CLI with mTLS certificates

To make it easier, we will set the mTLS certificates for the `ank` CLI by using environment variables:

```shell
export ANK_CA_PEM=/etc/ankaios/certs/ca.pem
export ANK_CRT_PEM=${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank.pem
export ANK_KEY_PEM=${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank-key.pem
```

Now you can use the `ank` CLI as follows:

```shell
ank get workloads
```

Or in a single line call:

```shell
ANK_CA_PEM=/etc/ankaios/certs/ca.pem ANK_CRT_PEM=${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank.pem ANK_KEY_PEM=${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank-key.pem ank get workloads
```

Alternatively, you can pass the mTLS certificates as command line arguments:

```shell
ank --ca_pem=/etc/ankaios/certs/ca.pem --crt_pem="${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank.pem" --key_pem="${XDG_CONFIG_HOME:-$HOME/.config}/ankaios/ank-key.pem" get workloads
```
