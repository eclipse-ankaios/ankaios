#!/usr/bin/bash

set -ex

pushd $(git rev-parse --show-toplevel)
sudo podman build --no-cache -t localhost/ank-persist:latest -f examples/plugins/basic_persistency/Containerfile .
popd
