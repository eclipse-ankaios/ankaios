#!/bin/bash
set -e

# Usage function
usage() {
    echo "Usage: $0 [--sdk-source <pypi|github|local>] [--sdk-version <version>] [--sdk-branch <branch>] [--proto-source <default|branch|local>] [--proto-branch <branch>] [--proto-path <path>]"
    echo ""
    echo "If the installation is made from Pypi, the proto files will be fetched during the install with no possibility to change them."
    echo ""
    echo "Options:"
    echo "  --help | -h        Show this help message and exit."
    echo "  --sdk-source       Source of the SDK (pypi, github, local). Default: pypi."
    echo "  --sdk-version      Version of the SDK (used with pypi)."
    echo "  --sdk-branch       Branch to clone (used with github). Default: main."
    echo "  --proto-source     Source of the proto files (branch, local). By default the files are fetched automatically with the SDK."
    echo "  --proto-branch     Ankaios branch to fetch proto files (used with branch)."
    echo "  --proto-path       Path to local proto files (used with local)."
}

# Default values
SDK_SOURCE="pypi"
SDK_BRANCH="main"
PROTO_SOURCE="default"

PYTHON_SDK_DIR="ank-sdk-python"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            usage
            exit 0
            ;;
        --sdk-source)
            SDK_SOURCE="$2"
            shift 2
            ;;
        --sdk-version)
            SDK_VERSION="$2"
            shift 2
            ;;
        --sdk-branch)
            SDK_BRANCH="$2"
            shift 2
            ;;
        --proto-source)
            PROTO_SOURCE="$2"
            shift 2
            ;;
        --proto-branch)
            PROTO_BRANCH="$2"
            shift 2
            ;;
        --proto-path)
            PROTO_PATH="$2"
            shift 2
            ;;
        *)
            usage
            exit 1
            ;;
    esac
done

# Check arguments
if [[ $SDK_SOURCE == "pypi" && -n $PROTO_SOURCE && $PROTO_SOURCE != "default" ]]; then
    echo "Proto files cannot be specified when installing from PyPI. They will be fetched during the installation."
    exit 1
fi

# Install SDK
case $SDK_SOURCE in
    pypi)
        if [ -z "$SDK_VERSION" ]; then
            echo "Installing the latest version of the Ankaios SDK from PyPI."
            python3 -m pip install "ankaios-sdk"
        else
            echo "Installing Ankaios SDK version $SDK_VERSION from PyPI."
            python3 -m pip install "ankaios-sdk==$SDK_VERSION"
        fi
        ;;
    github)
        echo "Cloning Ankaios SDK from GitHub branch '$SDK_BRANCH'."
        git clone -b "$SDK_BRANCH" https://github.com/eclipse-ankaios/ank-sdk-python.git
        ;;
    local)
        if [ ! -d "$PYTHON_SDK_DIR" ]; then
            echo "Local SDK directory '$PYTHON_SDK_DIR' not found."
            exit 1
        fi
        ;;
    *)
        echo "Invalid SDK source: $SDK_SOURCE"
        usage
        exit 1
        ;;
esac

# Handle proto files
if [[ -n $PROTO_SOURCE ]]; then
    case $PROTO_SOURCE in
        branch)
            if [ -z "$PROTO_BRANCH" ]; then
                echo "Proto branch must be specified for branch source."
                exit 1
            fi
            mkdir -p ank-sdk-python/ankaios_sdk/_protos/0.6.0

            # Get ank_base proto file
            PROTO_LINK="https://raw.githubusercontent.com/eclipse-ankaios/ankaios/refs/heads/${PROTO_BRANCH}/api/proto/ank_base.proto"
            curl -s "$PROTO_LINK" | grep -v "^\s*//" | grep -v "^\s*$" > ank-sdk-python/ankaios_sdk/_protos/0.6.0/ank_base.proto
            if [ $? -ne 0 ]; then
                echo "Failed to download or process the ank_base.proto file."
                exit 1
            fi

            # Get the control_api proto file
            PROTO_LINK="https://raw.githubusercontent.com/eclipse-ankaios/ankaios/refs/heads/${PROTO_BRANCH}/api/proto/control_api.proto"
            curl -s "$PROTO_LINK" | grep -v "^\s*//" | grep -v "^\s*$" > ank-sdk-python/ankaios_sdk/_protos/0.6.0/control_api.proto
            if [ $? -ne 0 ]; then
                echo "Failed to download or process the control_api.proto file."
                exit 1
            fi
            ;;
        local)
            if [ -z "$PROTO_PATH" ]; then
                echo "Proto path must be specified for local source."
                exit 1
            fi
            mkdir -p ank-sdk-python/ankaios_sdk/_protos/0.6.0
            if [ -f "$PROTO_PATH"/ank_base.proto ]; then
                cp "$PROTO_PATH"/ank_base.proto ank-sdk-python/ankaios_sdk/_protos/0.6.0/
            fi
            if [ -f "$PROTO_PATH"/control_api.proto ]; then
                cp "$PROTO_PATH"/control_api.proto ank-sdk-python/ankaios_sdk/_protos/0.6.0/
            fi
            ;;
        default)
            ;;
        *)
            echo "Invalid proto source: $PROTO_SOURCE"
            usage
            exit 1
            ;;
    esac
fi

# Install the sdk if not already installed
if [[ $SDK_SOURCE != "pypi" ]]; then
    cd $PYTHON_SDK_DIR || { echo "Failed to change directory to $PYTHON_SDK_DIR. Exiting."; exit 1; }
    python3 -m pip install .
    if [ $? -ne 0 ]; then
        echo "Failed to install the Ankaios SDK."
        exit 1
    fi
fi

echo "Setup completed successfully."
