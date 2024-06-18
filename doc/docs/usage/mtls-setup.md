# Setting Up Ankaios with mTLS

 Mutual TLS (MTLS) is a security protocol that verifies both the client and server identities before establishing a connection. To set up MTLS with OpenSSL perform the following actions:

1. Generate CA keys and certificate
2. Generate keys and certificates for `ank-server`, `ank-agent` and `ank` (CLI).
3. Perform the Ankaios installation script `install.sh` with mTLS support.

## Generate CA keys and certificate

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ca.cnf`. You are welcome to include additional fields if necessary:

```ini
# Content of ca.cnf
[req]
distinguished_name = req_distinguished_name
prompt = no

[req_distinguished_name]
CN = ankaios-ca
```

Generate CA key:

```bash
openssl genpkey -algorithm ED25519 -out ".certs/ca-key.pem"
```

Generate CA certificate:

```bash
openssl req -config "./ca.cnf" -new -x509 -key ".certs/ca-key.pem" -out ".certs/ca.pem"
```

## Generate key and certificate for `ank-server`

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ank-server.cnf`. You are welcome to include additional fields if necessary:

```ini
# Content of ank-server.cnf
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

```bash
openssl genpkey -algorithm ED25519 -out ".certs/ank-server-key.pem"
```

Generate ank-server certificate signing request:

```bash
openssl req -config "./ank-server.cnf" -new -key ".certs/ank-server-key.pem" -out ".certs/ank-server.csr"
```

Generate ank-server certificate:

```bash
openssl x509 -req -in ".certs/server.csr" -CA ".certs/ca.pem" -CAkey ".certs/ca-key.pem" -extensions v3_req -extfile "./ank-server.cnf" -out ".certs/ank-server.pem"
```

## Generate key and certificate for `ank-agent`

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ank-agent.cnf`. You are welcome to include additional fields if necessary:

```ini
# Content of ank-agent.cnf
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

```bash
openssl genpkey -algorithm ED25519 -out ".certs/ank-agent-key.pem"
```

Generate ank-agent certificate signing request:

```bash
openssl req -config "./ank-agent.cnf" -new -key ".certs/ank-agent-key.pem" -out ".certs/ank-agent.csr"
```

Generate ank-agent certificate:

```bash
openssl x509 -req -in ".certs/ank-agent.csr" -CA ".certs/ca.pem" -CAkey ".certs/ca-key.pem" -extensions v3_req -extfile "./ank-agent.cnf" -out ".certs/ank-agent.pem"
```

## Generate key and certificate for the CLI `ank`

Construct an [OpenSSL configuration file](https://www.openssl.org/docs/manmaster/man5/config.html) named `ank.cnf`. You are welcome to include additional fields if necessary:

```ini
# Content of ank.cnf
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

```bash
openssl genpkey -algorithm ED25519 -out ".certs/ank-key.pem"
```

Generate ank certificate signing request:

```bash
openssl req -config "./ank.cnf" -new -key ".certs/ank-key.pem" -out ".certs/ank.csr"
```

Generate ank certificate:

```bash
openssl x509 -req -in ".certs/ank.csr" -CA ".certs/ca.pem" -CAkey ".certs/ca-key.pem" -extensions v3_req -extfile "./ank.cnf" -out ".certs/ank.pem"
```

## Perform the Ankaios installation script `install.sh` with mTLS support

TBD
