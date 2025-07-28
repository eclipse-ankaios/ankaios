#!/bin/bash
set -e

# Usage function
usage() {
    echo "Usage: $0 [--sdk-source <crates|github|local>] [--sdk-version <version>] [--sdk-branch <branch>]"
    echo ""
    echo "Options:"
    echo "  --help | -h        Show this help message and exit."
    echo "  --sdk-source       Source of the SDK (crates, github, local). Default: crates."
    echo "  --sdk-version      Version of the SDK (used with crates)."
    echo "  --sdk-branch       Branch to clone (used with github). Default: main."
}

# Default values
SDK_SOURCE="crates"
SDK_BRANCH="main"

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
RUST_SDK_DIR="ank-sdk-rust"
TOML_CONTENT=""

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
        *)
            usage
            exit 1
            ;;
    esac
done

# Install SDK
case $SDK_SOURCE in
    crates)
        if [ -z "$SDK_VERSION" ]; then
            echo "The version must be specified for crates source."
            exit 1
        fi
        echo "Using the Ankaios SDK version '$SDK_VERSION' from crates.io."
        TOML_CONTENT="ankaios_sdk = \"$SDK_VERSION\""
        ;;
    github)
        echo "Cloning Ankaios SDK from GitHub branch '$SDK_BRANCH'."
        git clone -b "$SDK_BRANCH" https://github.com/eclipse-ankaios/ank-sdk-rust.git
        TOML_CONTENT="ankaios_sdk = { path = \"${RUST_SDK_DIR}\" }"
        ;;
    local)
        if [ ! -d "$RUST_SDK_DIR" ]; then
            echo "Local SDK directory '$RUST_SDK_DIR' not found."
            exit 1
        fi
        TOML_CONTENT="ankaios_sdk = { path = \"${RUST_SDK_DIR}\" }"
        ;;
    *)
        echo "Invalid SDK source: $SDK_SOURCE"
        usage
        exit 1
        ;;
esac

# Update the Cargo.toml file
CARGO_TOML="${SCRIPT_DIR}/Cargo.toml"
if [ ! -f "$CARGO_TOML" ]; then
    echo "Cargo.toml file not found in the script directory."
    exit 1
fi
# Use sed to update the ankaios_sdk = line
sed -i "s/^# ankaios_sdk = .*/${TOML_CONTENT}/" "$CARGO_TOML"

echo "Setup completed successfully."
