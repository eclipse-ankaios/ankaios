#!/usr/bin/bash

set -ex

pushd $(git rev-parse --show-toplevel)
sudo podman build --no-cache -t basic_persistency:0.1 -f examples/plugins/basic_persistency/Containerfile .
popd
