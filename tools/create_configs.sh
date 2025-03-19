#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ROOT_DIR="${WORKSPACE:-$(realpath -e "$SCRIPT_DIR/../")}"
CONFIG_FILES_NAME_BASE="ankaios_configs"
SERVER_CONFIG_FILE="${ROOT_DIR}/server/config/ank-server.conf"
AGENT_CONFIG_FILE="${ROOT_DIR}/agent/config/ank-agent.conf"
ANK_CONFIG_FILE="${ROOT_DIR}/ank/config/ank.conf"

tar -cvzf "${CONFIG_FILES_NAME_BASE}".tar.gz \
    -C "$(dirname "$SERVER_CONFIG_FILE")" "$(basename "$SERVER_CONFIG_FILE")" \
    -C "$(dirname "$AGENT_CONFIG_FILE")" "$(basename "$AGENT_CONFIG_FILE")" \
    -C "$(dirname "$ANK_CONFIG_FILE")" "$(basename "$ANK_CONFIG_FILE")"

echo "Creating checksums"
sha512sum "${CONFIG_FILES_NAME_BASE}".tar.gz > "${CONFIG_FILES_NAME_BASE}".tar.gz.sha512sum.txt

echo "Packaging config files for release '$CONFIG_FILES_NAME_BASE' finished."
