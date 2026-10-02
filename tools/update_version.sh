#!/bin/bash

# Copyright (c) 2024 Elektrobit Automotive GmbH
#
# This program and the accompanying materials are made available under the
# terms of the Apache License, Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
#
# SPDX-License-Identifier: Apache-2.0

set -e

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
base_dir="$script_dir/.."
workspace_config="$base_dir/Cargo.toml"

usage() {
    echo "Usage: $0 --version VERSION [--sdk-version SDK_VERSION]"
    echo "Update Ankaios files to VERSION and optionally SDK files to SDK_VERSION."
    exit 1
}

log_update() {
    echo "Updating $(realpath -e --relative-base="$(pwd)" "$1")"
}

# Initialize variables
version=""
rust_sdk_version=""

# Parse arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        -h|--help) usage ;;
        --version|--sdk-version|--rust_sdk_version)
            if [[ "$#" -lt 2 || -z "$2" || "$2" == --* ]]; then
                echo "Error: $1 requires a value."
                usage
            fi
            case $1 in
                --version) version="$2" ;;
                --sdk-version|--rust_sdk_version) rust_sdk_version="$2" ;;
            esac
            shift 2
            ;;
        --version=*|--sdk-version=*|--rust_sdk_version=*)
            option="${1%%=*}"
            value="${1#*=}"
            if [[ -z "$value" ]]; then
                echo "Error: $option requires a value."
                usage
            fi
            case $option in
                --version) version="$value" ;;
                --sdk-version|--rust_sdk_version) rust_sdk_version="$value" ;;
            esac
            shift
            ;;
        *)
            echo "Error: Unknown parameter passed: $1"
            usage
            shift
            ;;
    esac
done

# Check if VERSION is set
if [[ -z "$version" ]]; then
    echo "Error: VERSION is a mandatory argument."
    usage
fi

# Extract all packages from the workspace file
packages=$(awk '/members *= *\[/{flag=1; next} /\]/{flag=0} flag {gsub(/[" ,]/, ""); print}' "$workspace_config")

for pkg in $packages; do
   package_config="$base_dir/$pkg/Cargo.toml"
   log_update "$package_config"
   # Update version in Cargo.toml for a specific package
   sed -i "/\[package\]/,/\[/{s/version = \"[^\"]*\"/version = \"$version\"/}" "$package_config"
done

# Update the version of the examples
examples=$(find "$base_dir/examples" -type d \( -name \*_control_interface -o -name python_sdk_\* -o -name rust_sdk_\* \) -printf "%f\n")
for example in $examples; do
    dockerfile="$base_dir/examples/$example/Dockerfile"
    log_update "$dockerfile"
    sed -i "s/^ENV ANKAIOS_VERSION=.*/ENV ANKAIOS_VERSION=${version}/" "$dockerfile"
    if [[ -n "$rust_sdk_version" ]]; then
        sed -i "s/^ENV ANKAIOS_SDK_VERSION=.*/ENV ANKAIOS_SDK_VERSION=${rust_sdk_version}/" "$dockerfile"
    fi
done
