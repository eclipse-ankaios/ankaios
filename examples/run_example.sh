#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ANKAIOS_SERVER_SOCKET="0.0.0.0:25551"
ANKAIOS_SERVER_URL="http://${ANKAIOS_SERVER_SOCKET}"
DEFAULT_ANKAIOS_BIN_PATH="/usr/local/bin"
MANIFEST_FILE="startupState.yaml"

display_usage() {
  echo -e "Usage: $0 <name> [--ankaios-bin-dir <path>] [additional podman build args]"
  echo -e ""
  echo -e "Build and run a control interface application."
  echo -e "  name:              subfolder of the example, e.g. rust_control_interface"
  echo -e "  --help | -h:       display this help message and exit."
  echo -e "  --ankaios-bin-dir: specify the path to the Ankaios executables. This can be set using the ANK_BIN_DIR env var as well. Default is '${DEFAULT_ANKAIOS_BIN_PATH}'."
  echo -e "Optionally, any additional arguments will be passed to the podman build command."
  echo -e ""
  echo -e "Example: $0 rust_control_interface --manifest-file my_manifest.yaml --ankaios-bin-dir /path/to/ankaios/bin --target x86_64-unknown-linux-gnu"
}

parse_args() {
  # Default values
  ANK_BIN_DIR=${ANK_BIN_DIR:-$DEFAULT_ANKAIOS_BIN_PATH}
  PODMAN_BUILD_ARGS=""

  EXAMPLE=$1
  shift

  if [ "$EXAMPLE" = "--help" ] || [ "$EXAMPLE" = "-h" ]; then
    display_usage
    exit 0
  fi

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case $1 in
      --ankaios-bin-dir)
        ANK_BIN_DIR="$2"
        shift 2
        ;;
      --help|-h)
        display_usage
        exit 0
        ;;
      *)
        PODMAN_BUILD_ARGS="${PODMAN_BUILD_ARGS} $1"
        shift
        ;;
    esac
  done

  ANK_BIN_DIR=${ANK_BIN_DIR%/}  # Remove trailing slash
}


run_ankaios() {
  ANKAIOS_LOG_DIR="/tmp/"
  mkdir -p ${ANKAIOS_LOG_DIR}

  if pgrep -x "ank-server" >/dev/null
  then
    echo "Ankaios is already running, this example will be started over the existing cluster."
    echo "If this was not intended, make sure to clean up Ankaios by running 'cleanup.sh'"

    if ! pgrep -x "ank-agent" >/dev/null
    then
      echo "Agent seems not to be running. Something went wrong with it. Aborting run.."
      exit 3
    fi

    # Apply a new manifest file
    ${ANK_BIN_DIR}/ank --insecure apply ${SCRIPT_DIR}/${EXAMPLE}/${MANIFEST_FILE}
  else
    # Start the Ankaios server
    echo "Starting Ankaios server located in '${ANK_BIN_DIR}'."
    RUST_LOG=debug ${ANK_BIN_DIR}/ank-server --insecure --startup-config ${SCRIPT_DIR}/${EXAMPLE}/${MANIFEST_FILE} --address ${ANKAIOS_SERVER_SOCKET} > ${ANKAIOS_LOG_DIR}/ankaios-server.log 2>&1 &

    sleep 2
    # Start an Ankaios agent
    echo "Starting Ankaios agent agent_A located in '${ANK_BIN_DIR}'."
    RUST_LOG=debug ${ANK_BIN_DIR}/ank-agent --insecure --name agent_A --server-url ${ANKAIOS_SERVER_URL} > ${ANKAIOS_LOG_DIR}/ankaios-agent_A.log 2>&1 &

    # Wait for any process to exit
    wait -n

    # Exit with status of process that exited first
    exit $?
  fi
}

if [ -z $1 ]; then
  display_usage
  exit 1
fi

parse_args "$@"

if [[ ! -f ${ANK_BIN_DIR}/ank-server || ! -f ${ANK_BIN_DIR}/ank-agent ]]; then
  echo "Failed to build and execute example: no Ankaios executables inside '${ANK_BIN_DIR}'."
  display_usage
  exit 2
fi

if [ -z "${PODMAN_BUILD_ARGS}" ]; then
  echo "Build control interface example ..."
else
  echo "Build control interface example with: ${PODMAN_BUILD_ARGS} ..."
fi
podman build ${PODMAN_BUILD_ARGS} -t ${EXAMPLE}:0.1 -f examples/"${EXAMPLE}"/Dockerfile "${SCRIPT_DIR}"/../
echo done.

run_ankaios &

