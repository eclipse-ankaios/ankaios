#!/usr/bin/env bash
set -e

# Mount make-rshared
sudo mount --make-rshared / || true; sudo mount --make-rshared /run || true

# Start containerd
nohup /workspaces/ankaios/tools/start-containerd.sh > /dev/null
