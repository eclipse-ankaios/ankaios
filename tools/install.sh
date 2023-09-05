#!/bin/bash

set -e

# GITHUB RELEASE URL SCHEMA for concrete release artifact: https://github.com/<organisation>/<repo>/releases/download/<tag>/<concrete_artifact>
# GITHUB RELEASE URL SCHEMA for latest release artifact: https://github.com/<organisation>/<repo>/releases/latest/download/<concrete_artifact> (takes the release marked as latest)
RELEASE_URL_BASE="https://github.com/eclipse-ankaios/ankaios/releases"
DEFAULT_BIN_DESTINATION="/usr/local/bin/"
BIN_DESTINATION="${DEFAULT_BIN_DESTINATION}"

setup_verify_arch() {
    if [ -z "$ARCH" ]; then
        ARCH=$(uname -m)
    fi
    case $ARCH in
        amd64|x86_64)
            ARCH=amd64;;
        arm64|aarch64)
            ARCH=arm64;;
        *)
            fail "Unsupported architecture '${ARCH}'."
    esac

    if [ -z "$OS_NAME" ]; then
        OS_NAME=$(uname -s | tr '[:upper:]' '[:lower:]')
    fi
    case $OS_NAME in
        linux) ;;
        *)
            fail "Unsupported OS kernel type '${OS_NAME}'"
    esac
}

display_usage() {  
    echo -e "Usage: $0 [-v] [-i]"
    echo -e "Install Ankaios on a system."
    echo -e "  -v: Ankaios specific version to install. Default: latest version."
    echo -e "  -i: Installation path. Default: ${DEFAULT_BIN_DESTINATION}\n"
}

fail() {
    display_usage >&2
    if [ $# -eq 1 ]; then
        echo -e "$1"
    fi
    exit 1
}

download_release() {
    if ! curl -sfLO "$1"; then
        fail "Error: download failed. No resource under '$1'"
    fi
}

cleanup_routine() {
    if [ -d "${ANKAIOS_TMP_DIR}" ]; then
        rm -rf "${ANKAIOS_TMP_DIR}"
    fi
}

trap cleanup_routine EXIT

# parse script args
while getopts v:i: opt; do
    case $opt in
        v) ANKAIOS_VERSION="$OPTARG";;
        i) BIN_DESTINATION="$OPTARG";;
        *)
            fail "Error: Invalid parameter, aborting"
        ;;
    esac
done

# Use absolute path for tar -C option otherwise relative paths as script argument are failing on tar extraction
case $BIN_DESTINATION in 
    /*) ;;
    *) BIN_DESTINATION="$(pwd)/${BIN_DESTINATION}";;
esac

# fail if default or custom installation dir does not exist
if [ ! -d "${BIN_DESTINATION}" ]; then
    fail "Error: installation path '${BIN_DESTINATION}' does not exist."
fi

setup_verify_arch
SUFFIX="${OS_NAME}-${ARCH}"
echo "Platform: $SUFFIX"

RELEASE_FILE_NAME="ankaios-${SUFFIX}.tar.gz"
RELEASE_FILE_NAME_WITH_SHA="${RELEASE_FILE_NAME}.sha512sum.txt"

echo "Ankaios version: ${ANKAIOS_VERSION}"

# in case of missing version, download latest
if [ -z "$ANKAIOS_VERSION" ] ; then
    echo "No version provided, use default: latest"
    ANKAIOS_RELEASE_URL="${RELEASE_URL_BASE}/latest/download/${RELEASE_FILE_NAME}"
    ANKAIOS_RELEASE_URL_SHA="${RELEASE_URL_BASE}/latest/download/${RELEASE_FILE_NAME_WITH_SHA}"
else
    echo "Version provided, use version '${ANKAIOS_VERSION}'"
    ANKAIOS_RELEASE_URL="${RELEASE_URL_BASE}/download/${ANKAIOS_VERSION}/${RELEASE_FILE_NAME}"
    ANKAIOS_RELEASE_URL_SHA="${RELEASE_URL_BASE}/download/${ANKAIOS_VERSION}/${RELEASE_FILE_NAME_WITH_SHA}"
fi

ANKAIOS_TMP_DIR=$(mktemp -d)
echo "Creating tmp directory for download artifacts: '${ANKAIOS_TMP_DIR}'"
cd "${ANKAIOS_TMP_DIR}"

echo "Downloading the release: '${ANKAIOS_RELEASE_URL}'"
download_release "${ANKAIOS_RELEASE_URL_SHA}"
download_release "${ANKAIOS_RELEASE_URL}"

# skip checksum validation if sha512sum is not available
if command -v sha512sum >/dev/null; then
    echo "Checking file checksum"
    sha512sum -c "${RELEASE_FILE_NAME_WITH_SHA}"
else
    echo "Warning: 'sha512sum' not installed. Skipping checksum validation."
fi

# prefix with sudo if install dir is not writeable with current permissions
PREFIX="sudo"
if [ -w "${BIN_DESTINATION}" ]; then
    PREFIX=""
fi

echo "Extracting the binaries into install folder: '${BIN_DESTINATION}'"
${PREFIX} tar -xvzf "${RELEASE_FILE_NAME}" -C "${BIN_DESTINATION}"
${PREFIX} chmod +x "${BIN_DESTINATION}"/ank{,-agent,-server}

echo "Installation has finished."
