#!/usr/bin/bash

set -ex

pushd /home/pierrey/repos/gitrepo/ankaios
sudo podman build --no-cache -t localhost/ank-persist:latest -f examples/plugins/basic_persistency/Dockerfile .
popd
