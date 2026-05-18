#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
AUR_REPO_BASE="${AUR_REPO_BASE:-ssh://aur@aur.archlinux.org}"
ASSETS=("ank-server.service" "ank-agent.service" "ankaios-cli.install")

if [ -z "${ANKAIOS_VERSION}" ]; then
    echo "Error: ANKAIOS_VERSION environment variable is not set."
    exit 1
fi

tmp_dir="$(mktemp -d)"
echo "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT

cd "$SCRIPT_DIR"
for package in ankaios ankaios-bin ankaios-git; do
    git clone "$AUR_REPO_BASE/$package.git" "$tmp_dir/$package"

    cd "$tmp_dir/$package"
    git checkout -b master || true # AUR only accepts the master branch
    rm -rf -- *

    m4 -D ANKAIOS_VERSION="$ANKAIOS_VERSION" "$SCRIPT_DIR/PKGBUILD-$package.m4" > PKGBUILD
    cp -t ./ "${ASSETS[@]/#/$SCRIPT_DIR/}"
    updpkgsums
    makepkg --printsrcinfo > .SRCINFO

    git add PKGBUILD .SRCINFO "${ASSETS[@]}"
    git commit -a -m "Update version to $ANKAIOS_VERSION" || true
    git push

    cd "$SCRIPT_DIR"
done
