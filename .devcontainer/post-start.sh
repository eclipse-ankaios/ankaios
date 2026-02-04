#!/usr/bin/env bash
set -e

# Mount make-rshared
sudo mount --make-rshared / || true

# Start containerd
nohup /workspaces/ankaios/tools/start-containerd.sh > /dev/null

# Start-up checks
if ssh -T git@github.com 2>&1 | grep -q "successfully authenticated"; then
    echo "✓ Git SSH authentication working"
else
    echo "✗ Git SSH authentication not set-up"
fi

