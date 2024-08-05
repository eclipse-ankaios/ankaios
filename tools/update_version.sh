#!/bin/bash
set -e

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
base_dir="$script_dir/.."
workspace_config="$base_dir/Cargo.toml"

usage() {
    echo "Usage: $0 [--release] VERSION"
    echo "Update Ankaios files to VERSION."
    echo "  --release Official release with assets for download."
    exit 1
}

# Initialize variables
release=0
version=""

# Parse arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --release) release=1; shift ;;
        -h|--help) usage ;;
        *)
            if [[ -z "$version" ]]; then
                version="$1"
            else
                echo "Error: Unknown parameter passed: $1"
                usage
            fi
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
   echo "Updating $package_config"
   # Update version in Cargo.toml for a specific package
   sed -i "/\[package\]/,/\[/{s/version = \"[^\"]*\"/version = \"$version\"/}" "$package_config"
done

# Some versions must only be updated for official releases as only those provide assets for download
if [ "$release" = "1" ]; then
    # Update ankaios-docker
    sed -i "s/^ARG VERSION=.*/ARG VERSION=${version}/" "$base_dir/tools/ankaios-docker/agent/Dockerfile"
    sed -i "s/^ARG VERSION=.*/ARG VERSION=${version}/" "$base_dir/tools/ankaios-docker/server/Dockerfile"
fi



