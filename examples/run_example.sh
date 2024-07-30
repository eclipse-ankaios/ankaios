#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ANKAIOS_SERVER_SOCKET="0.0.0.0:25551"
ANKAIOS_SERVER_URL="http://${ANKAIOS_SERVER_SOCKET}"
DEFAULT_ANKAIOS_BIN_PATH="/usr/local/bin"

display_usage() {
    echo -e "Usage: $0 EXAMPLE"
    echo -e "Build and run a control interface example."
    echo -e "  EXAMPLE: subfolder of the example, e.g. rust_control_interface"
    echo -e "Optionally, set environment variable for alternative Ankaios executable path: export ANK_BIN_DIR=/path/to/ankaios/executables, if not set default path: '${DEFAULT_ANKAIOS_BIN_PATH}'"
}


run_ankaios() {
  ANKAIOS_LOG_DIR="/tmp/"
  mkdir -p ${ANKAIOS_LOG_DIR}

  # Start the Ankaios server
  echo "Starting Ankaios server located in '${ANK_BIN_DIR}'."
  RUST_LOG=debug ${ANK_BIN_DIR}/ank-server --insecure --startup-config ${SCRIPT_DIR}/config/startupState.yaml --address ${ANKAIOS_SERVER_SOCKET} > ${ANKAIOS_LOG_DIR}/ankaios-server.log 2>&1 &

  sleep 2
  # Start an Ankaios agent
  echo "Starting Ankaios agent agent_A located in '${ANK_BIN_DIR}'."
  RUST_LOG=debug ${ANK_BIN_DIR}/ank-agent --insecure --name agent_A --server-url ${ANKAIOS_SERVER_URL} > ${ANKAIOS_LOG_DIR}/ankaios-agent_A.log 2>&1 &

  # Wait for any process to exit
  wait -n

  # Exit with status of process that exited first
  exit $?
}

if [ -z $1 ]; then
  display_usage
  exit 1
fi

if [ -z ${ANK_BIN_DIR} ]; then
  ANK_BIN_DIR=${DEFAULT_ANKAIOS_BIN_PATH}
fi

ANK_BIN_DIR=${ANK_BIN_DIR%/} # remove trailing / if there is one

if [[ ! -f ${ANK_BIN_DIR}/ank-server || ! -f ${ANK_BIN_DIR}/ank-agent ]]; then
  echo "Failed to build and execute example: no Ankaios executables inside '${ANK_BIN_DIR}'."
  display_usage
  exit 2
fi

echo Build control interface example ...
podman build "${@:2}" -t control_interface_prod:0.1 -f examples/$1/Dockerfile ${SCRIPT_DIR}/../
echo done.

if pgrep -x "ank-server" >/dev/null
then
  echo -e "\nAbort startup. Ankaios server is already running."
  echo "Shutdown the Ankaios server instance manually or"
  echo -e "if 'run_example.sh' was executed previously,\nexecute 'shutdown_example.sh' afterwards to stop the example."
  exit 3
fi

run_ankaios &

