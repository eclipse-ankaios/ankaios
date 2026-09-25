#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
AUR_REPO_BASE="${AUR_REPO_BASE:-ssh://aur@aur.archlinux.org}"

if [ -z "${ANKAIOS_VERSION}" ]; then
    echo "Error: ANKAIOS_VERSION environment variable is not set."
    exit 1
fi

tmp_dir="$(mktemp -d)"
echo "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT

for package in ankaios ankaios-bin ankaios-git; do
    git clone "$AUR_REPO_BASE/$package.git" "$tmp_dir/$package"

    cd "$tmp_dir/$package"
    git checkout -b master || true # AUR only accepts the master branch
    rm -rf -- *

    "$SCRIPT_DIR/build_package.sh" "$package" "$tmp_dir/$package"

    git add -A
    git commit -m "Update version to $ANKAIOS_VERSION" || true
    git push

    cd "$SCRIPT_DIR"
done
