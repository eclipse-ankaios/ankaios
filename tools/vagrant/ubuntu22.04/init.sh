#!/bin/bash

apt update
apt install -y curl tmux vim uidmap

# This would install podman 3.4.4
#apt install podman

# We want a newer podman and thus install static binaries
cd /tmp
VERSION=v4.9.4
curl -fsSL -o podman-linux-amd64.tar.gz https://github.com/mgoltzsche/podman-static/releases/download/$VERSION/podman-linux-amd64.tar.gz
tar -xzf podman-linux-amd64.tar.gz
cp -r podman-linux-amd64/usr podman-linux-amd64/etc /
rm -rf podman-linux-amd64 podman-linux-amd64.tar.gz
