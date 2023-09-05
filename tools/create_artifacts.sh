#!/bin/bash
set -e

display_usage() {  
    echo -e "Usage: $0 -p"
    echo -e "Prepare Ankaios release."
    echo -e "  -p: System platform, e.g. linux-amd64.\n"
}

fail() {
    display_usage
    if [ $# -eq 1 ]; then
        echo -e "$1"
    fi
    exit 1
}

# parse script args
while getopts p: opt; do
    case $opt in
        p) RELEASE_ARCHITECTURE="${OPTARG}";;
        *)
            fail "Error: Invalid parameter, aborting"
        ;;
    esac
done

# in case of missing required args, fail
if [ -z "$RELEASE_ARCHITECTURE" ] ; then
    fail
fi

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ROOT_DIR="${WORKSPACE:-$(realpath -e "$SCRIPT_DIR/../")}"
DIST_DIR="${ROOT_DIR}/dist/${RELEASE_ARCHITECTURE}"
ANK_BIN_DIR="${DIST_DIR}/bin"
RELEASE_FILE_NAME_BASE="ankaios-${RELEASE_ARCHITECTURE}"

echo "Creating the archive for '$RELEASE_FILE_NAME_BASE'"
cd "${DIST_DIR}"
chmod +x ank{,-server,-agent}
tar -cvzf "${RELEASE_FILE_NAME_BASE}".tar.gz --directory=${ANK_BIN_DIR} $(ls "${ANK_BIN_DIR}")

echo "Creating checksums"
sha512sum "${RELEASE_FILE_NAME_BASE}".tar.gz > "${RELEASE_FILE_NAME_BASE}".tar.gz.sha512sum.txt

echo "Packaging artifacts for release '$RELEASE_FILE_NAME_BASE' finished."
