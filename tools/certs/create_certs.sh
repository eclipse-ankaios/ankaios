#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ROOT_DIR="${WORKSPACE:-$(realpath -e "$SCRIPT_DIR/../../")}"
CONFIGS_DIR="$SCRIPT_DIR/config"
CERTS_OUT_DIR="${1:-$ROOT_DIR/.certs}"

mkdir -p "$CERTS_OUT_DIR"

echo "Generate CA certificates ..."
openssl genpkey -algorithm ED25519 -out "$CERTS_OUT_DIR/ca-key.pem"
openssl req -config "$CONFIGS_DIR/ca.cnf" -new -x509 -key "$CERTS_OUT_DIR/ca-key.pem" -out "$CERTS_OUT_DIR/ca.pem"

echo "Generate ank-server certificates ..."
openssl genpkey -algorithm ED25519 -out "$CERTS_OUT_DIR/server-key.pem"
openssl req -config "$CONFIGS_DIR/server.cnf" -new -key "$CERTS_OUT_DIR/server-key.pem" -out "$CERTS_OUT_DIR/server.csr"
openssl x509 -req -in "$CERTS_OUT_DIR/server.csr" -CA "$CERTS_OUT_DIR/ca.pem" -CAkey "$CERTS_OUT_DIR/ca-key.pem" -extensions v3_req -extfile "$CONFIGS_DIR/server.cnf" -out "$CERTS_OUT_DIR/server.pem"

echo "Generate ank-agent certificates ..."
openssl genpkey -algorithm ED25519 -out "$CERTS_OUT_DIR/agent-key.pem"
openssl req -config "$CONFIGS_DIR/agent.cnf" -new -key "$CERTS_OUT_DIR/agent-key.pem" -out "$CERTS_OUT_DIR/agent.csr"
openssl x509 -req -in "$CERTS_OUT_DIR/agent.csr" -CA "$CERTS_OUT_DIR/ca.pem" -CAkey "$CERTS_OUT_DIR/ca-key.pem" -extensions v3_req -extfile "$CONFIGS_DIR/agent.cnf" -out "$CERTS_OUT_DIR/agent.pem"

echo "Generate ank-cli certificates ..."
openssl genpkey -algorithm ED25519 -out "$CERTS_OUT_DIR/cli-key.pem"
openssl req -config "$CONFIGS_DIR/cli.cnf" -new -key "$CERTS_OUT_DIR/cli-key.pem" -out "$CERTS_OUT_DIR/cli.csr"
openssl x509 -req -in "$CERTS_OUT_DIR/cli.csr" -CA "$CERTS_OUT_DIR/ca.pem" -CAkey "$CERTS_OUT_DIR/ca-key.pem" -extensions v3_req -extfile "$CONFIGS_DIR/cli.cnf" -out "$CERTS_OUT_DIR/cli.pem"

chmod 600 $CERTS_OUT_DIR/*.pem
