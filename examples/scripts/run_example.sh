#!/bin/bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ANKAIOS_SERVER_SOCKET="0.0.0.0:25551"
ANKAIOS_SERVER_URL="http://${ANKAIOS_SERVER_SOCKET}"
NEED_PODMAN_SERVICE=$(ank --version | grep '0.1'| wc -l)

run_ankaios() {
  ANKAIOS_LOG_DIR="/var/log"
  mkdir -p ${ANKAIOS_LOG_DIR}

  # For Ankaios version < 0.2 podman service is needed!
  if [ $NEED_PODMAN_SERVICE -eq "1" ]; then
    if [ $(ps aux | grep 'podman system service'| wc -l) -eq "1" ]; then
      echo "podman service not running -> start podman service"
      podman system service --time=0 unix:///tmp/podman.sock &
      t=0
      until [ -e /tmp/podman.sock ] || (( t++ >= 10 )); do
        sleep 1
      done
      [ -e /tmp/podman.sock ] && echo /tmp/podman.sock created. || echo /tmp/podman.sock not found.
    else
      echo "podman service is already running"
    fi
  fi

  # Start the Ankaios server
  echo "Starting Ankaios server"
  ank-server --startup-config ${SCRIPT_DIR}/../config/startupState.yaml --address ${ANKAIOS_SERVER_SOCKET} > ${ANKAIOS_LOG_DIR}/ankaios-server.log 2>&1 &

  sleep 2
  # Start an Ankaios agent
  echo "Starting Ankaios agent agent_A"
  if [ $NEED_PODMAN_SERVICE -eq "1" ]; then
    ank-agent --name agent_A --server-url ${ANKAIOS_SERVER_URL} -p /tmp/podman.sock > ${ANKAIOS_LOG_DIR}/ankaios-agent_A.log 2>&1 &
  else
    ank-agent --name agent_A --server-url ${ANKAIOS_SERVER_URL} > ${ANKAIOS_LOG_DIR}/ankaios-agent_A.log 2>&1 &
  fi

  # Wait for any process to exit
  wait -n

  # Exit with status of process that exited first
  exit $?
}

echo Build control interface example ...
podman build -t control_interface_prod:0.1 -f .devcontainer/Dockerfile .
echo done.

run_ankaios &

