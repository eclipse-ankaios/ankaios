# Setting Up Ankaios with mTLS

 Mutual TLS (MTLS) is a security protocol that verifies both the client and server identities before establishing a connection. To set up MTLS with OpenSSL perform the following actions:

1. Generate CA key and certificate
2. Generate keys and certificates for `ank-server`, `ank-agent` and `ank`
3. Perform Ankaios installation with mTLS support

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

```shell
openssl genpkey -algorithm ED25519 -out ".certs/ca-key.pem"
```

Generate CA certificate:

```shell
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

```shell
openssl genpkey -algorithm ED25519 -out ".certs/ank-server-key.pem"
```

Generate ank-server certificate signing request:

```shell
openssl req -config "./ank-server.cnf" -new -key ".certs/ank-server-key.pem" -out ".certs/ank-server.csr"
```

Generate ank-server certificate:

```shell
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

```shell
openssl genpkey -algorithm ED25519 -out ".certs/ank-agent-key.pem"
```

Generate ank-agent certificate signing request:

```shell
openssl req -config "./ank-agent.cnf" -new -key ".certs/ank-agent-key.pem" -out ".certs/ank-agent.csr"
```

Generate ank-agent certificate:

```shell
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

```shell
openssl genpkey -algorithm ED25519 -out ".certs/ank-key.pem"
```

Generate ank certificate signing request:

```shell
openssl req -config "./ank.cnf" -new -key ".certs/ank-key.pem" -out ".certs/ank.csr"
```

Generate ank certificate:

```shell
openssl x509 -req -in ".certs/ank.csr" -CA ".certs/ca.pem" -CAkey ".certs/ca-key.pem" -extensions v3_req -extfile "./ank.cnf" -out ".certs/ank.pem"
```

## Perform Ankaios installation with mTLS support

To set up Ankaios with mutual TLS (mTLS) support, you need to supply the necessary mTLS certificates to the `ank-server`, `ank-agent`, and `ank` CLI components. Here's a step-by-step guide:

### Install the `ank-server` with mTLS certificates

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -s -- -s "--ankserver_ca_pem ./certs/ca.pem --ankserver_crt_pem ./certs/ank-server.pem --ankserver_key_pem ./certs/ank-server-key.pem"
```

### Install the `ank-agent` with mTLS certificates

```shell
curl -sfL https://github.com/eclipse-ankaios/ankaios/releases/latest/download/install.sh | bash -s -- -s "--ankagent_ca_pem ./certs/ca.pem --ankagent_crt_pem ./certs/ank-agent.pem --ankagent_key_pem ./certs/ank-agent-key.pem"
```

### Configure the `ank` CLI with mTLS certificates

To make it easier, we will set the mTLS certificates for the `ank` CLI by using environment variables:

```shell
export ANK_CA_PEM=./.certs/ca.pem ANK_CRT_PEM=./.certs/ank.pem ANK_KEY_PEM=./.certs/ank-key.pem
```

Now you can use the `ank` CLI as follows:

```shell
ank get workloads
```

Or in a single line call:

```shell
ANK_CA_PEM=./.certs/ca.pem ANK_CRT_PEM=./.certs/ank.pem ANK_KEY_PEM=./.certs/ank-key.pem ank get workloads
```

Alternatively, you can pass the mTLS certificates as command line arguments:

```shell
ank --ank_ca_pem=./.certs/ca.pem --ank_crt_pem=./.certs/ank.pem --ank_key_pem=./.certs/ank-key.pem get workloads
```
